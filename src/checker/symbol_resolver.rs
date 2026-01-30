//! Symbol Resolver Module
//!
//! This module contains symbol resolution methods for CheckerState
//! as part of Phase 2 architecture refactoring.
//!
//! The methods in this module handle:
//! - Symbol type resolution helpers
//! - Global intrinsic detection
//! - Symbol information queries
//! - Identifier symbol resolution
//! - Qualified name resolution
//! - Private identifier resolution
//! - Type parameter resolution
//! - Library type resolution
//! - Type query resolution
//! - Namespace member resolution
//! - Global value resolution
//! - Heritage symbol resolution
//! - Access class resolution
//!
//! This module extends CheckerState with additional methods for symbol-related
//! operations, providing cleaner APIs for common patterns.

use crate::binder::symbol_flags::CLASS;
use crate::binder::{ContainerKind, ScopeId, SymbolId, symbol_flags};
use crate::checker::state::{CheckerState, MAX_TREE_WALK_ITERATIONS};
use crate::parser::NodeIndex;
use crate::parser::syntax_kind_ext;
use crate::scanner::SyntaxKind;
use crate::solver::TypeId;
use std::sync::Arc;
use tracing::trace;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TypeSymbolResolution {
    Type(SymbolId),
    ValueOnly(SymbolId),
    NotFound,
}

