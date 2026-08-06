//! Name, identifier, and import resolution for `BinderState`.
//!
//! This module contains all symbol resolution methods: scope-walking identifier
//! lookup, filtered name resolution, private identifier resolution, import
//! resolution with re-export chain following, and scope discovery.

use crate::{ContainerKind, ScopeId, SymbolId, symbol_flags};
use rustc_hash::FxHashSet;
use std::sync::Arc;
use tracing::{Level, debug, span};
use tsz_parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;

use super::{BinderState, MAX_SCOPE_WALK_ITERATIONS};

/// Kill-switch for the type-only re-export resolution cache. When
/// `TSZ_DISABLE_REEXPORT_TYPE_ONLY_CACHE` is set to a non-empty, non-`0`
/// value, `resolve_import_with_reexports_type_only` bypasses its memo and
/// re-walks the chain on every call. Used to verify the cache produces
/// byte-identical diagnostics.
fn reexport_type_only_cache_disabled() -> bool {
    use std::sync::OnceLock;
    static DISABLED: OnceLock<bool> = OnceLock::new();
    *DISABLED.get_or_init(|| {
        std::env::var("TSZ_DISABLE_REEXPORT_TYPE_ONLY_CACHE")
            .map(|v| !v.is_empty() && v != "0")
            .unwrap_or(false)
    })
}

/// Kill-switch for the [`BinderState::find_enclosing_scope`] memo. When
/// `TSZ_DISABLE_ENCLOSING_SCOPE_CACHE` is set to a non-empty, non-`0` value the
/// walk neither consults nor populates the cache, walking the full ancestor
/// chain on every call. Used to prove the memo produces byte-identical
/// diagnostics: it only ever short-circuits a walk that would reach the same
/// scope anyway, so disabling it must not change any result.
fn enclosing_scope_cache_disabled() -> bool {
    use std::sync::OnceLock;
    static DISABLED: OnceLock<bool> = OnceLock::new();
    *DISABLED.get_or_init(|| {
        std::env::var("TSZ_DISABLE_ENCLOSING_SCOPE_CACHE")
            .map(|v| !v.is_empty() && v != "0")
            .unwrap_or(false)
    })
}

