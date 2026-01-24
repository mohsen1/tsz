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
//!
//! This module extends CheckerState with additional methods for symbol-related
//! operations, providing cleaner APIs for common patterns.

use crate::binder::{ContainerKind, ScopeId, SymbolId, symbol_flags};
use crate::checker::state::{CheckerState, MAX_TREE_WALK_ITERATIONS};
use crate::parser::NodeIndex;
use crate::scanner::SyntaxKind;
use crate::solver::TypeId;
use std::sync::Arc;
use tracing::trace;

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
    pub fn symbol_has_flag(&self, sym_id: SymbolId, flag: u32) -> bool {
        self.ctx
            .binder
            .symbols
            .get(sym_id)
            .map(|symbol| (symbol.flags & flag) != 0)
            .unwrap_or(false)
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
    pub(crate) fn is_class_member_symbol(flags: u32) -> bool {
        (flags
            & (symbol_flags::PROPERTY
                | symbol_flags::METHOD
                | symbol_flags::GET_ACCESSOR
                | symbol_flags::SET_ACCESSOR
                | symbol_flags::CONSTRUCTOR))
            != 0
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
    /// - File locals (global scope from lib.d.ts)
    /// - Lib binders' file_locals
    ///
    /// Returns None if the identifier cannot be resolved to any symbol.
    pub(crate) fn resolve_identifier_symbol(&self, idx: NodeIndex) -> Option<SymbolId> {
        let node = self.ctx.arena.get(idx)?;
        let name = self.ctx.arena.get_identifier(node)?.escaped_text.as_str();

        // Collect lib binders for cross-arena symbol lookup
        let lib_binders = self.get_lib_binders();

        let debug = std::env::var("BIND_DEBUG").is_ok();

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
                                            let is_class_member =
                                                Self::is_class_member_symbol(member_symbol.flags);
                                            if debug {
                                                trace!(
                                                    flags =
                                                        format_args!("0x{:x}", member_symbol.flags),
                                                    is_class_member, "[BIND_RESOLVE] Member flags"
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

        // === PHASE 4: Check lib binders' file_locals directly ===
        if debug {
            trace!(
                lib_binders_count = lib_binders.len(),
                "[BIND_RESOLVE] Checking lib binders' file_locals"
            );
        }
        for (i, lib_binder) in lib_binders.iter().enumerate() {
            if debug {
                trace!(
                    lib_index = i,
                    file_locals_size = lib_binder.file_locals.len(),
                    "[BIND_RESOLVE] Lib binder file_locals"
                );
            }
            if let Some(sym_id) = lib_binder.file_locals.get(name) {
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
        self.resolve_qualified_symbol_inner(idx, &mut visited_aliases)
    }

    /// Inner implementation of qualified symbol resolution with cycle detection.
    pub(crate) fn resolve_qualified_symbol_inner(
        &self,
        idx: NodeIndex,
        visited_aliases: &mut Vec<SymbolId>,
    ) -> Option<SymbolId> {
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
        let left_sym = self.resolve_qualified_symbol_inner(qn.left, visited_aliases)?;
        let left_sym = self.resolve_alias_symbol(left_sym, visited_aliases)?;
        let right_name = self
            .ctx
            .arena
            .get(qn.right)
            .and_then(|node| self.ctx.arena.get_identifier(node))
            .map(|ident| ident.escaped_text.as_str())?;

        let left_symbol = self.ctx.binder.get_symbol(left_sym)?;
        let exports = left_symbol.exports.as_ref()?;
        let member_sym = exports.get(right_name)?;
        self.resolve_alias_symbol(member_sym, visited_aliases)
    }
}
