//! Name, identifier, and import resolution for `BinderState`.
//!
//! This module contains all symbol resolution methods: scope-walking identifier
//! lookup, filtered name resolution, private identifier resolution, import
//! resolution with re-export chain following, and scope discovery.

use crate::{ContainerKind, ScopeId, SymbolId};
use rustc_hash::FxHashSet;
use std::sync::Arc;
use tracing::{Level, debug, span};
use tsz_parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;

use super::state::{BinderState, MAX_SCOPE_WALK_ITERATIONS};

impl BinderState {
    // =========================================================================
    // Identifier & Name Resolution
    // =========================================================================

    /// Resolve an identifier at a given node to its `SymbolId`.
    ///
    /// This performs the full resolution chain:
    /// 1. Check the identifier resolution cache
    /// 2. Walk scope chain from the enclosing scope
    /// 3. Fall back to parameter names (for scope-less binders)
    /// 4. Check file-level locals
    /// 5. Check lib binders for global symbols
    ///
    /// Results are cached (both hits and misses) for performance.
    ///
    /// # Returns
    ///
    /// - `Some(SymbolId)` if the identifier resolves to a symbol
    /// - `None` if the identifier cannot be found
    ///
    /// # Errors
    ///
    /// This method doesn't return errors directly, but some conditions may lead to:
    /// - Resolution failures
    ///
    /// # Panics
    ///
    /// Panics if the resolved identifier cache lock is poisoned.
    pub fn resolve_identifier(&self, arena: &NodeArena, node_idx: NodeIndex) -> Option<SymbolId> {
        // Fast path: identifier resolution is pure for a fixed binder + arena.
        // Cache both hits and misses to avoid repeated scope walks in checker hot paths.
        let cache_key = (std::ptr::from_ref::<NodeArena>(arena) as usize, node_idx.0);
        if let Some(&cached) = self
            .resolved_identifier_cache
            .read()
            .unwrap()
            .get(&cache_key)
        {
            return cached;
        }

        let _span = span!(Level::DEBUG, "resolve_identifier", node_idx = node_idx.0).entered();

        let result = 'resolve: {
            // Get the identifier text
            let name = if let Some(ident) = arena.get_identifier_at(node_idx) {
                &ident.escaped_text
            } else {
                break 'resolve None;
            };

            debug!("[RESOLVE] Looking up identifier '{}'", name);

            if let Some(mut scope_id) = self.find_enclosing_scope(arena, node_idx) {
                // Walk up the scope chain
                let mut scope_depth = 0;
                while scope_id.is_some() {
                    if let Some(scope) = self.scopes.get(scope_id.0 as usize) {
                        if let Some(sym_id) = scope.table.get(name) {
                            debug!(
                                "[RESOLVE] '{}' FOUND in scope at depth {} (id={})",
                                name, scope_depth, sym_id.0
                            );
                            // Resolve import if this symbol is imported from another module
                            if let Some(resolved) = self.resolve_import_if_needed(sym_id) {
                                break 'resolve Some(resolved);
                            }
                            break 'resolve Some(sym_id);
                        }
                        scope_id = scope.parent;
                        scope_depth += 1;
                    } else {
                        break;
                    }
                }
            }

            // Fallback for bound-state binders without persistent scopes.
            if let Some(sym_id) = self.resolve_parameter_fallback(arena, node_idx, name) {
                debug!(
                    "[RESOLVE] '{}' FOUND via parameter fallback (id={})",
                    name, sym_id.0
                );
                // Resolve import if this symbol is imported from another module
                if let Some(resolved) = self.resolve_import_if_needed(sym_id) {
                    break 'resolve Some(resolved);
                }
                break 'resolve Some(sym_id);
            }

            // Finally check file locals / globals
            if let Some(sym_id) = self.file_locals.get(name) {
                debug!(
                    "[RESOLVE] '{}' FOUND in file_locals (id={})",
                    name, sym_id.0
                );
                // Resolve import if this symbol is imported from another module
                if let Some(resolved) = self.resolve_import_if_needed(sym_id) {
                    break 'resolve Some(resolved);
                }
                break 'resolve Some(sym_id);
            }

            // Chained lookup: check lib binders for global symbols
            // This enables resolving console, Array, Object, etc. from lib.d.ts
            for (i, lib_binder) in self.lib_binders.iter().enumerate() {
                if let Some(sym_id) = lib_binder.file_locals.get(name) {
                    debug!(
                        "[RESOLVE] '{}' FOUND in lib_binder[{}] (id={}) - LIB SYMBOL",
                        name, i, sym_id.0
                    );
                    // Note: lib symbols are not imports, so no need to resolve
                    break 'resolve Some(sym_id);
                }
            }

            // Symbol not found - log the failure
            debug!(
                "[RESOLVE] '{}' NOT FOUND - searched scopes, file_locals, and {} lib binders",
                name,
                self.lib_binders.len()
            );

            None
        };