// =============================================================================
// Symbol Resolution Methods
// =============================================================================

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Symbol Type Resolution
    // =========================================================================

    /// Get the type of a symbol with caching.
    ///
    /// This is a convenience wrapper around `get_type_of_symbol` that
    /// provides a clearer name for the operation.
    pub fn get_symbol_type(&mut self, sym_id: SymbolId) -> TypeId {
        self.get_type_of_symbol(sym_id)
    }

    // =========================================================================
    // Global Symbol Detection
    // =========================================================================

    /// Check if a name refers to a global intrinsic value.
    ///
    /// Returns true for names like `undefined`, `NaN`, `Infinity`, etc.
    pub fn is_global_intrinsic(&self, name: &str) -> bool {
        matches!(
            name,
            "undefined"
                | "NaN"
                | "Infinity"
                | "Math"
                | "JSON"
                | "Object"
                | "Array"
                | "String"
                | "Number"
                | "Boolean"
                | "Symbol"
                | "Date"
                | "RegExp"
                | "Error"
                | "Function"
                | "Promise"
        )
    }

    /// Check if a name refers to a global constructor.
    ///
    /// Returns true for built-in constructor names like `Object`, `Array`, etc.
    pub fn is_global_constructor(&self, name: &str) -> bool {
        matches!(
            name,
            "Object"
                | "Array"
                | "String"
                | "Number"
                | "Boolean"
                | "Symbol"
                | "Date"
                | "RegExp"
                | "Error"
                | "Function"
                | "Promise"
                | "Map"
                | "Set"
                | "WeakMap"
                | "WeakSet"
                | "Proxy"
                | "Reflect"
        )
    }

    // =========================================================================
    // Symbol Information Queries
    // =========================================================================

    /// Get the name of a symbol.
    ///
    /// Returns the symbol's name as a string, or None if the symbol doesn't exist.
    pub fn get_symbol_name(&self, sym_id: SymbolId) -> Option<String> {
        self.ctx
            .binder
            .symbols
            .get(sym_id)
            .map(|symbol| symbol.escaped_name.clone())
    }

    /// Check if a symbol is exported.
    ///
    /// Returns true if the symbol has the exported flag set.
    pub fn is_symbol_exported(&self, sym_id: SymbolId) -> bool {
        self.ctx
            .binder
            .symbols
            .get(sym_id)
            .map(|symbol| symbol.is_exported)
            .unwrap_or(false)
    }

    /// Check if a symbol is type-only (e.g., from `import type`).
    ///
    /// Returns true if the symbol has the type-only flag set.
    pub fn is_symbol_type_only(&self, sym_id: SymbolId) -> bool {
        self.ctx
            .binder
            .symbols
            .get(sym_id)
            .map(|symbol| symbol.is_type_only)
            .unwrap_or(false)
    }

    // =========================================================================
    // Symbol Property Queries
    // =========================================================================

    /// Get the value declaration of a symbol.
    ///
    /// Returns the primary value declaration node for the symbol, if any.
    pub fn get_symbol_value_declaration(
        &self,
        sym_id: SymbolId,
    ) -> Option<crate::parser::NodeIndex> {
        self.ctx.binder.symbols.get(sym_id).and_then(|symbol| {
            let decl = symbol.value_declaration;
            if decl.0 != u32::MAX { Some(decl) } else { None }
        })
    }

    /// Get all declarations for a symbol.
    ///
    /// Returns all declaration nodes associated with the symbol.
    pub fn get_symbol_declarations(&self, sym_id: SymbolId) -> Vec<crate::parser::NodeIndex> {
        self.ctx
            .binder
            .symbols
            .get(sym_id)
            .map(|symbol| symbol.declarations.clone())
            .unwrap_or_default()
    }

    /// Check if a symbol has a specific flag.
    ///
    /// Returns true if the symbol has the specified flag bit set.
    /// Returns false if the symbol doesn't exist.
    pub fn symbol_has_flag(&self, sym_id: SymbolId, flag: u32) -> bool {
        self.ctx
            .binder
            .symbols
            .get(sym_id)
            .map(|symbol| (symbol.flags & flag) != 0)
            .unwrap_or(false)
    }

    /// Safely get symbol flags, returning 0 if symbol doesn't exist.
    ///
    /// This defensive accessor prevents crashes when symbol IDs are invalid
    /// or reference symbols that don't exist in any binder.
    pub fn symbol_flags_safe(&self, sym_id: SymbolId) -> u32 {
        self.ctx
            .binder
            .symbols
            .get(sym_id)
            .map(|symbol| symbol.flags)
            .unwrap_or(0)
    }

    /// Safely get symbol flags with lib binders fallback.
    ///
    /// Returns 0 if the symbol doesn't exist in any binder.
    pub fn symbol_flags_with_libs(
        &self,
        sym_id: SymbolId,
        lib_binders: &[Arc<crate::binder::BinderState>],
    ) -> u32 {
        self.ctx
            .binder
            .get_symbol_with_libs(sym_id, lib_binders)
            .map(|symbol| symbol.flags)
            .unwrap_or(0)
    }

    // =========================================================================
    // Identifier Resolution
    // =========================================================================

    /// Collect lib binders from lib_contexts for cross-arena symbol lookup.
    /// This enables symbol resolution across lib.d.ts files when lib_binders
    /// is not populated in the binder (e.g., in the driver.rs path).
    pub(crate) fn get_lib_binders(&self) -> Vec<Arc<crate::binder::BinderState>> {
        self.ctx
            .lib_contexts
            .iter()
            .map(|lc| Arc::clone(&lc.binder))
            .collect()
    }

    /// Check if a symbol represents a class member (property, method, accessor, or constructor).
    ///
    /// This filters out instance members that cannot be accessed as standalone values.
    /// However, static members and constructors should still be accessible.
    pub(crate) fn is_class_member_symbol(flags: u32) -> bool {
        // Check if it's any kind of class member
        let is_member = (flags
            & (symbol_flags::PROPERTY
                | symbol_flags::METHOD
                | symbol_flags::GET_ACCESSOR
                | symbol_flags::SET_ACCESSOR
                | symbol_flags::CONSTRUCTOR))
            != 0;

        if !is_member {
            return false;
        }

        // Allow constructors - they represent the class itself
        if (flags & symbol_flags::CONSTRUCTOR) != 0 {
            return false;
        }

        // Allow static members - they're accessible via the class name
        if (flags & symbol_flags::STATIC) != 0 {
            return false;
        }

        // Filter out instance members (properties, methods, accessors without STATIC)
        true
    }

    /// Find the enclosing scope for a node by walking up the parent chain.
    /// Returns the first scope ID found in the binder's node_scope_ids map.
    pub(crate) fn find_enclosing_scope(&self, node_idx: NodeIndex) -> Option<ScopeId> {
        let mut current = node_idx;
        let mut iterations = 0;
        while !current.is_none() {
            iterations += 1;
            if iterations > MAX_TREE_WALK_ITERATIONS {
                // Safety limit reached - break to prevent infinite loop
                break;
            }
            if let Some(&scope_id) = self.ctx.binder.node_scope_ids.get(&current.0) {
                return Some(scope_id);
            }
            if let Some(ext) = self.ctx.arena.get_extended(current) {
                current = ext.parent;
            } else {
                break;
            }
        }

        // Only fall back to ScopeId(0) if it's a valid module scope
        // This prevents using an invalid fallback scope that could cause
        // symbols to be incorrectly found or not found
        if let Some(scope) = self.ctx.binder.scopes.first() {
            // Only return ScopeId(0) if it's a module scope (the global/file scope)
            if scope.kind == ContainerKind::Module {
                return Some(ScopeId(0));
            }
        }
        None
    }

    /// Resolve an identifier node to its symbol ID.
    ///
    /// This function walks the scope chain from the identifier's location upward,
    /// checking each scope's symbol table for the name. It also checks:
    /// - Module exports
    /// - Type parameter scope (for generic functions, classes, type aliases)
    /// - File locals (global scope from lib.d.ts)
    /// - Lib binders' file_locals
    ///
    /// Returns None if the identifier cannot be resolved to any symbol.
    pub(crate) fn resolve_identifier_symbol(&self, idx: NodeIndex) -> Option<SymbolId> {
        let node = self.ctx.arena.get(idx)?;
        let name = self.ctx.arena.get_identifier(node)?.escaped_text.as_str();

        let ignore_libs = !self.ctx.has_lib_loaded();
        // Collect lib binders for cross-arena symbol lookup
        let lib_binders = if ignore_libs {
            Vec::new()
        } else {
            self.get_lib_binders()
        };
        let should_skip_lib_symbol =
            |sym_id: SymbolId| ignore_libs && self.ctx.symbol_is_from_lib(sym_id);

        let debug = std::env::var("BIND_DEBUG").is_ok();

        // === PHASE 0: Check type parameter scope (HIGHEST PRIORITY) ===
        // Type parameters should be resolved before checking any other scope.
        // This is critical for generic functions, classes, and type aliases.
        if let Some(&type_id) = self.ctx.type_parameter_scope.get(name) {
            // Create a synthetic symbol ID for the type parameter
            // Type parameters don't have SymbolId, so we use a special encoding
            // The caller should handle this by using type_id directly
            // For now, we skip this phase and let the caller handle type parameters via lookup_type_parameter
            if debug {
                trace!(
                    name = %name,
                    type_id = ?type_id,
                    "[BIND_RESOLVE] Found in type_parameter_scope"
                );
            }
            // NOTE: Type parameters are handled separately via lookup_type_parameter
            // We don't return a SymbolId here because type parameters don't have them
            // Fall through to check regular scopes
        }

        // === PHASE 1: Initial logging ===
        if debug {
            trace!("\n[BIND_RESOLVE] ========================================");
            trace!(name = %name, idx = ?idx, "[BIND_RESOLVE] Looking up identifier");
            trace!(
                lib_contexts = self.ctx.lib_contexts.len(),
                "[BIND_RESOLVE] Lib contexts available"
            );
            trace!(
                lib_binders = lib_binders.len(),
                "[BIND_RESOLVE] Lib binders collected"
            );
            trace!(
                scopes = self.ctx.binder.scopes.len(),
                "[BIND_RESOLVE] Total scopes in binder"
            );
            trace!(
                file_locals = self.ctx.binder.file_locals.len(),
                "[BIND_RESOLVE] file_locals size"
            );
        }

        // === PHASE 2: Scope chain traversal (local -> parent -> ... -> module) ===
        if let Some(mut scope_id) = self.find_enclosing_scope(idx) {
            if debug {
                trace!(scope_id = ?scope_id, "[BIND_RESOLVE] Starting scope chain");
            }
            let require_export = false;
            let mut scope_depth = 0;
            while !scope_id.is_none() {
                scope_depth += 1;
                // Safety limit to prevent infinite loops in malformed scope chains
                if scope_depth > MAX_TREE_WALK_ITERATIONS {
                    break;
                }
                if let Some(scope) = self.ctx.binder.scopes.get(scope_id.0 as usize) {
                    if debug {
                        trace!(
                            scope_depth,
                            id = ?scope_id,
                            kind = ?scope.kind,
                            parent = ?scope.parent,
                            table_size = scope.table.len(),
                            "[BIND_RESOLVE] Scope"
                        );
                    }

                    // Check scope's local symbol table
                    if let Some(sym_id) = scope.table.get(name) {
                        if should_skip_lib_symbol(sym_id) {
                            if debug {
                                trace!(
                                    name = %name,
                                    sym_id = ?sym_id,
                                    "[BIND_RESOLVE] SKIPPED: lib symbol with noLib"
                                );
                            }
                        } else {
                            if debug {
                                trace!(
                                    name = %name,
                                    sym_id = ?sym_id,
                                    "[BIND_RESOLVE] Found in scope table"
                                );
                            }
                            // Use get_symbol_with_libs to check lib binders
                            if let Some(symbol) =
                                self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders)
                            {
                                // Defensive: Ensure symbol fields are accessible before accessing flags
                                let export_ok = !require_export
                                    || scope.kind != ContainerKind::Module
                                    || symbol.is_exported
                                    || (symbol.flags & symbol_flags::EXPORT_VALUE) != 0;
                                let is_class_member = Self::is_class_member_symbol(symbol.flags);
                                if debug {
                                    trace!(
                                        flags = format_args!("0x{:x}", symbol.flags),
                                        "[BIND_RESOLVE] Symbol flags"
                                    );
                                    trace!(
                                        is_exported = symbol.is_exported,
                                        export_ok, is_class_member, "[BIND_RESOLVE] Symbol status"
                                    );
                                }
                                if export_ok && !is_class_member {
                                    if debug {
                                        trace!(
                                            sym_id = ?sym_id,
                                            scope_id = ?scope_id,
                                            "[BIND_RESOLVE] SUCCESS: Returning from scope"
                                        );
                                    }
                                    return Some(sym_id);
                                } else if debug {
                                    trace!(
                                        export_ok,
                                        is_class_member,
                                        "[BIND_RESOLVE] SKIPPED: export_ok or class_member"
                                    );
                                }
                            } else if !require_export || scope.kind != ContainerKind::Module {
                                if debug {
                                    trace!(
                                        name = %name,
                                        scope_id = ?scope_id,
                                        "[BIND_RESOLVE] SUCCESS: Found in scope (no symbol data)"
                                    );
                                }
                                return Some(sym_id);
                            } else if debug {
                                trace!(
                                    "[BIND_RESOLVE] SKIPPED: No symbol data and require_export or module scope"
                                );
                            }
                        }
                    }

                    // Check module exports
                    if scope.kind == ContainerKind::Module {
                        if debug {
                            trace!(
                                container_node = ?scope.container_node,
                                "[BIND_RESOLVE] Checking module exports"
                            );
                        }
                        if let Some(container_sym_id) =
                            self.ctx.binder.get_node_symbol(scope.container_node)
                        {
                            if debug {
                                trace!(
                                    container_sym_id = ?container_sym_id,
                                    "[BIND_RESOLVE] Container symbol"
                                );
                            }
                            if let Some(container_symbol) = self
                                .ctx
                                .binder
                                .get_symbol_with_libs(container_sym_id, &lib_binders)
                            {
                                if let Some(exports) = container_symbol.exports.as_ref() {
                                    if debug {
                                        trace!(
                                            exports_count = exports.len(),
                                            "[BIND_RESOLVE] Module exports count"
                                        );
                                    }
                                    if let Some(member_id) = exports.get(name) {
                                        if should_skip_lib_symbol(member_id) {
                                            if debug {
                                                trace!(
                                                    name = %name,
                                                    member_id = ?member_id,
                                                    "[BIND_RESOLVE] SKIPPED: lib symbol with noLib"
                                                );
                                            }
                                        } else {
                                            if debug {
                                                trace!(
                                                    name = %name,
                                                    member_id = ?member_id,
                                                    "[BIND_RESOLVE] Found in exports"
                                                );
                                            }
                                            if let Some(member_symbol) = self
                                                .ctx
                                                .binder
                                                .get_symbol_with_libs(member_id, &lib_binders)
                                            {
                                                // Defensive: Validate symbol before accessing flags
                                                let is_class_member = Self::is_class_member_symbol(
                                                    member_symbol.flags,
                                                );
                                                if debug {
                                                    trace!(
                                                        flags = format_args!(
                                                            "0x{:x}",
                                                            member_symbol.flags
                                                        ),
                                                        is_class_member,
                                                        "[BIND_RESOLVE] Member flags"
                                                    );
                                                }
                                                if !is_class_member {
                                                    if debug {
                                                        trace!(
                                                            member_id = ?member_id,
                                                            "[BIND_RESOLVE] SUCCESS: Returning from module exports"
                                                        );
                                                    }
                                                    return Some(member_id);
                                                }
                                            } else {
                                                if debug {
                                                    trace!(
                                                        name = %name,
                                                        "[BIND_RESOLVE] SUCCESS: Found in module exports (no symbol data)"
                                                    );
                                                }
                                                return Some(member_id);
                                            }
                                        }
                                    }
                                } else if debug {
                                    trace!("[BIND_RESOLVE] Container has no exports");
                                }
                            } else if debug {
                                trace!("[BIND_RESOLVE] Could not get container symbol data");
                            }
                        } else if debug {
                            trace!("[BIND_RESOLVE] No container symbol for module");
                        }
                    }

                    let parent_id = scope.parent;
                    // Nested namespaces can reference non-exported parent members (TSC behavior).
                    scope_id = parent_id;
                } else {
                    if debug {
                        trace!(
                            scope_depth,
                            scope_id = ?scope_id,
                            "[BIND_RESOLVE] INVALID scope_id - breaking"
                        );
                    }
                    break;
                }
            }
            if debug {
                trace!(scope_depth, "[BIND_RESOLVE] Exhausted scope chain");
            }
        } else if debug {
            trace!(idx = ?idx, "[BIND_RESOLVE] No enclosing scope found for node");
        }

        // === PHASE 3: Check file_locals (global scope from lib.d.ts) ===
        if debug {
            trace!(
                file_locals_count = self.ctx.binder.file_locals.len(),
                "[BIND_RESOLVE] Checking file_locals"
            );
        }

        if let Some(sym_id) = self.ctx.binder.file_locals.get(name) {
            if should_skip_lib_symbol(sym_id) {
                if debug {
                    trace!(
                        name = %name,
                        sym_id = ?sym_id,
                        "[BIND_RESOLVE] SKIPPED: lib symbol with noLib"
                    );
                }
            } else {
                if debug {
                    trace!(
                        name = %name,
                        sym_id = ?sym_id,
                        "[BIND_RESOLVE] Found in file_locals"
                    );
                }
                // Use get_symbol_with_libs to check lib binders
                if let Some(symbol) = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders) {
                    let is_class_member = Self::is_class_member_symbol(symbol.flags);
                    if debug {
                        trace!(
                            flags = format_args!("0x{:x}", symbol.flags),
                            is_class_member, "[BIND_RESOLVE] Symbol flags"
                        );
                    }
                    if !is_class_member {
                        if debug {
                            trace!(
                                sym_id = ?sym_id,
                                "[BIND_RESOLVE] SUCCESS: Returning from file_locals"
                            );
                        }
                        return Some(sym_id);
                    } else if debug {
                        trace!("[BIND_RESOLVE] SKIPPED: is_class_member");
                    }
                } else {
                    if debug {
                        trace!(
                            name = %name,
                            "[BIND_RESOLVE] SUCCESS: Found in file_locals (no symbol data)"
                        );
                    }
                    return Some(sym_id);
                }
            }
        }

        // === PHASE 4: Check lib binders' file_locals directly ===
        // Skip this phase if lib symbols are merged - they're all in file_locals already
        // with unique remapped IDs (no collision risk).
        let skip_lib_binder_scan = self.ctx.binder.lib_symbols_are_merged();
        if debug {
            trace!(
                lib_binders_count = lib_binders.len(),
                skip_lib_binder_scan, "[BIND_RESOLVE] Checking lib binders' file_locals"
            );
        }
        if !skip_lib_binder_scan {
            for (i, lib_binder) in lib_binders.iter().enumerate() {
                if debug {
                    trace!(
                        lib_index = i,
                        file_locals_size = lib_binder.file_locals.len(),
                        "[BIND_RESOLVE] Lib binder file_locals"
                    );
                }
                if let Some(sym_id) = lib_binder.file_locals.get(name) {
                    if should_skip_lib_symbol(sym_id) {
                        if debug {
                            trace!(
                                name = %name,
                                lib_index = i,
                                sym_id = ?sym_id,
                                "[BIND_RESOLVE] SKIPPED: lib symbol with noLib"
                            );
                        }
                        continue;
                    }
                    if debug {
                        trace!(
                            name = %name,
                            lib_index = i,
                            sym_id = ?sym_id,
                            "[BIND_RESOLVE] Found in lib binder"
                        );
                    }

                    // Try to get symbol data with cross-arena resolution
                    // This handles cases where lib symbols reference other arenas
                    let symbol_opt = lib_binder.get_symbol_with_libs(sym_id, &lib_binders);

                    if let Some(symbol) = symbol_opt {
                        let is_class_member = Self::is_class_member_symbol(symbol.flags);
                        if debug {
                            trace!(
                                flags = format_args!("0x{:x}", symbol.flags),
                                is_class_member, "[BIND_RESOLVE] Symbol flags"
                            );
                        }
                        // For lib binders, be more permissive with class members
                        // Intrinsic types (Object, Array, etc.) may have class member flags
                        // but should still be accessible as global values
                        if !is_class_member || (symbol.flags & symbol_flags::EXPORT_VALUE) != 0 {
                            if debug {
                                trace!(
                                    sym_id = ?sym_id,
                                    lib_index = i,
                                    "[BIND_RESOLVE] SUCCESS: Returning from lib binder"
                                );
                            }
                            return Some(sym_id);
                        } else if debug {
                            trace!("[BIND_RESOLVE] SKIPPED: is_class_member without EXPORT_VALUE");
                        }
                    } else {
                        // No symbol data available - return sym_id anyway
                        // This handles cross-arena references and ambient declarations
                        if debug {
                            trace!(
                                name = %name,
                                lib_index = i,
                                "[BIND_RESOLVE] SUCCESS: Found in lib binder (no symbol data)"
                            );
                        }
                        return Some(sym_id);
                    }
                }
            }
        } // end if !skip_lib_binder_scan

        // === PHASE 5: Symbol not found - diagnostic dump ===
        if debug {
            trace!(
                name = %name,
                "[BIND_RESOLVE] FAILED: NOT FOUND in any location"
            );
            trace!("[BIND_RESOLVE] Diagnostic dump");
            trace!(
                scope_chain_levels = self.find_enclosing_scope(idx).map_or(0, |s| {
                    let mut count = 0;
                    let mut sid = s;
                    while !sid.is_none() {
                        if let Some(scope) = self.ctx.binder.scopes.get(sid.0 as usize) {
                            count += 1;
                            sid = scope.parent;
                        } else {
                            break;
                        }
                    }
                    count
                }),
                "[BIND_RESOLVE] Searched scope chain levels"
            );
            trace!(
                file_locals_entries = self.ctx.binder.file_locals.len(),
                "[BIND_RESOLVE] Searched file_locals"
            );
            trace!(
                lib_binders_count = lib_binders.len(),
                "[BIND_RESOLVE] Searched lib binders"
            );

            // Dump file_locals for debugging (if not too large)
            if self.ctx.binder.file_locals.len() < 50 {
                trace!("[BIND_RESOLVE] Main binder file_locals:");
                for (n, id) in self.ctx.binder.file_locals.iter() {
                    trace!(name = %n, id = ?id, "[BIND_RESOLVE] file_local");
                }
            } else {
                trace!(
                    file_locals_count = self.ctx.binder.file_locals.len(),
                    "[BIND_RESOLVE] file_locals too large to dump"
                );
            }

            // Sample lib binder file_locals
            for (i, lib_binder) in lib_binders.iter().enumerate() {
                if lib_binder.file_locals.len() < 30 {
                    trace!(lib_index = i, "[BIND_RESOLVE] Lib binder file_locals");
                    for (n, id) in lib_binder.file_locals.iter() {
                        trace!(name = %n, id = ?id, "[BIND_RESOLVE] file_local");
                    }
                } else {
                    trace!(
                        lib_index = i,
                        count = lib_binder.file_locals.len(),
                        "[BIND_RESOLVE] Lib binder has many file_locals (sampling first 10)"
                    );
                    for (n, id) in lib_binder.file_locals.iter().take(10) {
                        trace!(name = %n, id = ?id, "[BIND_RESOLVE] file_local");
                    }
                }
            }
            trace!("[BIND_RESOLVE] ========================================\n");
        }

        None
    }

    /// Resolve an identifier symbol for type positions, skipping value-only symbols.
    pub(crate) fn resolve_identifier_symbol_in_type_position(
        &self,
        idx: NodeIndex,
    ) -> TypeSymbolResolution {
        let node = match self.ctx.arena.get(idx) {
            Some(node) => node,
            None => return TypeSymbolResolution::NotFound,
        };
        let name = match self.ctx.arena.get_identifier(node) {
            Some(ident) => ident.escaped_text.as_str(),
            None => return TypeSymbolResolution::NotFound,
        };

        let ignore_libs = !self.ctx.has_lib_loaded();
        // Collect lib binders for cross-arena symbol lookup
        let lib_binders = if ignore_libs {
            Vec::new()
        } else {
            self.get_lib_binders()
        };
        let should_skip_lib_symbol =
            |sym_id: SymbolId| ignore_libs && self.ctx.symbol_is_from_lib(sym_id);
        let mut value_only_candidate = None;

        let mut accept_type_symbol = |sym_id: SymbolId| -> bool {
            if should_skip_lib_symbol(sym_id) {
                return false;
            }
            // Get symbol flags to check for special cases
            let flags = self
                .ctx
                .binder
                .get_symbol_with_libs(sym_id, &lib_binders)
                .map(|s| s.flags)
                .unwrap_or(0);

            // Namespaces and modules are value-only but should be allowed in type position
            // because they can contain types (e.g., MyNamespace.ValueInterface)
            let is_namespace_or_module =
                (flags & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE)) != 0;

            if is_namespace_or_module {
                return true;
            }

            let is_value_only = (self.alias_resolves_to_value_only(sym_id, None)
                || self.symbol_is_value_only(sym_id, None))
                && !self.symbol_is_type_only(sym_id, None);
            if is_value_only {
                if value_only_candidate.is_none() {
                    value_only_candidate = Some(sym_id);
                }
                return false;
            }
            true
        };

        // === PHASE 1: Scope chain traversal (local -> parent -> ... -> module) ===
        if let Some(mut scope_id) = self.find_enclosing_scope(idx) {
            let require_export = false;
            let mut scope_depth = 0;
            while !scope_id.is_none() {
                scope_depth += 1;
                if scope_depth > MAX_TREE_WALK_ITERATIONS {
                    break;
                }
                if let Some(scope) = self.ctx.binder.scopes.get(scope_id.0 as usize) {
                    // Check scope's local symbol table
                    if let Some(sym_id) = scope.table.get(name) {
                        if let Some(symbol) =
                            self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders)
                        {
                            let export_ok = !require_export
                                || scope.kind != ContainerKind::Module
                                || symbol.is_exported
                                || (symbol.flags & symbol_flags::EXPORT_VALUE) != 0;
                            let is_class_member = Self::is_class_member_symbol(symbol.flags);
                            if export_ok && !is_class_member && accept_type_symbol(sym_id) {
                                return TypeSymbolResolution::Type(sym_id);
                            }
                        } else if !require_export || scope.kind != ContainerKind::Module {
                            return TypeSymbolResolution::Type(sym_id);
                        }
                    }

                    // Check module exports
                    if scope.kind == ContainerKind::Module {
                        if let Some(container_sym_id) =
                            self.ctx.binder.get_node_symbol(scope.container_node)
                        {
                            if let Some(container_symbol) = self
                                .ctx
                                .binder
                                .get_symbol_with_libs(container_sym_id, &lib_binders)
                            {
                                if let Some(exports) = container_symbol.exports.as_ref() {
                                    if let Some(member_id) = exports.get(name) {
                                        if let Some(member_symbol) = self
                                            .ctx
                                            .binder
                                            .get_symbol_with_libs(member_id, &lib_binders)
                                        {
                                            let is_class_member =
                                                Self::is_class_member_symbol(member_symbol.flags);
                                            if !is_class_member && accept_type_symbol(member_id) {
                                                return TypeSymbolResolution::Type(member_id);
                                            }
                                        } else {
                                            return TypeSymbolResolution::Type(member_id);
                                        }
                                    }
                                }
                            }
                        }
                    }

                    scope_id = scope.parent;
                } else {
                    break;
                }
            }
        }

        // === PHASE 2: Check file_locals (global scope from lib.d.ts) ===
        if let Some(sym_id) = self.ctx.binder.file_locals.get(name) {
            if let Some(symbol) = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders) {
                let is_class_member = Self::is_class_member_symbol(symbol.flags);
                if !is_class_member && accept_type_symbol(sym_id) {
                    return TypeSymbolResolution::Type(sym_id);
                }
            }
        }

        // === PHASE 3: Check lib binders' file_locals directly ===
        // Skip this phase if lib symbols are merged - they're all in file_locals already
        // with unique remapped IDs (no collision risk).
        if !self.ctx.binder.lib_symbols_are_merged() {
            for lib_binder in &lib_binders {
                if let Some(sym_id) = lib_binder.file_locals.get(name) {
                    let symbol_opt = lib_binder.get_symbol_with_libs(sym_id, &lib_binders);
                    if let Some(symbol) = symbol_opt {
                        let is_class_member = Self::is_class_member_symbol(symbol.flags);
                        if !is_class_member || (symbol.flags & symbol_flags::EXPORT_VALUE) != 0 {
                            if accept_type_symbol(sym_id) {
                                return TypeSymbolResolution::Type(sym_id);
                            }
                        }
                    } else {
                        return TypeSymbolResolution::Type(sym_id);
                    }
                }
            }
        }

        if let Some(value_only) = value_only_candidate {
            TypeSymbolResolution::ValueOnly(value_only)
        } else {
            TypeSymbolResolution::NotFound
        }
    }

    /// Resolve a private identifier to its symbols across class scopes.
    ///
    /// Private identifiers (e.g., `#foo`) are only valid within class bodies.
    /// This function walks the scope chain and collects all symbols with the
    /// matching private name from class scopes.
    ///
    /// Returns a tuple of (symbols_found, saw_class_scope) where:
    /// - symbols_found: Vec of SymbolIds for all matching private members
    /// - saw_class_scope: true if any class scope was encountered
    pub(crate) fn resolve_private_identifier_symbols(
        &self,
        idx: NodeIndex,
    ) -> (Vec<SymbolId>, bool) {
        let node = match self.ctx.arena.get(idx) {
            Some(node) => node,
            None => return (Vec::new(), false),
        };
        let name = match self.ctx.arena.get_identifier(node) {
            Some(ident) => ident.escaped_text.as_str(),
            None => return (Vec::new(), false),
        };

        let mut symbols = Vec::new();
        let mut saw_class_scope = false;
        let Some(mut scope_id) = self.find_enclosing_scope(idx) else {
            return (symbols, saw_class_scope);
        };

        let mut iterations = 0;
        while !scope_id.is_none() {
            iterations += 1;
            if iterations > MAX_TREE_WALK_ITERATIONS {
                break;
            }
            let Some(scope) = self.ctx.binder.scopes.get(scope_id.0 as usize) else {
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

    /// Resolve a qualified name or identifier to a symbol ID.
    ///
    /// Handles both simple identifiers and qualified names (e.g., `A.B.C`).
    /// Also resolves through alias symbols (imports).
    pub(crate) fn resolve_qualified_symbol(&self, idx: NodeIndex) -> Option<SymbolId> {
        let mut visited_aliases = Vec::new();
        self.resolve_qualified_symbol_inner(idx, &mut visited_aliases, 0)
    }

    /// Resolve a qualified name or identifier for type positions.
    pub(crate) fn resolve_qualified_symbol_in_type_position(
        &self,
        idx: NodeIndex,
    ) -> TypeSymbolResolution {
        let mut visited_aliases = Vec::new();
        self.resolve_qualified_symbol_inner_in_type_position(idx, &mut visited_aliases, 0)
    }

    /// Inner implementation of qualified symbol resolution for type positions.
    pub(crate) fn resolve_qualified_symbol_inner_in_type_position(
        &self,
        idx: NodeIndex,
        visited_aliases: &mut Vec<SymbolId>,
        depth: usize,
    ) -> TypeSymbolResolution {
        // Prevent stack overflow from deeply nested qualified names
        const MAX_QUALIFIED_NAME_DEPTH: usize = 128;
        if depth >= MAX_QUALIFIED_NAME_DEPTH {
            return TypeSymbolResolution::NotFound;
        }

        let node = match self.ctx.arena.get(idx) {
            Some(node) => node,
            None => return TypeSymbolResolution::NotFound,
        };

        if node.kind == SyntaxKind::Identifier as u16 {
            return match self.resolve_identifier_symbol_in_type_position(idx) {
                TypeSymbolResolution::Type(sym_id) => {
                    let Some(sym_id) = self.resolve_alias_symbol(sym_id, visited_aliases) else {
                        return TypeSymbolResolution::NotFound;
                    };
                    TypeSymbolResolution::Type(sym_id)
                }
                other => other,
            };
        }

        if node.kind == SyntaxKind::StringLiteral as u16
            || node.kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16
        {
            let Some(literal) = self.ctx.arena.get_literal(node) else {
                return TypeSymbolResolution::NotFound;
            };
            if let Some(sym_id) = self.ctx.binder.file_locals.get(&literal.text) {
                let is_value_only = (self
                    .alias_resolves_to_value_only(sym_id, Some(&literal.text))
                    || self.symbol_is_value_only(sym_id, Some(&literal.text)))
                    && !self.symbol_is_type_only(sym_id, Some(&literal.text));
                if is_value_only {
                    return TypeSymbolResolution::ValueOnly(sym_id);
                }
                let Some(sym_id) = self.resolve_alias_symbol(sym_id, visited_aliases) else {
                    return TypeSymbolResolution::NotFound;
                };
                return TypeSymbolResolution::Type(sym_id);
            }
            return TypeSymbolResolution::NotFound;
        }

        if node.kind != crate::parser::syntax_kind_ext::QUALIFIED_NAME {
            return TypeSymbolResolution::NotFound;
        }

        let qn = match self.ctx.arena.get_qualified_name(node) {
            Some(qn) => qn,
            None => return TypeSymbolResolution::NotFound,
        };
        let left_sym = match self.resolve_qualified_symbol_inner_in_type_position(
            qn.left,
            visited_aliases,
            depth + 1,
        ) {
            TypeSymbolResolution::Type(sym_id) => sym_id,
            other => return other,
        };
        let Some(left_sym) = self.resolve_alias_symbol(left_sym, visited_aliases) else {
            return TypeSymbolResolution::NotFound;
        };
        let right_name = match self
            .ctx
            .arena
            .get(qn.right)
            .and_then(|node| self.ctx.arena.get_identifier(node))
            .map(|ident| ident.escaped_text.as_str())
        {
            Some(name) => name,
            None => return TypeSymbolResolution::NotFound,
        };

        let Some(left_symbol) = self.ctx.binder.get_symbol(left_sym) else {
            return TypeSymbolResolution::NotFound;
        };
        let Some(exports) = left_symbol.exports.as_ref() else {
            return TypeSymbolResolution::NotFound;
        };

        // First try direct exports
        if let Some(member_sym) = exports.get(right_name) {
            let is_value_only = (self.alias_resolves_to_value_only(member_sym, Some(right_name))
                || self.symbol_is_value_only(member_sym, Some(right_name)))
                && !self.symbol_is_type_only(member_sym, Some(right_name));
            if is_value_only {
                return TypeSymbolResolution::ValueOnly(member_sym);
            }
            let Some(member_sym) = self.resolve_alias_symbol(member_sym, visited_aliases) else {
                return TypeSymbolResolution::NotFound;
            };
            return TypeSymbolResolution::Type(member_sym);
        }

        // If not found in direct exports, check for re-exports
        if let Some(ref module_specifier) = left_symbol.import_module {
            if let Some(reexported_sym) =
                self.resolve_reexported_member_symbol(module_specifier, right_name, visited_aliases)
            {
                let is_value_only = (self
                    .alias_resolves_to_value_only(reexported_sym, Some(right_name))
                    || self.symbol_is_value_only(reexported_sym, Some(right_name)))
                    && !self.symbol_is_type_only(reexported_sym, Some(right_name));
                if is_value_only {
                    return TypeSymbolResolution::ValueOnly(reexported_sym);
                }
                return TypeSymbolResolution::Type(reexported_sym);
            }
        }

        TypeSymbolResolution::NotFound
    }

    /// Inner implementation of qualified symbol resolution with cycle detection.
    pub(crate) fn resolve_qualified_symbol_inner(
        &self,
        idx: NodeIndex,
        visited_aliases: &mut Vec<SymbolId>,
        depth: usize,
    ) -> Option<SymbolId> {
        // Prevent stack overflow from deeply nested qualified names
        const MAX_QUALIFIED_NAME_DEPTH: usize = 128;
        if depth >= MAX_QUALIFIED_NAME_DEPTH {
            return None;
        }

        let node = self.ctx.arena.get(idx)?;

        if node.kind == SyntaxKind::Identifier as u16 {
            let sym_id = self.resolve_identifier_symbol(idx)?;
            return self.resolve_alias_symbol(sym_id, visited_aliases);
        }

        if node.kind == SyntaxKind::StringLiteral as u16
            || node.kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16
        {
            let literal = self.ctx.arena.get_literal(node)?;
            if let Some(sym_id) = self.ctx.binder.file_locals.get(&literal.text) {
                return self.resolve_alias_symbol(sym_id, visited_aliases);
            }
            return None;
        }

        if node.kind != crate::parser::syntax_kind_ext::QUALIFIED_NAME {
            return None;
        }

        let qn = self.ctx.arena.get_qualified_name(node)?;
        let left_sym = self.resolve_qualified_symbol_inner(qn.left, visited_aliases, depth + 1)?;
        let left_sym = self.resolve_alias_symbol(left_sym, visited_aliases)?;
        let right_name = self
            .ctx
            .arena
            .get(qn.right)
            .and_then(|node| self.ctx.arena.get_identifier(node))
            .map(|ident| ident.escaped_text.as_str())?;

        let left_symbol = self.ctx.binder.get_symbol(left_sym)?;
        let exports = left_symbol.exports.as_ref()?;

        // First try direct exports
        if let Some(member_sym) = exports.get(right_name) {
            return self.resolve_alias_symbol(member_sym, visited_aliases);
        }

        // If not found in direct exports, check for re-exports
        // This handles cases like: export { foo } from './bar'
        if let Some(ref module_specifier) = left_symbol.import_module {
            if let Some(reexported_sym) =
                self.resolve_reexported_member_symbol(module_specifier, right_name, visited_aliases)
            {
                return Some(reexported_sym);
            }
        }

        None
    }

    /// Resolve a re-exported member symbol by following re-export chains.
    ///
    /// This function handles cases where a namespace member is re-exported from
    /// another module using `export { foo } from './bar'` or `export * from './bar'`.
    pub(crate) fn resolve_reexported_member_symbol(
        &self,
        module_specifier: &str,
        member_name: &str,
        visited_aliases: &mut Vec<SymbolId>,
    ) -> Option<SymbolId> {
        let mut visited_modules = rustc_hash::FxHashSet::default();
        self.resolve_reexported_member_symbol_inner(
            module_specifier,
            member_name,
            visited_aliases,
            &mut visited_modules,
        )
    }

    /// Inner implementation with cycle detection for module re-exports.
    fn resolve_reexported_member_symbol_inner(
        &self,
        module_specifier: &str,
        member_name: &str,
        visited_aliases: &mut Vec<SymbolId>,
        visited_modules: &mut rustc_hash::FxHashSet<(String, String)>,
    ) -> Option<SymbolId> {
        // Cycle detection: check if we've already visited this (module, member) pair
        let key = (module_specifier.to_string(), member_name.to_string());
        if visited_modules.contains(&key) {
            return None;
        }
        visited_modules.insert(key);

        // First, check if it's a direct export from this module
        if let Some(module_exports) = self.ctx.binder.module_exports.get(module_specifier) {
            if let Some(sym_id) = module_exports.get(member_name) {
                return self.resolve_alias_symbol(sym_id, visited_aliases);
            }
        }

        // Check for named re-exports: `export { foo } from 'bar'`
        if let Some(file_reexports) = self.ctx.binder.reexports.get(module_specifier) {
            if let Some((source_module, original_name)) = file_reexports.get(member_name) {
                let name_to_lookup = original_name.as_deref().unwrap_or(member_name);
                return self.resolve_reexported_member_symbol_inner(
                    source_module,
                    name_to_lookup,
                    visited_aliases,
                    visited_modules,
                );
            }
        }

        // Check for wildcard re-exports: `export * from 'bar'`
        // TSC behavior: If two `export *` declarations export the same name,
        // that name is considered AMBIGUOUS and is NOT exported
        // (unless explicitly re-exported by name, which is checked above).
        if let Some(source_modules) = self.ctx.binder.wildcard_reexports.get(module_specifier) {
            let mut found_result: Option<SymbolId> = None;
            let mut found_count = 0;

            for source_module in source_modules {
                if let Some(sym_id) = self.resolve_reexported_member_symbol_inner(
                    source_module,
                    member_name,
                    visited_aliases,
                    visited_modules,
                ) {
                    found_count += 1;
                    if found_count == 1 {
                        found_result = Some(sym_id);
                    } else {
                        // Multiple sources export the same name - ambiguous, treat as not exported
                        return None;
                    }
                }
            }

            if found_result.is_some() {
                return found_result;
            }
        }

        None
    }

    // =========================================================================
    // Type Parameter Resolution
    // =========================================================================

    /// Look up a type parameter by name in the current type parameter scope.
    ///
    /// Type parameters are scoped to their declaring generic (function, class, interface, etc.).
    /// This function checks the current type parameter scope to resolve type parameter names.
    pub(crate) fn lookup_type_parameter(&self, name: &str) -> Option<TypeId> {
        self.ctx.type_parameter_scope.get(name).copied()
    }

    /// Get all type parameter bindings for passing to TypeLowering.
    ///
    /// Returns a vector of (name, TypeId) pairs for all type parameters in scope.
    pub(crate) fn get_type_param_bindings(&self) -> Vec<(crate::interner::Atom, TypeId)> {
        self.ctx
            .type_parameter_scope
            .iter()
            .map(|(name, &type_id)| (self.ctx.types.intern_string(name), type_id))
            .collect()
    }

    // =========================================================================
    // Entity Name Resolution
    // =========================================================================

    /// Get the text representation of an entity name node.
    ///
    /// Entity names can be simple identifiers or qualified names (e.g., `A.B.C`).
    /// This function recursively builds the full text representation.
    pub(crate) fn entity_name_text(&self, idx: NodeIndex) -> Option<String> {
        let node = self.ctx.arena.get(idx)?;
        if node.kind == SyntaxKind::Identifier as u16 {
            return self
                .ctx
                .arena
                .get_identifier(node)
                .map(|ident| ident.escaped_text.clone());
        }
        if node.kind == syntax_kind_ext::QUALIFIED_NAME {
            let qn = self.ctx.arena.get_qualified_name(node)?;
            let left = self.entity_name_text(qn.left)?;
            let right = self.entity_name_text(qn.right)?;
            let mut combined = String::with_capacity(left.len() + 1 + right.len());
            combined.push_str(&left);
            combined.push('.');
            combined.push_str(&right);
            return Some(combined);
        }
        None
    }

    /// Get the rightmost identifier text for an entity name.
    ///
    /// This is used to disambiguate symbol IDs that may collide across binders
    /// by matching the actual identifier text referenced in source.
    #[allow(dead_code)]
    pub(crate) fn entity_name_symbol_text(&self, idx: NodeIndex) -> Option<String> {
        let node = self.ctx.arena.get(idx)?;
        if node.kind == SyntaxKind::Identifier as u16 {
            return self
                .ctx
                .arena
                .get_identifier(node)
                .map(|ident| ident.escaped_text.clone());
        }
        if node.kind == syntax_kind_ext::QUALIFIED_NAME {
            let qn = self.ctx.arena.get_qualified_name(node)?;
            return self.entity_name_symbol_text(qn.right);
        }
        None
    }

    // =========================================================================
    // Symbol Resolution for Lowering
    // =========================================================================

    /// Resolve a type symbol for type lowering.
    ///
    /// Returns the symbol ID if the resolved symbol has the TYPE flag set.
    /// Returns None for built-in types that have special handling in TypeLowering.
    pub(crate) fn resolve_type_symbol_for_lowering(&self, idx: NodeIndex) -> Option<u32> {
        use crate::solver::types::is_compiler_managed_type;

        // Skip built-in types that have special handling in TypeLowering
        // These types use built-in TypeKey representations instead of Refs
        if let Some(node) = self.ctx.arena.get(idx)
            && let Some(ident) = self.ctx.arena.get_identifier(node)
        {
            if is_compiler_managed_type(ident.escaped_text.as_str()) {
                return None;
            }
        }

        let sym_id = match self.resolve_qualified_symbol_in_type_position(idx) {
            TypeSymbolResolution::Type(sym_id) => sym_id,
            _ => return None,
        };
        let lib_binders = self.get_lib_binders();
        let symbol = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders)?;
        if (symbol.flags & symbol_flags::TYPE) != 0 {
            Some(sym_id.0)
        } else {
            None
        }
    }

    /// Resolve a value symbol for type lowering.
    ///
    /// Returns the symbol ID if the resolved symbol has VALUE or ALIAS flags set.
    pub(crate) fn resolve_value_symbol_for_lowering(&self, idx: NodeIndex) -> Option<u32> {
        if let Some(node) = self.ctx.arena.get(idx) {
            if node.kind == SyntaxKind::Identifier as u16
                && let Some(sym_id) = self.resolve_identifier_symbol(idx)
                && self.alias_resolves_to_type_only(sym_id)
            {
                return None;
            }
            if node.kind == syntax_kind_ext::QUALIFIED_NAME {
                let mut current = idx;
                loop {
                    let Some(node) = self.ctx.arena.get(current) else {
                        break;
                    };
                    if node.kind == SyntaxKind::Identifier as u16 {
                        if let Some(sym_id) = self.resolve_identifier_symbol(current)
                            && self.alias_resolves_to_type_only(sym_id)
                        {
                            return None;
                        }
                        break;
                    }
                    if node.kind != syntax_kind_ext::QUALIFIED_NAME {
                        break;
                    }
                    let Some(qn) = self.ctx.arena.get_qualified_name(node) else {
                        break;
                    };
                    current = qn.left;
                }
            }
        }
        let sym_id = self.resolve_qualified_symbol(idx)?;
        let lib_binders = self.get_lib_binders();
        let symbol = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders)?;
        if symbol.is_type_only {
            return None;
        }
        if (symbol.flags & (symbol_flags::VALUE | symbol_flags::ALIAS)) != 0 {
            Some(sym_id.0)
        } else {
            None
        }
    }

    // =========================================================================
    // Global Symbol Resolution
    // =========================================================================

    /// Resolve a global value symbol by name from file_locals and lib binders.
    ///
    /// This is used for looking up global values like `console`, `Math`, `globalThis`, etc.
    /// It checks:
    /// 1. Local file_locals (for user-defined globals and merged lib symbols)
    /// 2. Lib binders' file_locals (only when lib_symbols_merged is false)
    pub(crate) fn resolve_global_value_symbol(&self, name: &str) -> Option<SymbolId> {
        // First check local file_locals
        if let Some(sym_id) = self.ctx.binder.file_locals.get(name) {
            return Some(sym_id);
        }

        // Skip lib binder scan if lib symbols are merged - they're all in file_locals already
        if self.ctx.binder.lib_symbols_are_merged() {
            return None;
        }

        // Legacy path: check lib binders for global symbols
        let lib_binders = self.get_lib_binders();
        for lib_binder in &lib_binders {
            if let Some(sym_id) = lib_binder.file_locals.get(name) {
                return Some(sym_id);
            }
        }

        None
    }

    // =========================================================================
    // Heritage Symbol Resolution
    // =========================================================================

    /// Resolve a heritage clause expression to its symbol.
    ///
    /// Heritage clauses appear in `extends` and `implements` clauses of classes and interfaces.
    /// This function handles:
    /// - Simple identifiers (e.g., `class B extends A`)
    /// - Qualified names (e.g., `class B extends Namespace.A`)
    /// - Property access expressions (e.g., `class B extends module.A`)
    pub(crate) fn resolve_heritage_symbol(&self, idx: NodeIndex) -> Option<SymbolId> {
        let node = self.ctx.arena.get(idx)?;

        if node.kind == SyntaxKind::Identifier as u16 {
            return self.resolve_identifier_symbol(idx);
        }

        if node.kind == syntax_kind_ext::QUALIFIED_NAME {
            return self.resolve_qualified_symbol(idx);
        }

        if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            let access = self.ctx.arena.get_access_expr(node)?;
            let left_sym = self.resolve_heritage_symbol(access.expression)?;
            let name = self
                .ctx
                .arena
                .get(access.name_or_argument)
                .and_then(|name_node| self.ctx.arena.get_identifier(name_node))
                .map(|ident| ident.escaped_text.clone())?;
            let left_symbol = self.ctx.binder.get_symbol(left_sym)?;
            let exports = left_symbol.exports.as_ref()?;
            return exports.get(&name);
        }

        None
    }

    /// Check if an expression is a property access on an unresolved import.
    ///
    /// Used to suppress TS2304 errors when TS2307 was already emitted for the module.
    pub(crate) fn is_property_access_on_unresolved_import(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };

        // Handle property access expressions (e.g., B.B in extends B.B)
        if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            let Some(access) = self.ctx.arena.get_access_expr(node) else {
                return false;
            };
            // Check if the left side is an unresolved import or a property access on one
            return self.is_unresolved_import_symbol(access.expression)
                || self.is_property_access_on_unresolved_import(access.expression);
        }

        // Handle qualified names (e.g., A.B in type position)
        if node.kind == syntax_kind_ext::QUALIFIED_NAME {
            let Some(qn) = self.ctx.arena.get_qualified_name(node) else {
                return false;
            };
            return self.is_unresolved_import_symbol(qn.left)
                || self.is_property_access_on_unresolved_import(qn.left);
        }

        // Direct identifier - check if it's an unresolved import
        if node.kind == SyntaxKind::Identifier as u16 {
            return self.is_unresolved_import_symbol(idx);
        }

        false
    }

    /// Check if an identifier refers to an unresolved import symbol.
    ///
    /// Returns true if:
    /// - The symbol is an ALIAS (import)
    /// - The imported module cannot be resolved through any of:
    ///   - module_exports
    ///   - shorthand_ambient_modules
    ///   - declared_modules
    ///   - CLI-resolved modules
    pub(crate) fn is_unresolved_import_symbol(&self, idx: NodeIndex) -> bool {
        let Some(sym_id) = self.resolve_identifier_symbol(idx) else {
            return false;
        };

        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };

        // Check if this is an ALIAS symbol (import)
        if symbol.flags & symbol_flags::ALIAS == 0 {
            return false;
        }

        // Check if it has an import_module - if so, check if that module is resolved
        if let Some(ref module_name) = symbol.import_module {
            // Check various ways a module can be resolved
            if self.ctx.binder.module_exports.contains_key(module_name) {
                return false; // Module is resolved
            }
            if self.is_ambient_module_match(module_name) {
                return false; // Ambient module pattern matches
            }
            if let Some(ref resolved) = self.ctx.resolved_modules {
                if resolved.contains(module_name) {
                    return false; // CLI resolved module
                }
            }
            // Module is not resolved - this is an unresolved import
            return true;
        }

        // For import equals declarations without import_module set,
        // check if the value_declaration is an import equals with a require
        if !symbol.value_declaration.is_none() {
            let Some(decl_node) = self.ctx.arena.get(symbol.value_declaration) else {
                return false;
            };
            if decl_node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION {
                if let Some(import) = self.ctx.arena.get_import_decl(decl_node) {
                    if let Some(ref_node) = self.ctx.arena.get(import.module_specifier) {
                        if ref_node.kind == SyntaxKind::StringLiteral as u16 {
                            if let Some(lit) = self.ctx.arena.get_literal(ref_node) {
                                let module_name = &lit.text;
                                if !self.ctx.binder.module_exports.contains_key(module_name)
                                    && !self
                                        .ctx
                                        .binder
                                        .shorthand_ambient_modules
                                        .contains(module_name)
                                    && !self.ctx.binder.declared_modules.contains(module_name)
                                {
                                    return true;
                                }
                            }
                        }
                    }
                }
            }
        }

        false
    }

    /// Check if a module specifier matches a declared or shorthand ambient module pattern.
    ///
    /// Supports simple wildcard patterns using `*` (e.g., "foo*baz", "*!text").
    pub(crate) fn is_ambient_module_match(&self, module_name: &str) -> bool {
        if self.matches_module_pattern(&self.ctx.binder.declared_modules, module_name)
            || self.matches_module_pattern(&self.ctx.binder.shorthand_ambient_modules, module_name)
        {
            return true;
        }

        // Also check module_exports keys for wildcard module declarations with bodies.
        // These are stored as exact pattern strings in module_exports.
        self.ctx
            .binder
            .module_exports
            .keys()
            .any(|pattern| Self::module_name_matches_pattern(pattern, module_name))
    }

    fn matches_module_pattern(
        &self,
        patterns: &rustc_hash::FxHashSet<String>,
        module_name: &str,
    ) -> bool {
        patterns
            .iter()
            .any(|pattern| Self::module_name_matches_pattern(pattern, module_name))
    }

    fn module_name_matches_pattern(pattern: &str, module_name: &str) -> bool {
        let pattern = pattern.trim().trim_matches('"').trim_matches('\'');
        let module_name = module_name.trim().trim_matches('"').trim_matches('\'');

        if !pattern.contains('*') {
            return pattern == module_name;
        }

        // Use globset for robust wildcard matching (handles multiple '*' correctly)
        // Allow '*' to match path separators so patterns like "*!text" match "./file!text".
        if let Ok(glob) = globset::GlobBuilder::new(pattern)
            .literal_separator(false)
            .build()
        {
            let matcher = glob.compile_matcher();
            return matcher.is_match(module_name);
        }

        false
    }

    // =========================================================================
    // Require/Import Resolution
    // =========================================================================

    /// Extract the module specifier from a require() call expression or
    /// a string literal (for import equals declarations where the parser
    /// stores only the string literal, not the full require() call).
    ///
    /// Returns the module path string (e.g., `'./util'` from `require('./util')`).
    pub(crate) fn get_require_module_specifier(&self, idx: NodeIndex) -> Option<String> {
        let node = self.ctx.arena.get(idx)?;

        // For import equals declarations, the parser stores just the string literal
        // e.g., `import x = require('./util')` has module_specifier = StringLiteral('./util')
        if node.kind == SyntaxKind::StringLiteral as u16 {
            let literal = self.ctx.arena.get_literal(node)?;
            return Some(literal.text.clone());
        }

        // Handle full require() call expression (for other contexts)
        if node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return None;
        }

        let call = self.ctx.arena.get_call_expr(node)?;
        let callee_node = self.ctx.arena.get(call.expression)?;
        let callee_ident = self.ctx.arena.get_identifier(callee_node)?;
        if callee_ident.escaped_text != "require" {
            return None;
        }

        let args = call.arguments.as_ref()?;
        let first_arg = args.nodes.first().copied()?;
        let arg_node = self.ctx.arena.get(first_arg)?;
        let literal = self.ctx.arena.get_literal(arg_node)?;
        Some(literal.text.clone())
    }

    /// Resolve a require() call to its symbol.
    ///
    /// For require() calls, we don't resolve to a single symbol.
    /// Instead, compute_type_of_symbol handles this by creating a module namespace type.
    pub(crate) fn resolve_require_call_symbol(
        &self,
        idx: NodeIndex,
        _visited_aliases: Option<&mut Vec<SymbolId>>,
    ) -> Option<SymbolId> {
        // For require() calls, we don't resolve to a single symbol.
        // Instead, compute_type_of_symbol handles this by creating a module namespace type.
        // This function now just returns None to indicate no single symbol resolution.
        let _ = self.get_require_module_specifier(idx)?;
        // Module resolution for require() is handled in compute_type_of_symbol
        // by creating an object type from module_exports.
        None
    }

    /// Check if a node is a `require()` call expression.
    ///
    /// This is used to detect import equals declarations like `import x = require('./module')`
    /// where we want to return ANY type instead of the literal string type.
    #[allow(dead_code)] // Infrastructure for module resolution
    pub(crate) fn is_require_call(&self, idx: NodeIndex) -> bool {
        let node = match self.ctx.arena.get(idx) {
            Some(n) => n,
            None => return false,
        };
        if node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return false;
        }

        let call = match self.ctx.arena.get_call_expr(node) {
            Some(c) => c,
            None => return false,
        };

        let callee_node = match self.ctx.arena.get(call.expression) {
            Some(n) => n,
            None => return false,
        };

        let callee_ident = match self.ctx.arena.get_identifier(callee_node) {
            Some(ident) => ident,
            None => return false,
        };

        callee_ident.escaped_text == "require"
    }

    // =========================================================================
    // Type Query Resolution
    // =========================================================================

    /// Find the missing left-most identifier in a type query expression.
    ///
    /// For `typeof A.B.C`, if `A` is unresolved, this returns the node for `A`.
    pub(crate) fn missing_type_query_left(&self, idx: NodeIndex) -> Option<NodeIndex> {
        let mut current = idx;
        let mut iterations = 0;
        loop {
            iterations += 1;
            if iterations > MAX_TREE_WALK_ITERATIONS {
                return None;
            }
            let node = self.ctx.arena.get(current)?;
            if node.kind == SyntaxKind::Identifier as u16 {
                if self.resolve_identifier_symbol(current).is_none() {
                    return Some(current);
                }
                return None;
            }
            if node.kind != syntax_kind_ext::QUALIFIED_NAME {
                return None;
            }
            let qn = self.ctx.arena.get_qualified_name(node)?;
            current = qn.left;
        }
    }

    /// Report a type query missing member error.
    ///
    /// For `typeof A.B` where `B` is not found in `A`'s exports, emits TS2694.
    /// Returns true if an error was reported.
    pub(crate) fn report_type_query_missing_member(&mut self, idx: NodeIndex) -> bool {
        let node = match self.ctx.arena.get(idx) {
            Some(node) => node,
            None => return false,
        };
        if node.kind != syntax_kind_ext::QUALIFIED_NAME {
            return false;
        }
        let qn = match self.ctx.arena.get_qualified_name(node) {
            Some(qn) => qn,
            None => return false,
        };

        let left_sym = match self.resolve_qualified_symbol(qn.left) {
            Some(sym) => sym,
            None => return false,
        };
        let left_symbol = match self.ctx.binder.get_symbol(left_sym) {
            Some(symbol) => symbol,
            None => return false,
        };

        let right_name = match self
            .ctx
            .arena
            .get(qn.right)
            .and_then(|node| self.ctx.arena.get_identifier(node))
            .map(|ident| ident.escaped_text.as_str())
        {
            Some(name) => name,
            None => return false,
        };

        // Check direct exports first
        if let Some(exports) = left_symbol.exports.as_ref() {
            if exports.has(right_name) {
                return false;
            }
        }

        // For classes, check if the member exists in the class's members (static members)
        // This handles `typeof C.staticMember` where C is a class
        if left_symbol.flags & CLASS != 0 {
            if let Some(members) = left_symbol.members.as_ref() {
                if members.has(right_name) {
                    return false;
                }
            }
        }

        // Check for re-exports from other modules
        // This handles cases like: export { foo } from './bar'
        if let Some(ref module_specifier) = left_symbol.import_module {
            let mut visited_aliases = Vec::new();
            if self
                .resolve_reexported_member_symbol(
                    module_specifier,
                    right_name,
                    &mut visited_aliases,
                )
                .is_some()
            {
                return false;
            }
        }

        let namespace_name = self
            .entity_name_text(qn.left)
            .unwrap_or_else(|| left_symbol.escaped_name.clone());
        self.error_namespace_no_export(&namespace_name, right_name, qn.right);
        true
    }

    // =========================================================================
    // Test Option Resolution
    // =========================================================================

    /// Parse a boolean option from test file comments.
    ///
    /// Looks for patterns like `// @key: true` or `// @key: false` in the first 32 lines.
    pub(crate) fn parse_test_option_bool(text: &str, key: &str) -> Option<bool> {
        for line in text.lines().take(32) {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let is_comment =
                trimmed.starts_with("//") || trimmed.starts_with("/*") || trimmed.starts_with('*');
            if !is_comment {
                break;
            }

            let lower = trimmed.to_ascii_lowercase();
            let Some(pos) = lower.find(key) else {
                continue;
            };
            let after_key = &lower[pos + key.len()..];
            let Some(colon_pos) = after_key.find(':') else {
                continue;
            };
            let value = after_key[colon_pos + 1..].trim();

            // Parse boolean value, handling comma-separated values like "true, false"
            // Also handle trailing commas, semicolons, and other delimiters
            let value_clean = if let Some(comma_pos) = value.find(',') {
                &value[..comma_pos]
            } else if let Some(semicolon_pos) = value.find(';') {
                &value[..semicolon_pos]
            } else {
                value
            }
            .trim();

            match value_clean {
                "true" => return Some(true),
                "false" => return Some(false),
                _ => continue,
            }
        }
        None
    }

    /// Resolve the noImplicitAny setting from source file comments.
    pub(crate) fn resolve_no_implicit_any_from_source(&self, text: &str) -> bool {
        if let Some(value) = Self::parse_test_option_bool(text, "@noimplicitany") {
            return value;
        }
        if let Some(strict) = Self::parse_test_option_bool(text, "@strict") {
            return strict;
        }
        self.ctx.no_implicit_any() // Use the value from the strict flag
    }

    /// Resolve the noImplicitReturns setting from source file comments.
    pub(crate) fn resolve_no_implicit_returns_from_source(&self, text: &str) -> bool {
        if let Some(value) = Self::parse_test_option_bool(text, "@noimplicitreturns") {
            return value;
        }
        // noImplicitReturns is NOT enabled by strict mode by default
        false
    }

    /// Resolve the useUnknownInCatchVariables setting from source file comments.
    pub(crate) fn resolve_use_unknown_in_catch_variables_from_source(&self, text: &str) -> bool {
        if let Some(value) = Self::parse_test_option_bool(text, "@useunknownincatchvariables") {
            return value;
        }
        if let Some(strict) = Self::parse_test_option_bool(text, "@strict") {
            return strict;
        }
        self.ctx.use_unknown_in_catch_variables() // Use the value from the strict flag
    }

    // =========================================================================
    // Duplicate Declaration Resolution
    // =========================================================================

    /// Resolve the declaration node for duplicate identifier checking.
    ///
    /// For some nodes (like short-hand properties), we need to walk up to find
    /// the actual declaration node to report the error on.
    pub(crate) fn resolve_duplicate_decl_node(&self, decl_idx: NodeIndex) -> Option<NodeIndex> {
        let mut current = decl_idx;
        for _ in 0..8 {
            let node = self.ctx.arena.get(current)?;
            match node.kind {
                syntax_kind_ext::VARIABLE_DECLARATION
                | syntax_kind_ext::FUNCTION_DECLARATION
                | syntax_kind_ext::CLASS_DECLARATION
                | syntax_kind_ext::INTERFACE_DECLARATION
                | syntax_kind_ext::TYPE_ALIAS_DECLARATION
                | syntax_kind_ext::ENUM_DECLARATION
                | syntax_kind_ext::GET_ACCESSOR
                | syntax_kind_ext::SET_ACCESSOR
                | syntax_kind_ext::CONSTRUCTOR => {
                    return Some(current);
                }
                _ => {
                    if let Some(ext) = self.ctx.arena.get_extended(current) {
                        current = ext.parent;
                    } else {
                        return None;
                    }
                }
            }
        }
        None
    }

    // =========================================================================
    // Class Access Resolution
    // =========================================================================

    /// Resolve the class for a member access expression.
    ///
    /// Returns the class declaration node and whether the access is on the constructor type.
    /// Used for checking private/protected member accessibility.
    pub(crate) fn resolve_class_for_access(
        &mut self,
        expr_idx: NodeIndex,
        object_type: TypeId,
    ) -> Option<(NodeIndex, bool)> {
        if self.is_this_expression(expr_idx)
            && let Some(ref class_info) = self.ctx.enclosing_class
        {
            return Some((class_info.class_idx, self.is_constructor_type(object_type)));
        }

        if self.is_super_expression(expr_idx)
            && let Some(ref class_info) = self.ctx.enclosing_class
            && let Some(base_idx) = self.get_base_class_idx(class_info.class_idx)
        {
            return Some((base_idx, self.is_constructor_type(object_type)));
        }

        if let Some(sym_id) = self.resolve_identifier_symbol(expr_idx)
            && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
            && symbol.flags & symbol_flags::CLASS != 0
            && let Some(class_idx) = self.get_class_declaration_from_symbol(sym_id)
        {
            return Some((class_idx, true));
        }

        if object_type != TypeId::ANY
            && object_type != TypeId::ERROR
            && let Some(class_idx) = self.get_class_decl_from_type(object_type)
        {
            return Some((class_idx, false));
        }

        None
    }

    /// Resolve the receiver class for a member access expression.
    ///
    /// Similar to `resolve_class_for_access`, but returns only the class node.
    /// Used for determining what class the receiver belongs to.
    pub(crate) fn resolve_receiver_class_for_access(
        &self,
        expr_idx: NodeIndex,
        object_type: TypeId,
    ) -> Option<NodeIndex> {
        if self.is_this_expression(expr_idx) || self.is_super_expression(expr_idx) {
            return self.ctx.enclosing_class.as_ref().map(|info| info.class_idx);
        }

        if let Some(sym_id) = self.resolve_identifier_symbol(expr_idx)
            && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
            && symbol.flags & symbol_flags::CLASS != 0
        {
            return self.get_class_declaration_from_symbol(sym_id);
        }

        if object_type != TypeId::ANY
            && object_type != TypeId::ERROR
            && let Some(class_idx) = self.get_class_decl_from_type(object_type)
        {
            return Some(class_idx);
        }

        None
    }
}