/// Ancestor-chain depth below which [`BinderState::find_enclosing_scope`] never
/// touches its memo. Real identifiers sit within a few nodes of their enclosing
/// scope, so the overwhelmingly common shallow walk pays zero cache cost (no
/// lock, no allocation) and is byte-for-byte the original algorithm. The memo
/// engages only for pathologically deep ancestor chains — e.g. a deeply nested
/// generic type `A<A<A<...>>>` — where the per-identifier walk is O(depth) and
/// resolving the whole expression is O(depth^2). Chosen well above any
/// hand-written nesting so legitimate code never reaches the memoized path.
const ENCLOSING_SCOPE_MEMO_THRESHOLD: usize = 32;

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
            .expect("RwLock not poisoned")
            .get(&cache_key)
        {
            return cached;
        }

        let _span = span!(Level::DEBUG, "resolve_identifier", node_idx = node_idx.0).entered();

        let result = 'resolve: {
            // Get the identifier text
            let (name, name_atom_key) = if let Some(ident) = arena.get_identifier_at(node_idx) {
                (
                    ident.escaped_text.as_str(),
                    Some((arena.atom_owner_key(), ident.atom)),
                )
            } else {
                break 'resolve None;
            };

            debug!("[RESOLVE] Looking up identifier '{}'", name);

            if let Some(mut scope_id) = self.find_enclosing_scope(arena, node_idx) {
                // Walk up the scope chain
                let mut scope_depth = 0;
                while scope_id.is_some() {
                    if let Some(scope) = self.scopes.get(scope_id.0 as usize) {
                        if let Some(sym_id) = scope.table.get_by_atom_or_name(name_atom_key, name) {
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
            if let Some(sym_id) = self.file_locals.get_by_atom_or_name(name_atom_key, name) {
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

            // NOTE: scope-chain resolution deliberately does NOT consult
            // `BinderState::program_globals`. Identifier resolution can run
            // against cross-arena nodes whose declaring-file locals are not
            // in this binder's `file_locals`; a program-global hit here would
            // shadow the declaring file's own local (e.g. a user `interface
            // EventSource` shadowing DOM's `EventSource`). Only the explicit
            // global-type accessors (`get_global_type*`) consult that table.

            // Chained lookup: check lib binders for global symbols
            // This enables resolving console, Array, Object, etc. from lib.d.ts
            for (i, lib_binder) in self.lib_binders.iter().enumerate() {
                if let Some(sym_id) = lib_binder
                    .file_locals
                    .get_by_atom_or_name(name_atom_key, name)
                {
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
            .expect("RwLock not poisoned")
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
                {
                    // Filter out enum members from Module scope exports.
                    // Enum members should only be accessible via qualified form (e.g., Enum.Member),
                    // not as unqualified names inside merged namespace bodies.
                    let is_enum_member = self
                        .symbols
                        .get(member_id)
                        .is_some_and(|s| s.flags & symbol_flags::ENUM_MEMBER != 0);
                    if !is_enum_member && let Some(found) = consider(member_id) {
                        return Some(found);
                    }
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
        let ident = arena.get_identifier(node)?;
        let name = ident.escaped_text.as_str();
        let name_atom_key = Some((arena.atom_owner_key(), ident.atom));

        let mut consider =
            |sym_id: SymbolId| -> Option<SymbolId> { accept(sym_id).then_some(sym_id) };

        // Track module scope container node during scope walk so we can check
        // its exports as a last-resort fallback (see comment below).
        let mut module_container_node = None;

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

                if let Some(sym_id) = scope.table.get_by_atom_or_name(name_atom_key, name)
                    && let Some(found) = consider(sym_id)
                {
                    return Some(found);
                }

                // Remember the module scope's container node so we can check
                // its exports as a fallback after all other resolution fails.
                // We do NOT check exports here because `export = Namespace`
                // populates exports with child namespace members, which would
                // shadow global/lib declarations (e.g., DOM `ClipboardEvent`
                // shadowed by React's `ClipboardEvent`), causing circular type
                // references and incorrect TS2430 errors.
                if scope.kind == ContainerKind::Module && module_container_node.is_none() {
                    module_container_node = Some(scope.container_node);
                }

                scope_id = scope.parent;
            }
        }

        if let Some(sym_id) = self.file_locals.get_by_atom_or_name(name_atom_key, name)
            && let Some(found) = consider(sym_id)
        {
            return Some(found);
        }

        // Pre-merge: lib symbols still live in their per-lib `file_locals`,
        // so we have to traverse `lib_binders` directly to find them.
        //
        // Post-merge: every globally-visible lib symbol has been hoisted into
        // `self.file_locals` by `merge_lib_contexts_into_binder` (Phase 3).
        // Module-scoped lib names that do NOT belong in the global scope are
        // intentionally excluded from the merge — re-walking `lib_binders`
        // would put them back. Callers that legitimately need access to those
        // module-scoped lib symbols (lib augmentation handlers, the checker's
        // type-position resolver) probe `lib_contexts.file_locals` themselves
        // and apply their own scope-filter.
        //
        // Robustness audit (PR #B, item 2 in
        // `docs/architecture/ROBUSTNESS_AUDIT_2026-04-26.md`): the gating is
        // deliberate, not a bug. See the matching comments in
        // `crates/tsz-checker/src/symbols/symbol_resolver.rs` at the
        // `resolve_identifier_symbol` and `resolve_type_symbol` fallbacks.
        if !self.lib_symbols_merged {
            for lib_binder in lib_binders {
                if let Some(sym_id) = lib_binder
                    .file_locals
                    .get_by_atom_or_name(name_atom_key, name)
                    && let Some(found) = consider(sym_id)
                {
                    return Some(found);
                }
            }
        }

        // Last-resort fallback: check module exports for names that are only
        // reachable through `export = Namespace`. This runs after scope chain,
        // file_locals, and lib resolution, so global/lib names take precedence
        // over re-exported namespace members.
        if let Some(container_node) = module_container_node
            && let Some(container_sym_id) = self.get_node_symbol(container_node)
            && let Some(container_symbol) = self.get_symbol_with_libs(container_sym_id, lib_binders)
            && let Some(exports) = container_symbol.exports.as_ref()
            && let Some(member_id) = exports.get(name)
        {
            let is_enum_member = self
                .symbols
                .get(member_id)
                .is_some_and(|s| s.flags & symbol_flags::ENUM_MEMBER != 0);
            if !is_enum_member && let Some(found) = consider(member_id) {
                return Some(found);
            }
        }

        None
    }

    /// Probe lib `file_locals` for a name that the standard resolvers cannot
    /// reach.
    ///
    /// `resolve_identifier_with_filter` and friends skip `lib_binders` once
    /// `lib_symbols_merged` is set; Phase 3 of `merge_lib_contexts_into_binder`
    /// excludes external-module lib `file_locals` (e.g. the module-scoped
    /// `class Iterator` in `es2025.iterator.d.ts`) from the global hoist so
    /// they cannot pollute the global scope. Callers that legitimately need
    /// to reach those module-scoped lib symbols (type-position resolution,
    /// lib augmentation handlers, last-resort fallbacks) go through this
    /// method instead of iterating `LibContext` themselves.
    ///
    /// Takes `&[LibContext]` rather than a flattened `&[Arc<Self>]` because
    /// several checker call sites mutate `ctx.lib_contexts` directly without
    /// going through the `lib_binders_cached`-aware setter; consuming the
    /// canonical `LibContext` slice avoids that staleness window.
    ///
    /// `accept` receives `(file_sym_id, lib_symbol_flags)`. Return
    /// `Some(id)` to accept and stop; return `None` to skip and keep
    /// iterating. The picked `SymbolId` MUST be a current-binder ID — lib
    /// binders run in their own ID space.
    pub fn resolve_name_in_lib_module_locals<F>(
        &self,
        name: &str,
        lib_contexts: &[super::LibContext],
        mut accept: F,
    ) -> Option<SymbolId>
    where
        F: FnMut(SymbolId, u32) -> Option<SymbolId>,
    {
        let file_sym_id = self.file_locals.get(name)?;
        for lib_ctx in lib_contexts {
            let Some(lib_sym_id) = lib_ctx.binder.file_locals.get(name) else {
                continue;
            };
            let flags = lib_ctx.binder.get_symbol(lib_sym_id).map_or(0, |s| s.flags);
            if let Some(resolved) = accept(file_sym_id, flags) {
                return Some(resolved);
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
    /// If `meaning_flags` is non-zero, only include symbols whose flags overlap with
    /// `meaning_flags`. Receiver-only class members are excluded because they are
    /// not bare lexical names; class type parameters remain eligible.
    pub fn collect_visible_symbol_names_filtered(
        &self,
        arena: &NodeArena,
        node_idx: NodeIndex,
        meaning_flags: u32,
    ) -> Vec<String> {
        let mut names = FxHashSet::default();

        const RECEIVER_MEMBER_FLAGS: u32 = symbol_flags::PROPERTY
            | symbol_flags::METHOD
            | symbol_flags::GET_ACCESSOR
            | symbol_flags::SET_ACCESSOR
            | symbol_flags::CONSTRUCTOR;
        const SEMANTIC_MEANING_FLAGS: u32 = symbol_flags::VALUE
            | symbol_flags::TYPE
            | symbol_flags::NAMESPACE
            | symbol_flags::ALIAS;
        let passes_filter = |sym_id: &SymbolId, class_scope: bool| -> bool {
            self.get_symbol(*sym_id).is_none_or(|symbol| {
                let lexical_flags = if class_scope {
                    symbol.flags & !RECEIVER_MEMBER_FLAGS
                } else {
                    symbol.flags
                };
                if meaning_flags == 0 {
                    lexical_flags & SEMANTIC_MEANING_FLAGS != 0
                } else {
                    lexical_flags & meaning_flags != 0
                }
            })
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
                    if passes_filter(sym_id, scope.kind == ContainerKind::Class) {
                        names.insert(symbol_name.clone());
                    }
                }
                scope_id = scope.parent;
            }
        }

        for (symbol_name, sym_id) in self.file_locals.iter() {
            if passes_filter(sym_id, false) {
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

    /// Whether a type-parameter declaration shadows a same-named parameter
    /// from a lexically enclosing generic declaration.
    ///
    /// Generic declarations own persistent binder scopes. Walking those
    /// scopes keeps declaration ownership in the binder and automatically
    /// covers every syntax form that binds type parameters. The declaration's
    /// own symbol is ignored; only a distinct same-named type-parameter symbol
    /// in a parent scope counts as lexical shadowing.
    pub fn type_parameter_has_enclosing_same_name_declaration(
        &self,
        arena: &NodeArena,
        param_name_idx: NodeIndex,
    ) -> bool {
        let Some(name) = arena
            .get_identifier_at(param_name_idx)
            .map(|identifier| identifier.escaped_text.as_str())
        else {
            return false;
        };
        let Some(param_symbol) = self.get_node_symbol(param_name_idx) else {
            return false;
        };
        let Some(mut scope_id) = self.find_enclosing_scope(arena, param_name_idx) else {
            return false;
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
            if let Some(candidate) = scope.table.get(name)
                && candidate != param_symbol
                && self
                    .get_symbol(candidate)
                    .is_some_and(|symbol| symbol.has_any_flags(symbol_flags::TYPE_PARAMETER))
            {
                return true;
            }
            scope_id = scope.parent;
        }

        false
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
                            return self.get_node_symbol(param.name);
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
        let module_specifier = sym.import_module()?;

        // For namespace/require imports (`import * as X from "m"` or
        // `import X = require("m")`), import_name is None. These resolve to the
        // module namespace, NOT to a specific named export. Only try `export=`.
        if sym.import_name().is_none() {
            return self.resolve_import_with_reexports(module_specifier, "export=");
        }

        // Determine the export name:
        // - If import_name is set, use it (for renamed imports like `import { foo as bar }`)
        // - Otherwise use the symbol's escaped_name
        let export_name = sym.import_name().unwrap_or(&sym.escaped_name);

        // Try to resolve the import, following re-export chains
        if let Some(resolved) = self.resolve_import_with_reexports(module_specifier, export_name) {
            return Some(resolved);
        }

        None
    }

    /// Resolve an import by name from a module, following re-export chains.
    ///
    /// This function handles:
    /// - Direct exports: `export { foo }` - looks up in `module_exports`
    /// - Named re-exports: `export { foo } from 'bar'` - follows the re-export mapping
    /// - Wildcard re-exports: `export * from 'bar'` - searches the re-exported module
    ///
    /// Results are cached to speed up repeated lookups (common with barrel files).
    pub fn resolve_import_with_reexports(
        &self,
        module_specifier: &str,
        export_name: &str,
    ) -> Option<SymbolId> {
        // Check cache first for fast path
        if let Some(&cached) = self
            .resolved_export_cache
            .read()
            .expect("RwLock not poisoned")
            .get(module_specifier)
            .and_then(|module_exports| module_exports.get(export_name))
        {
            return cached;
        }

        let mut visited = rustc_hash::FxHashSet::default();
        let result = self
            .resolve_import_with_reexports_inner_type_only(
                module_specifier,
                export_name,
                false,
                &mut visited,
            )
            .map(|(sym_id, _is_type_only)| sym_id);

        // Cache the result (including None for not found)
        self.resolved_export_cache
            .write()
            .expect("resolved_export_cache RwLock poisoned")
            .entry(module_specifier.to_string())
            .or_default()
            .insert(export_name.to_string(), result);
        result
    }

    /// Resolve an import by name from a module while preserving type-only wildcard provenance.
    ///
    /// Returns the resolved symbol and whether the path to it passed through a
    /// `export type * from ...` wildcard re-export.
    ///
    /// The result is memoized per binder in `resolved_export_type_only_cache`:
    /// for a fixed binder the resolution is a pure function of
    /// `(module_specifier, export_name)` over the immutable re-export tables, so
    /// repeated type-position lookups answer in O(1) instead of re-walking the
    /// named/wildcard re-export chain every time. This is the dominant cost when
    /// checking large barrel-re-export-heavy projects. Set
    /// `TSZ_DISABLE_REEXPORT_TYPE_ONLY_CACHE=1` to bypass the cache (used to
    /// verify byte-identical diagnostics).
    pub fn resolve_import_with_reexports_type_only(
        &self,
        module_specifier: &str,
        export_name: &str,
    ) -> Option<(SymbolId, bool)> {
        if reexport_type_only_cache_disabled() {
            let mut visited = rustc_hash::FxHashSet::default();
            return self.resolve_import_with_reexports_inner_type_only(
                module_specifier,
                export_name,
                false,
                &mut visited,
            );
        }

        if let Some(&cached) = self
            .resolved_export_type_only_cache
            .read()
            .expect("resolved_export_type_only_cache RwLock poisoned")
            .get(module_specifier)
            .and_then(|module_exports| module_exports.get(export_name))
        {
            return cached;
        }

        let mut visited = rustc_hash::FxHashSet::default();
        let result = self.resolve_import_with_reexports_inner_type_only(
            module_specifier,
            export_name,
            false,
            &mut visited,
        );

        self.resolved_export_type_only_cache
            .write()
            .expect("resolved_export_type_only_cache RwLock poisoned")
            .entry(module_specifier.to_string())
            .or_default()
            .insert(export_name.to_string(), result);
        result
    }

    /// Inner implementation with cycle detection for module re-exports.
    fn resolve_import_with_reexports_inner_type_only(
        &self,
        module_specifier: &str,
        export_name: &str,
        is_type_only: bool,
        visited: &mut rustc_hash::FxHashSet<(String, String)>,
    ) -> Option<(SymbolId, bool)> {
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
            // Check if the exported symbol itself was marked type-only
            // (e.g., `export type { A }` sets is_type_only on the symbol).
            let sym_is_type_only = if let Some(sym) = self.symbols.get(sym_id) {
                is_type_only || sym.is_type_only
            } else {
                is_type_only
            };
            return Some((sym_id, sym_is_type_only));
        }
        if export_name == "default"
            && let Some(module_table) = self.module_exports.get(module_specifier)
            && let Some(sym_id) = module_table.get("export=")
        {
            let sym_is_type_only = if let Some(sym) = self.symbols.get(sym_id) {
                is_type_only || sym.is_type_only
            } else {
                is_type_only
            };
            return Some((sym_id, sym_is_type_only));
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
                return self.resolve_import_with_reexports_inner_type_only(
                    source_module,
                    name_to_lookup,
                    is_type_only,
                    visited,
                );
            }
        }

        // Check for wildcard re-exports: `export * from 'bar'`
        // A module can have multiple wildcard re-exports, check all of them.
        //
        // `export *` never forwards `default` (ECMAScript's
        // `ExportStarAsNamedExports` drops the local name `default` from what a
        // wildcard export re-exports; `tsc`'s `visitExportedUnnamedExportBindings`
        // is only called when `specifier.name.escapedText !== InternalSymbolName.Default`).
        // Only a *named* re-export (`export { default } from 'bar'`, handled by
        // the `reexports` branch above) forwards a default across a barrel.
        if export_name != "default"
            && let Some(entries) = self.wildcard_reexports.get(module_specifier)
        {
            // When the caller is in value context (`is_type_only = false`), a
            // type-only path found in one wildcard source must not shadow a
            // VALUE export of the same name from a later wildcard source.
            // Type-only paths include pure type declarations
            // (TYPE_ALIAS/INTERFACE without value flags) and value-bearing
            // declarations reached through `export type *`.
            // TypeScript resolves each name in both type and value namespaces
            // independently: the VALUE wins for value-position uses.
            //
            // Strategy: collect a type-only fallback on the first pass; only return it
            // if no value export is found from any subsequent wildcard source.
            let mut type_only_fallback: Option<(SymbolId, bool)> = None;

            for (source_module, source_is_type_only) in entries {
                debug!(
                    "[RESOLVE_IMPORT] '{}' from module '{}' -> trying wildcard re-export from '{}' (type_only={})",
                    export_name, module_specifier, source_module, source_is_type_only
                );
                if let Some(result) = self.resolve_import_with_reexports_inner_type_only(
                    source_module,
                    export_name,
                    is_type_only || *source_is_type_only,
                    visited,
                ) {
                    // When in a type-only chain any match is fine; in value
                    // context prefer VALUE exports over type-only paths so a
                    // later `export * from './values'` can win over an earlier
                    // `export * from './types'` or `export type *`.
                    if is_type_only {
                        return Some(result);
                    }
                    if (result.1 || self.symbols.get(result.0).is_some_and(|s| s.is_pure_type()))
                        && type_only_fallback.is_none()
                    {
                        type_only_fallback = Some(result);
                        continue;
                    }
                    return Some(result);
                }
            }

            if let Some(fallback) = type_only_fallback {
                return Some(fallback);
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
        use tsz_parser::parser::syntax_kind_ext;

        let cache_enabled = !enclosing_scope_cache_disabled();
        // Key by the arena pointer as well as the node index: the same binder
        // can be queried against more than one arena, and node indices are
        // arena-local. Mirrors `resolved_identifier_cache`'s key shape.
        let arena_key = arena as *const NodeArena as usize;

        let mut current = node_idx;
        // Track whether we've passed through a ComputedPropertyName while walking up.
        // If so, the enclosing class member's function scope must be skipped because
        // computed property names are evaluated in the class scope, not the method scope.
        // In `[foo<T>(a)]<T>(a: T) {}`, `T` and `a` inside `[...]` must NOT resolve
        // to the method's own type parameter/parameter.
        let mut inside_computed_property_name = false;
        // Set once *any* ComputedPropertyName appears on the walk and never
        // reset. The memo is only consulted/populated while this is false: on
        // the computed-property-free prefix the enclosing scope is a pure
        // positional function of the node (no member-scope skipping), so a
        // cached entry always equals a fresh walk's result. Walks that touch a
        // ComputedPropertyName fall back to the exact original algorithm.
        let mut computed_property_on_path = false;
        let mut hops: usize = 0;
        // Non-scope ancestors past the threshold whose enclosing scope equals
        // this walk's result. Empty (and unallocated) for shallow walks.
        let mut to_memoize: Vec<u32> = Vec::new();

        // Walk up the AST using parent pointers to find the nearest scope
        let resolved = loop {
            if current.is_none() {
                break None;
            }
            let Some(node) = arena.get(current) else {
                break None;
            };

            if node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
                inside_computed_property_name = true;
                computed_property_on_path = true;
            }

            // The memo only participates once the walk is deep enough to matter
            // and while still on the computed-property-free prefix.
            let memoizable = cache_enabled
                && hops >= ENCLOSING_SCOPE_MEMO_THRESHOLD
                && !computed_property_on_path;

            if memoizable
                && let Some(&scope_id) = self
                    .find_enclosing_scope_cache
                    .read()
                    .expect("find_enclosing_scope_cache RwLock poisoned")
                    .get(&(arena_key, current.0))
            {
                break Some(scope_id);
            }

            // Check if this node creates a scope
            if let Some(&scope_id) = self.node_scope_ids.get(&current.0) {
                // If we're inside a computed property name and this scope belongs
                // to a class member (method, accessor, property), skip it.
                // The computed property name should resolve in the parent (class) scope.
                if inside_computed_property_name && Self::is_class_member_kind(node.kind) {
                    // Don't return this scope; continue walking to the class scope.
                    inside_computed_property_name = false;
                } else {
                    break Some(scope_id);
                }
            } else if memoizable {
                // A non-scope ancestor on the deep computed-property-free
                // prefix: its enclosing scope equals this walk's result.
                to_memoize.push(current.0);
            }

            if let Some(ext) = arena.get_extended(current) {
                current = ext.parent;
                hops += 1;
            } else {
                break None;
            }
        };

        // If no scope node was found, fall back to the root scope (index 0).
        let result = resolved.or_else(|| (!self.scopes.is_empty()).then_some(ScopeId(0)));

        if cache_enabled
            && !computed_property_on_path
            && !to_memoize.is_empty()
            && let Some(scope_id) = result
        {
            let mut cache = self
                .find_enclosing_scope_cache
                .write()
                .expect("find_enclosing_scope_cache RwLock poisoned");
            for node in to_memoize {
                cache.insert((arena_key, node), scope_id);
            }
        }

        result
    }

    /// Returns true if the node kind is a class member that creates its own function scope
    /// (method, constructor, accessor). These scopes must be skipped for names inside
    /// computed property names.
    const fn is_class_member_kind(kind: u16) -> bool {
        use tsz_parser::parser::syntax_kind_ext;
        kind == syntax_kind_ext::METHOD_DECLARATION
            || kind == syntax_kind_ext::CONSTRUCTOR
            || kind == syntax_kind_ext::GET_ACCESSOR
            || kind == syntax_kind_ext::SET_ACCESSOR
    }
}