        self.resolved_identifier_cache
            .write()
            .unwrap()
            .insert(cache_key, result);

        result
    }

    /// Resolve a name (string) by walking scopes from the given node and invoking a filter
    /// callback on candidates.
    ///
    /// This keeps scope traversal in the binder while allowing callers (checker) to
    /// apply contextual filtering (e.g., value-only vs type-only, class member filtering).
    pub fn resolve_name_with_filter<F>(
        &self,
        name: &str,
        arena: &NodeArena,
        node_idx: NodeIndex,
        lib_binders: &[Arc<Self>],
        mut accept: F,
    ) -> Option<SymbolId>
    where
        F: FnMut(SymbolId) -> bool,
    {
        let mut consider =
            |sym_id: SymbolId| -> Option<SymbolId> { accept(sym_id).then_some(sym_id) };

        if let Some(mut scope_id) = self.find_enclosing_scope(arena, node_idx) {
            let mut iterations = 0;
            while scope_id.is_some() {
                iterations += 1;
                if iterations > MAX_SCOPE_WALK_ITERATIONS {
                    break;
                }
                let Some(scope) = self.scopes.get(scope_id.0 as usize) else {
                    break;
                };

                if let Some(sym_id) = scope.table.get(name)
                    && let Some(found) = consider(sym_id)
                {
                    return Some(found);
                }

                if scope.kind == ContainerKind::Module
                    && let Some(container_sym_id) = self.get_node_symbol(scope.container_node)
                    && let Some(container_symbol) =
                        self.get_symbol_with_libs(container_sym_id, lib_binders)
                    && let Some(exports) = container_symbol.exports.as_ref()
                    && let Some(member_id) = exports.get(name)
                    && let Some(found) = consider(member_id)
                {
                    return Some(found);
                }

                scope_id = scope.parent;
            }
        }

        if let Some(sym_id) = self.file_locals.get(name)
            && let Some(found) = consider(sym_id)
        {
            return Some(found);
        }

        None
    }

    /// Resolve an identifier by walking scopes and invoking a filter callback on candidates.
    ///
    /// This keeps scope traversal in the binder while allowing callers (checker) to
    /// apply contextual filtering (e.g., value-only vs type-only, class member filtering).
    pub fn resolve_identifier_with_filter<F>(
        &self,
        arena: &NodeArena,
        node_idx: NodeIndex,
        lib_binders: &[Arc<Self>],
        mut accept: F,
    ) -> Option<SymbolId>
    where
        F: FnMut(SymbolId) -> bool,
    {
        let node = arena.get(node_idx)?;
        let name = if let Some(ident) = arena.get_identifier(node) {
            ident.escaped_text.as_str()
        } else {
            return None;
        };

        let mut consider =
            |sym_id: SymbolId| -> Option<SymbolId> { accept(sym_id).then_some(sym_id) };

        if let Some(mut scope_id) = self.find_enclosing_scope(arena, node_idx) {
            let mut iterations = 0;
            while scope_id.is_some() {
                iterations += 1;
                if iterations > MAX_SCOPE_WALK_ITERATIONS {
                    break;
                }
                let Some(scope) = self.scopes.get(scope_id.0 as usize) else {
                    break;
                };

                if let Some(sym_id) = scope.table.get(name)
                    && let Some(found) = consider(sym_id)
                {
                    return Some(found);
                }

                if scope.kind == ContainerKind::Module
                    && let Some(container_sym_id) = self.get_node_symbol(scope.container_node)
                    && let Some(container_symbol) =
                        self.get_symbol_with_libs(container_sym_id, lib_binders)
                    && let Some(exports) = container_symbol.exports.as_ref()
                    && let Some(member_id) = exports.get(name)
                    && let Some(found) = consider(member_id)
                {
                    return Some(found);
                }

                scope_id = scope.parent;
            }
        }

        if let Some(sym_id) = self.file_locals.get(name)
            && let Some(found) = consider(sym_id)
        {
            return Some(found);
        }

        if !self.lib_symbols_merged {
            for lib_binder in lib_binders {
                if let Some(sym_id) = lib_binder.file_locals.get(name)
                    && let Some(found) = consider(sym_id)
                {
                    return Some(found);
                }
            }
        }

        None
    }

    /// Collect visible symbol names for diagnostics and suggestions.
    /// If `meaning_flags` is non-zero, only include symbols whose flags overlap with `meaning_flags`.
    pub fn collect_visible_symbol_names(
        &self,
        arena: &NodeArena,
        node_idx: NodeIndex,
    ) -> Vec<String> {
        self.collect_visible_symbol_names_filtered(arena, node_idx, 0)
    }

    /// Collect visible symbol names filtered by meaning flags.
    /// If `meaning_flags` is non-zero, only include symbols whose flags overlap with `meaning_flags`.
    pub fn collect_visible_symbol_names_filtered(
        &self,
        arena: &NodeArena,
        node_idx: NodeIndex,
        meaning_flags: u32,
    ) -> Vec<String> {
        let mut names = FxHashSet::default();

        let passes_filter = |sym_id: &SymbolId| -> bool {
            if meaning_flags == 0 {
                return true;
            }
            self.get_symbol(*sym_id)
                .is_none_or(|sym| sym.flags & meaning_flags != 0)
        };

        if let Some(mut scope_id) = self.find_enclosing_scope(arena, node_idx) {
            let mut iterations = 0;
            while scope_id.is_some() {
                iterations += 1;
                if iterations > MAX_SCOPE_WALK_ITERATIONS {
                    break;
                }
                let Some(scope) = self.scopes.get(scope_id.0 as usize) else {
                    break;
                };
                for (symbol_name, sym_id) in scope.table.iter() {
                    if passes_filter(sym_id) {
                        names.insert(symbol_name.clone());
                    }
                }
                scope_id = scope.parent;
            }
        }

        for (symbol_name, sym_id) in self.file_locals.iter() {
            if passes_filter(sym_id) {
                names.insert(symbol_name.clone());
            }
        }

        names.into_iter().collect()
    }

    /// Resolve private identifiers (#foo) across class scopes.
    ///
    /// Returns (`symbols_found`, `saw_class_scope`).
    pub fn resolve_private_identifier_symbols(
        &self,
        arena: &NodeArena,
        node_idx: NodeIndex,
    ) -> (Vec<SymbolId>, bool) {
        let Some(node) = arena.get(node_idx) else {
            return (Vec::new(), false);
        };
        let name = match arena.get_identifier(node) {
            Some(ident) => ident.escaped_text.as_str(),
            None => return (Vec::new(), false),
        };

        let mut symbols = Vec::new();
        let mut saw_class_scope = false;
        let Some(mut scope_id) = self.find_enclosing_scope(arena, node_idx) else {
            return (symbols, saw_class_scope);
        };

        let mut iterations = 0;
        while scope_id.is_some() {
            iterations += 1;
            if iterations > MAX_SCOPE_WALK_ITERATIONS {
                break;
            }
            let Some(scope) = self.scopes.get(scope_id.0 as usize) else {
                break;
            };
            if scope.kind == ContainerKind::Class {
                saw_class_scope = true;
            }
            if let Some(sym_id) = scope.table.get(name) {
                symbols.push(sym_id);
            }
            scope_id = scope.parent;
        }

        (symbols, saw_class_scope)
    }

    // =========================================================================
    // Import Resolution
    // =========================================================================

    pub(crate) fn resolve_parameter_fallback(
        &self,
        arena: &NodeArena,
        node_idx: NodeIndex,
        name: &str,
    ) -> Option<SymbolId> {
        if self.scopes.is_empty() {
            let mut current = node_idx;
            while current.is_some() {
                let node = arena.get(current)?;
                if let Some(func) = arena.get_function(node) {
                    for &param_idx in &func.parameters.nodes {
                        let param = arena.get_parameter_at(param_idx)?;
                        let ident = arena.get_identifier_at(param.name)?;
                        if ident.escaped_text == name {
                            return self.node_symbols.get(&param.name.0).copied();
                        }
                    }
                }
                let ext = arena.get_extended(current)?;
                current = ext.parent;
            }
        }
        None
    }

    /// Resolve an imported symbol to its actual export from the source module.
    ///
    /// When a symbol is imported (e.g., `import { foo } from './file'`), the binder creates
    /// a local ALIAS symbol with `import_module` set to './file'. This method resolves that
    /// alias to the actual exported symbol from the source module by looking up `module_exports`
    /// and following re-export chains.
    ///
    /// Returns the resolved `SymbolId`, or the original `sym_id` if it's not an import or resolution fails.
    pub(crate) fn resolve_import_if_needed(&self, sym_id: SymbolId) -> Option<SymbolId> {
        // Get the symbol to check if it's an import
        let sym = self.symbols.get(sym_id)?;
        let module_specifier = sym.import_module.as_ref()?;

        // Determine the export name:
        // - If import_name is set, use it (for renamed imports like `import { foo as bar }`)
        // - Otherwise use the symbol's escaped_name
        let export_name = sym.import_name.as_ref().unwrap_or(&sym.escaped_name);

        // Try to resolve the import, following re-export chains
        self.resolve_import_with_reexports(module_specifier, export_name)
    }

    /// Resolve an import by name from a module, following re-export chains.
    ///
    /// This function handles:
    /// - Direct exports: `export { foo }` - looks up in `module_exports`
    /// - Named re-exports: `export { foo } from 'bar'` - follows the re-export mapping
    /// - Wildcard re-exports: `export * from 'bar'` - searches the re-exported module
    ///
    /// Results are cached to speed up repeated lookups (common with barrel files).
    pub(crate) fn resolve_import_with_reexports(
        &self,
        module_specifier: &str,
        export_name: &str,
    ) -> Option<SymbolId> {
        // Check cache first for fast path
        let cache_key = (module_specifier.to_string(), export_name.to_string());
        if let Some(&cached) = self.resolved_export_cache.read().unwrap().get(&cache_key) {
            return cached;
        }

        let mut visited = rustc_hash::FxHashSet::default();
        let result =
            self.resolve_import_with_reexports_inner(module_specifier, export_name, &mut visited);

        // Cache the result (including None for not found)
        self.resolved_export_cache
            .write()
            .expect("resolved_export_cache RwLock poisoned")
            .insert(cache_key, result);
        result
    }

    /// Inner implementation with cycle detection for module re-exports.
    pub(crate) fn resolve_import_with_reexports_inner(
        &self,
        module_specifier: &str,
        export_name: &str,
        visited: &mut rustc_hash::FxHashSet<(String, String)>,
    ) -> Option<SymbolId> {
        let _span =
            span!(Level::DEBUG, "resolve_import_with_reexports", %module_specifier, %export_name)
                .entered();

        // Cycle detection: check if we've already visited this (module, export) pair
        let key = (module_specifier.to_string(), export_name.to_string());
        if visited.contains(&key) {
            return None;
        }
        visited.insert(key);

        // First, check if it's a direct export from this module
        if let Some(module_table) = self.module_exports.get(module_specifier)
            && let Some(sym_id) = module_table.get(export_name)
        {
            debug!(
                "[RESOLVE_IMPORT] '{}' from module '{}' -> direct export symbol id={}",
                export_name, module_specifier, sym_id.0
            );
            return Some(sym_id);
        }

        // Not found in direct exports, check for named re-exports
        if let Some(file_reexports) = self.reexports.get(module_specifier) {
            // Check for named re-export: `export { foo } from 'bar'`
            if let Some((source_module, original_name)) = file_reexports.get(export_name) {
                let name_to_lookup = original_name.as_deref().unwrap_or(export_name);
                debug!(
                    "[RESOLVE_IMPORT] '{}' from module '{}' -> following named re-export from '{}', original name='{}'",
                    export_name, module_specifier, source_module, name_to_lookup
                );
                return self.resolve_import_with_reexports_inner(
                    source_module,
                    name_to_lookup,
                    visited,
                );
            }
        }

        // Check for wildcard re-exports: `export * from 'bar'`
        // A module can have multiple wildcard re-exports, check all of them
        if let Some(source_modules) = self.wildcard_reexports.get(module_specifier) {
            for source_module in source_modules {
                debug!(
                    "[RESOLVE_IMPORT] '{}' from module '{}' -> trying wildcard re-export from '{}'",
                    export_name, module_specifier, source_module
                );
                if let Some(result) =
                    self.resolve_import_with_reexports_inner(source_module, export_name, visited)
                {
                    return Some(result);
                }
            }
        }

        // Export not found
        debug!(
            "[RESOLVE_IMPORT] '{}' from module '{}' -> NOT FOUND",
            export_name, module_specifier
        );
        None
    }

    /// Public method for testing import resolution with reexports.
    /// This allows tests to verify that wildcard and named re-exports are properly resolved.
    pub fn resolve_import_if_needed_public(
        &self,
        module_specifier: &str,
        export_name: &str,
    ) -> Option<SymbolId> {
        self.resolve_import_with_reexports(module_specifier, export_name)
    }

    /// Resolve an import symbol to its target, following re-export chains.
    ///
    /// This is used by the checker to resolve imported symbols to their actual declarations,
    /// following both named re-exports (`export { foo } from 'bar'`) and wildcard re-exports
    /// (`export * from 'bar'`).
    ///
    /// Returns the resolved `SymbolId` if found, None otherwise.
    pub fn resolve_import_symbol(&self, sym_id: SymbolId) -> Option<SymbolId> {
        self.resolve_import_if_needed(sym_id)
    }

    // =========================================================================
    // Scope Discovery
    // =========================================================================

    /// Find the enclosing scope for a given node by walking up the AST.
    /// Returns the `ScopeId` of the nearest scope-creating ancestor node.
    pub fn find_enclosing_scope(&self, arena: &NodeArena, node_idx: NodeIndex) -> Option<ScopeId> {
        let mut current = node_idx;

        // Walk up the AST using parent pointers to find the nearest scope
        while current.is_some() {
            // Check if this node creates a scope
            if let Some(&scope_id) = self.node_scope_ids.get(&current.0) {
                return Some(scope_id);
            }

            // Move to parent node
            if let Some(_node) = arena.get(current) {
                if let Some(ext) = arena.get_extended(current) {
                    current = ext.parent;
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        // If no scope found, return the root scope (index 0) if it exists
        (!self.scopes.is_empty()).then_some(ScopeId(0))
    }
}
