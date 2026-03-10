//! Symbol resolution helpers (identifier lookup, qualified name resolution).
//! - Qualified name resolution
//! - Private identifier resolution
//! - Type parameter resolution
//! - Library type resolution
//! - Namespace member resolution
//!
//! This module extends `CheckerState` with additional methods for symbol-related
//! operations, providing cleaner APIs for common patterns.

use crate::state::CheckerState;
use std::sync::Arc;
use tracing::trace;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;
use tsz_solver::is_compiler_managed_type;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TypeSymbolResolution {
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

    // =========================================================================
    // Identifier Resolution
    // =========================================================================

    /// Collect lib binders from `lib_contexts` for cross-arena symbol lookup.
    /// This enables symbol resolution across lib.d.ts files when `lib_binders`
    /// is not populated in the binder (e.g., in the driver.rs path).
    pub(crate) fn get_lib_binders(&self) -> Vec<Arc<tsz_binder::BinderState>> {
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
    pub(crate) const fn is_class_member_symbol(flags: u32) -> bool {
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

    /// Check if a symbol is a string-literal ambient module declaration
    /// (e.g., `declare module "foobar"`). These should not be accessible as bare
    /// identifiers — only namespace declarations with identifier names
    /// (e.g., `declare namespace Foo`) should resolve in expression context.
    fn is_string_literal_module_symbol(
        &self,
        sym_id: SymbolId,
        lib_binders: &[Arc<tsz_binder::BinderState>],
    ) -> bool {
        let symbol = self.ctx.binder.get_symbol_with_libs(sym_id, lib_binders);
        let Some(symbol) = symbol else {
            return false;
        };
        // Only check symbols with MODULE flags
        if (symbol.flags & symbol_flags::MODULE) == 0 {
            return false;
        }
        // Check if ALL declarations are module declarations with string literal names
        if symbol.declarations.is_empty() {
            return false;
        }
        symbol.declarations.iter().all(|&decl_idx| {
            let Some(node) = self.ctx.arena.get(decl_idx) else {
                // Can't find node (possibly cross-file) — conservatively not a string module
                return false;
            };
            if node.kind != syntax_kind_ext::MODULE_DECLARATION {
                return false;
            }
            let Some(module) = self.ctx.arena.get_module(node) else {
                return false;
            };
            // If the name node is a StringLiteral, this is a string-literal module
            self.ctx
                .arena
                .get(module.name)
                .is_some_and(|name_node| name_node.kind == SyntaxKind::StringLiteral as u16)
        })
    }

    /// Check if a symbol is an `import =` alias that can serve as the left-hand
    /// side of a qualified type name (e.g. `import b = require("m"); b.T`).
    ///
    /// These aliases are namespace-like anchors in qualified type positions even
    /// when the alias itself is not a type. Bare uses (`let x: b`) remain
    /// invalid; this only matters when the alias is followed by `.Member`.
    fn is_import_equals_type_anchor(
        &self,
        sym_id: SymbolId,
        lib_binders: &[Arc<tsz_binder::BinderState>],
    ) -> bool {
        let Some(symbol) = self.ctx.binder.get_symbol_with_libs(sym_id, lib_binders) else {
            return false;
        };
        if (symbol.flags & symbol_flags::ALIAS) == 0 {
            return false;
        }

        let decl_idx = if symbol.value_declaration.is_some() {
            symbol.value_declaration
        } else {
            symbol
                .declarations
                .iter()
                .copied()
                .find(|idx| idx.is_some())
                .unwrap_or(NodeIndex::NONE)
        };

        decl_idx.is_some()
            && self
                .ctx
                .arena
                .get(decl_idx)
                .is_some_and(|node| node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION)
    }

    /// Resolve an identifier node to its symbol ID.
    ///
    /// This function walks the scope chain from the identifier's location upward,
    /// checking each scope's symbol table for the name. It also checks:
    /// - Module exports
    /// - Type parameter scope (for generic functions, classes, type aliases)
    /// - File locals (global scope from lib.d.ts)
    /// - Lib binders' `file_locals`
    ///
    /// Returns None if the identifier cannot be resolved to any symbol.
    pub(crate) fn resolve_identifier_symbol(&self, idx: NodeIndex) -> Option<SymbolId> {
        let result = self.resolve_identifier_symbol_inner(idx);
        if let Some(sym_id) = result {
            self.ctx.referenced_symbols.borrow_mut().insert(sym_id);
            trace!(sym_id = %sym_id.0, idx = %idx.0, "resolve_identifier_symbol: marked referenced");
        }
        result
    }

    /// Resolve identifier for write context (assignment target).
    pub(crate) fn resolve_identifier_symbol_for_write(&self, idx: NodeIndex) -> Option<SymbolId> {
        let result = self.resolve_identifier_symbol_inner(idx);
        if let Some(sym_id) = result {
            self.ctx.written_symbols.borrow_mut().insert(sym_id);
        }
        result
    }

    fn resolve_identifier_symbol_inner(&self, idx: NodeIndex) -> Option<SymbolId> {
        // Get identifier name for tracing
        let ident_name = self
            .ctx
            .arena
            .get_identifier_at(idx)
            .map(|i| i.escaped_text.as_str().to_string());

        let ignore_libs = !self.ctx.has_lib_loaded();
        let lib_binders = if ignore_libs {
            Vec::new()
        } else {
            self.get_lib_binders()
        };
        let is_from_lib = |sym_id: SymbolId| self.ctx.symbol_is_from_lib(sym_id);
        let should_skip_lib_symbol = |sym_id: SymbolId| ignore_libs && is_from_lib(sym_id);

        trace!(
            ident_name = ?ident_name,
            idx = ?idx,
            ignore_libs = ignore_libs,
            "Resolving identifier symbol"
        );

        // First try the binder's resolver which checks scope chain and file_locals
        let result = self.ctx.binder.resolve_identifier_with_filter(
            self.ctx.arena,
            idx,
            &lib_binders,
            |sym_id| {
                if should_skip_lib_symbol(sym_id) {
                    return false;
                }
                if let Some(symbol) = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders) {
                    let is_class_member = Self::is_class_member_symbol(symbol.flags);
                    if is_class_member {
                        return is_from_lib(sym_id)
                            && (symbol.flags & symbol_flags::EXPORT_VALUE) != 0;
                    }
                }
                true
            },
        );
        let result = {
            let expected_name = self
                .ctx
                .arena
                .get_identifier_at(idx)
                .map(|ident| ident.escaped_text.as_str());
            result.filter(|&sym_id| {
                let Some(expected_name) = expected_name else {
                    return false;
                };

                self.ctx
                    .binder
                    .get_symbol_with_libs(sym_id, &lib_binders)
                    .is_some_and(|symbol| symbol.escaped_name.as_str() == expected_name)
            })
        };

        // Filter out string-literal ambient module declarations (e.g. `declare module "foobar"`)
        // These should not resolve as bare identifiers — they are only reachable via import.
        let result =
            result.filter(|&sym_id| !self.is_string_literal_module_symbol(sym_id, &lib_binders));

        trace!(
            ident_name = ?ident_name,
            binder_result = ?result,
            "Binder resolution result"
        );

        // IMPORTANT: If the binder didn't find the symbol, check lib_contexts directly as a fallback.
        // The binder's method has a bug where it only queries lib_binders when lib_symbols_merged is FALSE.
        // After lib symbols are merged into the main binder, lib_symbols_merged is set to TRUE,
        // causing the binder to skip lib lookup entirely. By checking lib_contexts.file_locals
        // directly here as a fallback, we bypass that bug and ensure global symbols are always resolved.
        // This matches the pattern used successfully in generators.rs (lookup_global_type).
        if result.is_none() && !ignore_libs {
            // Get the identifier name
            let name = if let Some(ident) = self.ctx.arena.get_identifier_at(idx) {
                ident.escaped_text.as_str()
            } else {
                return None;
            };
            // Check lib_contexts directly for global symbols
            for (lib_idx, lib_ctx) in self.ctx.lib_contexts.iter().enumerate() {
                if let Some(lib_sym_id) = lib_ctx.binder.file_locals.get(name) {
                    trace!(
                        name = name,
                        lib_idx = lib_idx,
                        lib_sym_id = ?lib_sym_id,
                        "Found symbol in lib_context"
                    );
                    if !should_skip_lib_symbol(lib_sym_id) {
                        // Use file binder's sym_id for correct ID space after lib merge.
                        // Never return lib-context SymbolIds directly: they may collide with
                        // unrelated symbols in the current binder ID space.
                        let Some(file_sym_id) = self.ctx.binder.file_locals.get(name) else {
                            continue;
                        };
                        trace!(
                            name = name,
                            file_sym_id = ?file_sym_id,
                            lib_sym_id = ?lib_sym_id,
                            "Returning symbol from lib_contexts fallback"
                        );
                        return Some(file_sym_id);
                    }
                }
            }
        }

        trace!(
            ident_name = ?ident_name,
            final_result = ?result,
            "Symbol resolution final result"
        );

        if let Some(ident) = self.ctx.arena.get_identifier_at(idx)
            && let Some(found_sym_id) = result
            && self.ctx.binder.file_locals.get(ident.escaped_text.as_str()) == Some(found_sym_id)
            && let Some(ns_sym_id) = self
                .resolve_unqualified_name_in_enclosing_namespace(idx, ident.escaped_text.as_str())
            && ns_sym_id != found_sym_id
        {
            return Some(ns_sym_id);
        }

        if let Some(ident) = self.ctx.arena.get_identifier_at(idx)
            && result.is_none()
        {
            let name = ident.escaped_text.as_str();
            if let Some(sym_id) =
                self.resolve_identifier_symbol_from_all_binders(name, |sym_id, symbol| {
                    if should_skip_lib_symbol(sym_id) {
                        return false;
                    }

                    let is_class_member = Self::is_class_member_symbol(symbol.flags);
                    if is_class_member {
                        return is_from_lib(sym_id)
                            && (symbol.flags & symbol_flags::EXPORT_VALUE) != 0;
                    }
                    true
                })
            {
                return Some(sym_id);
            }

            // Cross-file namespace body fallback: if we're inside a namespace body
            // and the name wasn't found, check the merged namespace symbol's exports.
            // This handles e.g. `Point` in part2.ts referring to `Point` exported from
            // part1.ts's `namespace A`.
            if let Some(sym_id) = self.resolve_unqualified_name_in_enclosing_namespace(idx, name) {
                return Some(sym_id);
            }
        }

        trace!(
            ident_name = ?ident_name,
            final_result = ?result,
            "Symbol resolution final result"
        );

        if let Some(sym_id) = result
            && let Some(sym) = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders)
        {
            trace!(
                ident_name = ?ident_name,
                sym_id = sym_id.0,
                sym_name = sym.escaped_name.as_str(),
                sym_flags = sym.flags,
                "Symbol resolution resolved metadata"
            );
        }
        result
    }

    /// Resolve an identifier symbol for type positions, skipping value-only symbols.
    pub(crate) fn resolve_identifier_symbol_in_type_position(
        &self,
        idx: NodeIndex,
    ) -> TypeSymbolResolution {
        let result = self.resolve_identifier_symbol_in_type_position_inner(idx);
        if let TypeSymbolResolution::Type(sym_id) = result {
            self.ctx.referenced_symbols.borrow_mut().insert(sym_id);
        }
        result
    }

    /// Resolve an identifier when it appears as the left-hand side of a
    /// qualified type name (e.g. `Alias.Member`).
    ///
    /// This is slightly broader than ordinary type-position lookup because
    /// `import =` aliases act as namespace-like anchors even when the alias
    /// itself is value-only as a bare type.
    pub(crate) fn resolve_identifier_symbol_as_qualified_type_anchor(
        &self,
        idx: NodeIndex,
    ) -> Option<SymbolId> {
        let lib_binders = self.get_lib_binders();
        match self.resolve_identifier_symbol_in_type_position(idx) {
            TypeSymbolResolution::Type(sym_id) => {
                let mut visited_aliases = Vec::new();
                Some(
                    self.resolve_alias_symbol(sym_id, &mut visited_aliases)
                        .unwrap_or(sym_id),
                )
            }
            TypeSymbolResolution::ValueOnly(sym_id)
                if self.is_import_equals_type_anchor(sym_id, &lib_binders) =>
            {
                self.ctx.referenced_symbols.borrow_mut().insert(sym_id);
                let mut visited_aliases = Vec::new();
                Some(
                    self.resolve_alias_symbol(sym_id, &mut visited_aliases)
                        .unwrap_or(sym_id),
                )
            }
            TypeSymbolResolution::ValueOnly(_) | TypeSymbolResolution::NotFound => None,
        }
    }

    fn resolve_identifier_symbol_in_type_position_inner(
        &self,
        idx: NodeIndex,
    ) -> TypeSymbolResolution {
        let node = match self.ctx.arena.get(idx) {
            Some(node) => node,
            None => return TypeSymbolResolution::NotFound,
        };
        let ident = match self.ctx.arena.get_identifier(node) {
            Some(ident) => ident,
            None => return TypeSymbolResolution::NotFound,
        };
        let name = ident.escaped_text.as_str();

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

        // Check if this name exists in a local scope (namespace/module) that would shadow
        // the global lib symbol. If so, we skip the early lib_contexts check and let the
        // binder's scope-based resolution find the local symbol first.
        let name_in_local_scope = if !ignore_libs {
            self.ctx
                .binder
                .resolve_identifier_with_filter(
                    self.ctx.arena,
                    idx,
                    &lib_binders,
                    |_| true, // accept any symbol
                )
                .is_some_and(|found_sym_id| {
                    // Check if this symbol is different from the file_locals symbol.
                    // If it's different, it was found in a more local scope (namespace, etc.)
                    self.ctx.binder.file_locals.get(name) != Some(found_sym_id)
                })
        } else {
            false
        };

        // IMPORTANT: Check lib_contexts directly BEFORE calling binder's resolve_identifier_with_filter.
        // The binder's method has a bug where it only queries lib_binders when lib_symbols_merged is FALSE.
        // After lib symbols are merged into the main binder, lib_symbols_merged is set to TRUE,
        // causing the binder to skip lib lookup entirely. By checking lib_contexts.file_locals
        // directly here, we bypass that bug and ensure global type symbols are always resolved.
        // However, skip this early check when the name is declared in a local scope (namespace, etc.)
        // so that local symbols can shadow global ones.
        if !ignore_libs && !name_in_local_scope {
            for lib_ctx in &self.ctx.lib_contexts {
                if let Some(lib_sym_id) = lib_ctx.binder.file_locals.get(name) {
                    // After lib merge, the file binder has the same symbols with
                    // potentially different IDs. Use file binder's ID for returns,
                    // and skip symbols not present in current binder ID space.
                    let Some(sym_id) = self.ctx.binder.file_locals.get(name) else {
                        continue;
                    };
                    if !should_skip_lib_symbol(sym_id) {
                        // Check flags using lib binder (lib_sym_id is valid in lib binder)
                        let flags = lib_ctx.binder.get_symbol(lib_sym_id).map_or(0, |s| s.flags);

                        // Namespaces and modules are value-only but should be allowed in type position
                        let is_namespace_or_module = (flags
                            & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE))
                            != 0;

                        if is_namespace_or_module {
                            return TypeSymbolResolution::Type(sym_id);
                        }

                        // For ALIAS symbols, resolve to the target
                        if flags & symbol_flags::ALIAS != 0 {
                            let mut visited = Vec::new();
                            if let Some(target_sym_id) =
                                self.resolve_alias_symbol(sym_id, &mut visited)
                            {
                                // Check the target symbol's flags
                                let target_flags = self
                                    .ctx
                                    .binder
                                    .get_symbol_with_libs(target_sym_id, &lib_binders)
                                    .map_or(0, |s| s.flags);
                                if (target_flags
                                    & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE))
                                    != 0
                                {
                                    return TypeSymbolResolution::Type(target_sym_id);
                                }
                            }
                        }

                        // Check if this is a value-only symbol
                        let is_value_only = (self.alias_resolves_to_value_only(sym_id, None)
                            || self.symbol_is_value_only(sym_id, None))
                            && !self.symbol_is_type_only(sym_id, None);
                        if is_value_only {
                            if value_only_candidate.is_none() {
                                value_only_candidate = Some(sym_id);
                            }
                        } else {
                            // Valid type symbol found in lib
                            return TypeSymbolResolution::Type(sym_id);
                        }
                    }
                }
            }
        }

        let mut accept_type_symbol = |sym_id: SymbolId| -> bool {
            // Get symbol flags to check for special cases
            let flags = self
                .ctx
                .binder
                .get_symbol_with_libs(sym_id, &lib_binders)
                .map_or(0, |s| s.flags);

            // Namespaces and modules are value-only but should be allowed in type position
            // because they can contain types (e.g., MyNamespace.ValueInterface)
            let is_namespace_or_module =
                (flags & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE)) != 0;

            if is_namespace_or_module {
                return true;
            }

            // For ALIAS symbols (import equals declarations), resolve to the target
            // and check if it's a namespace/module
            if flags & symbol_flags::ALIAS != 0 {
                let mut visited = Vec::new();
                if let Some(target_sym_id) = self.resolve_alias_symbol(sym_id, &mut visited) {
                    let target_flags = self
                        .ctx
                        .binder
                        .get_symbol_with_libs(target_sym_id, &lib_binders)
                        .map_or(0, |s| s.flags);
                    if (target_flags
                        & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE))
                        != 0
                    {
                        return true;
                    }
                }
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

        let resolve_alias_type_position_result = |sym_id: SymbolId| {
            let mut visited_aliases = Vec::new();
            self.resolve_alias_symbol(sym_id, &mut visited_aliases)
                .map(|target_sym_id| {
                    let target_flags = self
                        .ctx
                        .binder
                        .get_symbol_with_libs(target_sym_id, &lib_binders)
                        .map_or(0, |s| s.flags);
                    let target_is_namespace_module = (target_flags
                        & (symbol_flags::MODULE
                            | symbol_flags::NAMESPACE_MODULE
                            | symbol_flags::VALUE_MODULE))
                        != 0;
                    let target_is_value_only = (self
                        .alias_resolves_to_value_only(target_sym_id, None)
                        || self.symbol_is_value_only(target_sym_id, None))
                        && !self.symbol_is_type_only(target_sym_id, None);
                    if target_is_value_only && !target_is_namespace_module {
                        TypeSymbolResolution::ValueOnly(target_sym_id)
                    } else {
                        TypeSymbolResolution::Type(target_sym_id)
                    }
                })
        };

        if let Some(local_sym_id) =
            self.ctx
                .binder
                .resolve_identifier_with_filter(self.ctx.arena, idx, &[], |sym_id| {
                    if self.ctx.symbol_is_from_lib(sym_id) {
                        return false;
                    }
                    if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
                        let is_class_member = Self::is_class_member_symbol(symbol.flags);
                        if is_class_member {
                            return false;
                        }
                    }
                    accept_type_symbol(sym_id)
                })
        {
            if let Some(symbol) = self.ctx.binder.get_symbol(local_sym_id)
                && symbol.flags & symbol_flags::ALIAS != 0
            {
                self.ctx
                    .referenced_symbols
                    .borrow_mut()
                    .insert(local_sym_id);
                if let Some(resolved) = resolve_alias_type_position_result(local_sym_id) {
                    return resolved;
                }
            }
            if self.ctx.binder.file_locals.get(name) == Some(local_sym_id)
                && let Some(ns_sym_id) =
                    self.resolve_unqualified_name_in_enclosing_namespace(idx, name)
                && ns_sym_id != local_sym_id
            {
                return TypeSymbolResolution::Type(ns_sym_id);
            }
            return TypeSymbolResolution::Type(local_sym_id);
        }

        if let Some(sym_id) = self.resolve_unqualified_name_in_enclosing_namespace(idx, name) {
            return TypeSymbolResolution::Type(sym_id);
        }

        let resolved = self.ctx.binder.resolve_identifier_with_filter(
            self.ctx.arena,
            idx,
            &lib_binders,
            |sym_id| {
                if should_skip_lib_symbol(sym_id) {
                    return false;
                }
                if let Some(symbol) = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders) {
                    let is_class_member = Self::is_class_member_symbol(symbol.flags);
                    if is_class_member {
                        return false;
                    }
                }
                accept_type_symbol(sym_id)
            },
        );

        if resolved.is_none()
            && let Some(sym_id) =
                self.resolve_identifier_symbol_from_all_binders(name, |sym_id, symbol| {
                    if should_skip_lib_symbol(sym_id) {
                        return false;
                    }

                    let is_class_member = Self::is_class_member_symbol(symbol.flags);
                    if is_class_member {
                        return false;
                    }
                    accept_type_symbol(sym_id)
                })
        {
            let is_value_only = (self.alias_resolves_to_value_only(sym_id, None)
                || self.symbol_is_value_only(sym_id, None))
                && !self.symbol_is_type_only(sym_id, None);
            if is_value_only {
                return TypeSymbolResolution::ValueOnly(sym_id);
            }
            return TypeSymbolResolution::Type(sym_id);
        }

        // Guard against SymbolId renumbering from lib merging: if the resolved
        // symbol's name doesn't match the requested name, the scope table has a
        // stale SymbolId. Reject it and fall through to value_only_candidate.
        let resolved = resolved.filter(|&sym_id| {
            self.ctx
                .binder
                .get_symbol_with_libs(sym_id, &lib_binders)
                .is_some_and(|s| s.escaped_name.as_str() == name)
        });
        if let Some(sym_id) = resolved {
            if let Some(symbol) = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders)
                && symbol.flags & symbol_flags::ALIAS != 0
            {
                // Mark the local alias as referenced (for unused-import tracking).
                // When we follow the alias chain below, only the target gets returned
                // and inserted into referenced_symbols by the caller. Without this,
                // imports used only in type positions appear unused (false TS6133).
                self.ctx.referenced_symbols.borrow_mut().insert(sym_id);
                if let Some(resolved) = resolve_alias_type_position_result(sym_id) {
                    return resolved;
                }
            }
            return TypeSymbolResolution::Type(sym_id);
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
    /// Returns a tuple of (`symbols_found`, `saw_class_scope`) where:
    /// - `symbols_found`: Vec of `SymbolIds` for all matching private members
    /// - `saw_class_scope`: true if any class scope was encountered
    pub(crate) fn resolve_private_identifier_symbols(
        &self,
        idx: NodeIndex,
    ) -> (Vec<SymbolId>, bool) {
        self.ctx
            .binder
            .resolve_private_identifier_symbols(self.ctx.arena, idx)
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
            let lib_binders = self.get_lib_binders();
            return match self.resolve_identifier_symbol_in_type_position(idx) {
                TypeSymbolResolution::Type(sym_id) => {
                    // Preserve unresolved alias symbols in type position.
                    // `import X = require("...")` aliases may not resolve to a concrete
                    // target symbol, but `X` is still a valid namespace-like type query
                    // anchor (e.g., `typeof X.Member`).
                    let resolved = self
                        .resolve_alias_symbol(sym_id, visited_aliases)
                        .unwrap_or(sym_id);
                    TypeSymbolResolution::Type(resolved)
                }
                TypeSymbolResolution::ValueOnly(sym_id)
                    if self.is_import_equals_type_anchor(sym_id, &lib_binders) =>
                {
                    let resolved = self
                        .resolve_alias_symbol(sym_id, visited_aliases)
                        .unwrap_or(sym_id);
                    TypeSymbolResolution::Type(resolved)
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

        if node.kind == tsz_parser::parser::syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            let Some(access) = self.ctx.arena.get_access_expr(node) else {
                return TypeSymbolResolution::NotFound;
            };

            let left_sym = match self.resolve_qualified_symbol_inner_in_type_position(
                access.expression,
                visited_aliases,
                depth + 1,
            ) {
                TypeSymbolResolution::Type(sym_id) => sym_id,
                other => return other,
            };

            let left_sym = self
                .resolve_alias_symbol(left_sym, visited_aliases)
                .unwrap_or(left_sym);

            let right_name = match self
                .ctx
                .arena
                .get_identifier_at(access.name_or_argument)
                .map(|ident| ident.escaped_text.as_str())
            {
                Some(name) => name,
                None => return TypeSymbolResolution::NotFound,
            };

            let lib_binders = self.get_lib_binders();
            let Some(left_symbol) = self.ctx.binder.get_symbol_with_libs(left_sym, &lib_binders)
            else {
                return TypeSymbolResolution::NotFound;
            };

            if let Some(exports) = left_symbol.exports.as_ref()
                && let Some(member_sym) = exports.get(right_name)
            {
                let is_value_only = (self
                    .alias_resolves_to_value_only(member_sym, Some(right_name))
                    || self.symbol_is_value_only(member_sym, Some(right_name)))
                    && !self.symbol_is_type_only(member_sym, Some(right_name));
                if is_value_only {
                    return TypeSymbolResolution::ValueOnly(member_sym);
                }
                let member_sym = self
                    .resolve_alias_symbol(member_sym, visited_aliases)
                    .unwrap_or(member_sym);
                return TypeSymbolResolution::Type(member_sym);
            }

            if let Some(ref module_specifier) = left_symbol.import_module
                && !((left_symbol.flags & symbol_flags::ALIAS) != 0
                    && self
                        .ctx
                        .module_resolves_to_non_module_entity(module_specifier))
                && let Some(reexported_sym) = self.resolve_reexported_member_symbol(
                    module_specifier,
                    right_name,
                    visited_aliases,
                )
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

            if let Some(reexported_sym) =
                self.resolve_member_from_import_equals_alias(left_sym, right_name, visited_aliases)
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

            return TypeSymbolResolution::NotFound;
        }

        if node.kind != tsz_parser::parser::syntax_kind_ext::QUALIFIED_NAME {
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
        let left_sym = self
            .resolve_alias_symbol(left_sym, visited_aliases)
            .unwrap_or(left_sym);
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

        // Look up the symbol across binders (file + libs)
        let lib_binders = self.get_lib_binders();
        let Some(left_symbol) = self.ctx.binder.get_symbol_with_libs(left_sym, &lib_binders) else {
            return TypeSymbolResolution::NotFound;
        };
        // First try direct exports
        if let Some(exports) = left_symbol.exports.as_ref()
            && let Some(member_sym) = exports.get(right_name)
        {
            let is_value_only = (self.alias_resolves_to_value_only(member_sym, Some(right_name))
                || self.symbol_is_value_only(member_sym, Some(right_name)))
                && !self.symbol_is_type_only(member_sym, Some(right_name));
            if is_value_only {
                return TypeSymbolResolution::ValueOnly(member_sym);
            }
            return TypeSymbolResolution::Type(
                self.resolve_alias_symbol(member_sym, visited_aliases)
                    .unwrap_or(member_sym),
            );
        }

        // If not found in direct exports, check for re-exports
        if let Some(ref module_specifier) = left_symbol.import_module {
            if (left_symbol.flags & symbol_flags::ALIAS) != 0
                && self
                    .ctx
                    .module_resolves_to_non_module_entity(module_specifier)
            {
                return TypeSymbolResolution::NotFound;
            }
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

        if let Some(reexported_sym) =
            self.resolve_member_from_import_equals_alias(left_sym, right_name, visited_aliases)
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

        TypeSymbolResolution::NotFound
    }

    fn resolve_identifier_symbol_from_all_binders(
        &self,
        name: &str,
        mut accept: impl FnMut(SymbolId, &tsz_binder::Symbol) -> bool,
    ) -> Option<SymbolId> {
        let all_binders = self.ctx.all_binders.as_ref()?;

        for (file_idx, binder) in all_binders.iter().enumerate() {
            if let Some(sym_id) = binder.file_locals.get(name) {
                let Some(sym_symbol) = binder.get_symbol(sym_id) else {
                    continue;
                };
                if !accept(sym_id, sym_symbol) {
                    continue;
                }
                if let Some(local_symbol) = self.ctx.binder.get_symbol(sym_id) {
                    if local_symbol.escaped_name != name {
                        self.ctx
                            .cross_file_symbol_targets
                            .borrow_mut()
                            .entry(sym_id)
                            .or_insert(file_idx);
                    }
                } else {
                    self.ctx
                        .cross_file_symbol_targets
                        .borrow_mut()
                        .entry(sym_id)
                        .or_insert(file_idx);
                }
                return Some(sym_id);
            }
        }

        None
    }

    /// Resolve a namespace member across all binders in multi-file mode.
    ///
    /// Cross-file lookup binders have `file_locals` (name→SymbolId) but empty symbol
    /// arenas. So we use the checker's own binder (which has the shared global symbol
    /// arena) to look up symbol data.
    ///
    /// Also handles nested namespaces: for `A.Utils.Plane`, searches parent namespace
    /// exports in each binder's `file_locals` to find the nested `Utils` namespace.
    pub(crate) fn resolve_namespace_member_from_all_binders(
        &self,
        namespace_name: &str,
        member_name: &str,
    ) -> Option<SymbolId> {
        let all_binders = self.ctx.all_binders.as_ref()?;

        for (file_idx, binder) in all_binders.iter().enumerate() {
            // Try to find namespace in file_locals first (top-level namespaces)
            if let Some(ns_sym_id) = binder.file_locals.get(namespace_name) {
                // Use checker's binder for symbol data (cross-file binders have empty arenas)
                if let Some(ns_symbol) = self.ctx.binder.get_symbol(ns_sym_id)
                    && ns_symbol.flags
                        & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE)
                        != 0
                    && let Some(exports) = ns_symbol.exports.as_ref()
                    && let Some(member_id) = exports.get(member_name)
                {
                    // Filter out enum members - they should only be accessible via qualified form
                    let is_enum_member = self
                        .ctx
                        .binder
                        .get_symbol(member_id)
                        .is_some_and(|s| s.flags & symbol_flags::ENUM_MEMBER != 0);
                    if !is_enum_member {
                        self.record_cross_file_member(member_id, member_name, file_idx);
                        return Some(member_id);
                    }
                }
            }

            // For nested namespaces (e.g., `Utils` inside `A`): search parent
            // namespace exports in this binder for the target namespace name.
            for (_, &parent_sym_id) in binder.file_locals.iter() {
                let Some(parent_sym) = self.ctx.binder.get_symbol(parent_sym_id) else {
                    continue;
                };
                if parent_sym.flags & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE)
                    == 0
                {
                    continue;
                }
                if let Some(parent_exports) = parent_sym.exports.as_ref()
                    && let Some(nested_ns_id) = parent_exports.get(namespace_name)
                    && let Some(nested_ns) = self.ctx.binder.get_symbol(nested_ns_id)
                    && nested_ns.flags
                        & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE)
                        != 0
                    && let Some(nested_exports) = nested_ns.exports.as_ref()
                    && let Some(member_id) = nested_exports.get(member_name)
                {
                    self.record_cross_file_member(member_id, member_name, file_idx);
                    return Some(member_id);
                }
            }
        }

        None
    }

    /// Record a cross-file symbol origin for proper arena delegation.
    fn record_cross_file_member(&self, member_id: SymbolId, member_name: &str, file_idx: usize) {
        if let Some(local_sym) = self.ctx.binder.get_symbol(member_id) {
            if local_sym.escaped_name.as_str() != member_name {
                self.ctx
                    .cross_file_symbol_targets
                    .borrow_mut()
                    .entry(member_id)
                    .or_insert(file_idx);
            }
        } else {
            self.ctx
                .cross_file_symbol_targets
                .borrow_mut()
                .entry(member_id)
                .or_insert(file_idx);
        }
    }

    /// Resolve an unqualified name by checking exports of enclosing namespace(s).
    ///
    /// When code inside `namespace A { ... }` in file2 references `Point`,
    /// and `Point` is exported from `namespace A` in file1, the normal scope
    /// chain only sees file2's namespace body. This method walks up the AST
    /// to find enclosing `MODULE_DECLARATION` nodes and checks their merged
    /// symbol exports for the name.
    fn resolve_unqualified_name_in_enclosing_namespace(
        &self,
        node_idx: NodeIndex,
        name: &str,
    ) -> Option<SymbolId> {
        // Only applies in global scripts — in external modules, namespaces
        // in different files do NOT merge (each file is its own module).
        if self.ctx.binder.is_external_module() {
            return None;
        }

        let arena = self.ctx.arena;
        let mut current = node_idx;

        // Walk up the AST looking for enclosing MODULE_DECLARATION nodes
        for _ in 0..100 {
            let ext = arena.get_extended(current)?;
            let parent_idx = ext.parent;
            if parent_idx.is_none() {
                break;
            }
            let parent_node = arena.get(parent_idx)?;
            if parent_node.kind == syntax_kind_ext::MODULE_DECLARATION {
                // Found an enclosing namespace. Get its name.
                if let Some(module_data) = arena.get_module(parent_node)
                    && let Some(ns_name_ident) = arena.get_identifier_at(module_data.name)
                {
                    // Same-block namespace members are visible inside the block
                    // even when they are not exported. Consult the namespace
                    // body's persistent scope before falling back to exports.
                    if module_data.body.is_some()
                        && let Some(&scope_id) =
                            self.ctx.binder.node_scope_ids.get(&module_data.body.0)
                        && let Some(scope) = self.ctx.binder.scopes.get(scope_id.0 as usize)
                        && let Some(member_id) = scope.table.get(name)
                    {
                        let is_enum_member = self
                            .ctx
                            .binder
                            .get_symbol(member_id)
                            .is_some_and(|s| s.flags & symbol_flags::ENUM_MEMBER != 0);
                        if !is_enum_member {
                            return Some(member_id);
                        }
                    }

                    let ns_name = ns_name_ident.escaped_text.as_str();
                    // Look up the name in the merged namespace's exports
                    // First check the global symbol directly
                    if let Some(ns_sym_id) = self.ctx.binder.file_locals.get(ns_name)
                        && let Some(ns_sym) = self.ctx.binder.get_symbol(ns_sym_id)
                        && let Some(exports) = ns_sym.exports.as_ref()
                        && let Some(member_id) = exports.get(name)
                    {
                        // Filter out enum members - they should only be accessible via qualified form
                        let is_enum_member = self
                            .ctx
                            .binder
                            .get_symbol(member_id)
                            .is_some_and(|s| s.flags & symbol_flags::ENUM_MEMBER != 0);
                        if !is_enum_member {
                            return Some(member_id);
                        }
                    }
                    // Also try cross-file resolution via all binders
                    if let Some(member_id) =
                        self.resolve_namespace_member_from_all_binders(ns_name, name)
                    {
                        return Some(member_id);
                    }
                }
            }
            current = parent_idx;
        }
        None
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
            // Preserve alias symbols when alias resolution has no concrete target
            // (e.g., `import X = require("...")` namespace-like aliases).
            return self
                .resolve_alias_symbol(sym_id, visited_aliases)
                .or(Some(sym_id));
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

        if node.kind == tsz_parser::parser::syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            let access = self.ctx.arena.get_access_expr(node)?;
            let left_sym =
                self.resolve_qualified_symbol_inner(access.expression, visited_aliases, depth + 1)?;
            let left_sym = self
                .resolve_alias_symbol(left_sym, visited_aliases)
                .unwrap_or(left_sym);
            let right_name = self
                .ctx
                .arena
                .get_identifier_at(access.name_or_argument)
                .map(|ident| ident.escaped_text.as_str())?;

            let lib_binders = self.get_lib_binders();
            let left_symbol = self
                .ctx
                .binder
                .get_symbol_with_libs(left_sym, &lib_binders)?;

            if let Some(exports) = left_symbol.exports.as_ref()
                && let Some(member_sym) = exports.get(right_name)
            {
                return Some(
                    self.resolve_alias_symbol(member_sym, visited_aliases)
                        .unwrap_or(member_sym),
                );
            }

            if let Some(ref module_specifier) = left_symbol.import_module {
                if (left_symbol.flags & symbol_flags::ALIAS) != 0
                    && self
                        .ctx
                        .module_resolves_to_non_module_entity(module_specifier)
                {
                    return None;
                }
                return self.resolve_reexported_member_symbol(
                    module_specifier,
                    right_name,
                    visited_aliases,
                );
            }

            if let Some(reexported_sym) =
                self.resolve_member_from_import_equals_alias(left_sym, right_name, visited_aliases)
            {
                return Some(reexported_sym);
            }

            // Cross-file namespace merging fallback: if the member wasn't found in
            // the resolved symbol's exports, check other files' namespace declarations
            // with the same name. This handles `namespace A` declared across files.
            if left_symbol.flags & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE)
                != 0
                && let Some(member_sym) = self.resolve_namespace_member_from_all_binders(
                    left_symbol.escaped_name.as_str(),
                    right_name,
                )
            {
                return Some(
                    self.resolve_alias_symbol(member_sym, visited_aliases)
                        .unwrap_or(member_sym),
                );
            }

            return None;
        }

        if node.kind != tsz_parser::parser::syntax_kind_ext::QUALIFIED_NAME {
            return None;
        }

        let qn = self.ctx.arena.get_qualified_name(node)?;
        let left_sym = self.resolve_qualified_symbol_inner(qn.left, visited_aliases, depth + 1)?;
        let left_sym = self
            .resolve_alias_symbol(left_sym, visited_aliases)
            .unwrap_or(left_sym);
        let right_name = self
            .ctx
            .arena
            .get(qn.right)
            .and_then(|node| self.ctx.arena.get_identifier(node))
            .map(|ident| ident.escaped_text.as_str())?;

        let lib_binders = self.get_lib_binders();
        let left_symbol = self
            .ctx
            .binder
            .get_symbol_with_libs(left_sym, &lib_binders)?;

        // First try direct exports
        if let Some(exports) = left_symbol.exports.as_ref()
            && let Some(member_sym) = exports.get(right_name)
        {
            return Some(
                self.resolve_alias_symbol(member_sym, visited_aliases)
                    .unwrap_or(member_sym),
            );
        }

        // If not found in direct exports, check for re-exports
        // This handles cases like: export { foo } from './bar'
        if let Some(ref module_specifier) = left_symbol.import_module {
            if (left_symbol.flags & symbol_flags::ALIAS) != 0
                && self
                    .ctx
                    .module_resolves_to_non_module_entity(module_specifier)
            {
                return None;
            }
            if let Some(reexported_sym) =
                self.resolve_reexported_member_symbol(module_specifier, right_name, visited_aliases)
            {
                return Some(reexported_sym);
            }
        }

        if let Some(reexported_sym) =
            self.resolve_member_from_import_equals_alias(left_sym, right_name, visited_aliases)
        {
            return Some(reexported_sym);
        }

        // Cross-file namespace merging fallback for qualified names in type position.
        if left_symbol.flags & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE) != 0
            && let Some(member_sym) = self.resolve_namespace_member_from_all_binders(
                left_symbol.escaped_name.as_str(),
                right_name,
            )
        {
            return Some(
                self.resolve_alias_symbol(member_sym, visited_aliases)
                    .unwrap_or(member_sym),
            );
        }

        None
    }

    fn resolve_member_from_import_equals_alias(
        &self,
        alias_sym: SymbolId,
        member_name: &str,
        visited_aliases: &mut Vec<SymbolId>,
    ) -> Option<SymbolId> {
        let symbol = self.ctx.binder.get_symbol(alias_sym)?;
        if symbol.flags & symbol_flags::ALIAS == 0 {
            return None;
        }

        let decl_idx = if symbol.value_declaration.is_some() {
            symbol.value_declaration
        } else {
            symbol
                .declarations
                .iter()
                .copied()
                .find(|idx| idx.is_some())
                .unwrap_or(NodeIndex::NONE)
        };

        if decl_idx.is_some()
            && let Some(decl_node) = self.ctx.arena.get(decl_idx)
            && decl_node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
            && let Some(import) = self.ctx.arena.get_import_decl(decl_node)
            && let Some(module_specifier) =
                self.get_require_module_specifier(import.module_specifier)
        {
            if self
                .ctx
                .module_resolves_to_non_module_entity(&module_specifier)
            {
                return None;
            }
            return self.resolve_reexported_member_symbol(
                &module_specifier,
                member_name,
                visited_aliases,
            );
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

    fn resolve_member_from_module_exports(
        &self,
        binder: &tsz_binder::BinderState,
        exports_table: &tsz_binder::SymbolTable,
        member_name: &str,
        visited_aliases: &mut Vec<SymbolId>,
    ) -> Option<SymbolId> {
        let can_resolve_aliases = std::ptr::eq(binder, self.ctx.binder);

        if let Some(sym_id) = exports_table.get(member_name) {
            if can_resolve_aliases {
                return Some(
                    self.resolve_alias_symbol(sym_id, visited_aliases)
                        .unwrap_or(sym_id),
                );
            }
            return Some(sym_id);
        }

        let export_equals_sym = exports_table.get("export=")?;
        let mut candidate_symbol_ids = vec![export_equals_sym];
        if can_resolve_aliases {
            let resolved_export_equals = self
                .resolve_alias_symbol(export_equals_sym, visited_aliases)
                .unwrap_or(export_equals_sym);
            if resolved_export_equals != export_equals_sym {
                candidate_symbol_ids.push(resolved_export_equals);
            }
        }

        for candidate_symbol_id in candidate_symbol_ids {
            let Some(target_symbol) = binder.get_symbol(candidate_symbol_id) else {
                continue;
            };

            if let Some(exports) = target_symbol.exports.as_ref()
                && let Some(sym_id) = exports.get(member_name)
            {
                if can_resolve_aliases {
                    return Some(
                        self.resolve_alias_symbol(sym_id, visited_aliases)
                            .unwrap_or(sym_id),
                    );
                }
                return Some(sym_id);
            }

            if let Some(members) = target_symbol.members.as_ref()
                && let Some(sym_id) = members.get(member_name)
            {
                if can_resolve_aliases {
                    return Some(
                        self.resolve_alias_symbol(sym_id, visited_aliases)
                            .unwrap_or(sym_id),
                    );
                }
                return Some(sym_id);
            }

            // Some binder states keep the namespace merge partner as a distinct symbol.
            // Search same-name symbols with module namespace flags for members.
            for merged_candidate_id in binder
                .get_symbols()
                .find_all_by_name(&target_symbol.escaped_name)
            {
                let Some(merged_symbol) = binder.get_symbol(merged_candidate_id) else {
                    continue;
                };
                if (merged_symbol.flags
                    & (symbol_flags::MODULE
                        | symbol_flags::NAMESPACE_MODULE
                        | symbol_flags::VALUE_MODULE))
                    == 0
                {
                    continue;
                }

                if let Some(exports) = merged_symbol.exports.as_ref()
                    && let Some(sym_id) = exports.get(member_name)
                {
                    if can_resolve_aliases {
                        return Some(
                            self.resolve_alias_symbol(sym_id, visited_aliases)
                                .unwrap_or(sym_id),
                        );
                    }
                    return Some(sym_id);
                }

                if let Some(members) = merged_symbol.members.as_ref()
                    && let Some(sym_id) = members.get(member_name)
                {
                    if can_resolve_aliases {
                        return Some(
                            self.resolve_alias_symbol(sym_id, visited_aliases)
                                .unwrap_or(sym_id),
                        );
                    }
                    return Some(sym_id);
                }
            }
        }

        None
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

        // First, check if it's a direct export from this module (ambient modules)
        if let Some(module_exports) = self.ctx.binder.module_exports.get(module_specifier)
            && let Some(sym_id) = self.resolve_member_from_module_exports(
                self.ctx.binder,
                module_exports,
                member_name,
                visited_aliases,
            )
        {
            return Some(sym_id);
        }

        // Cross-file resolution: use canonical file-key lookups via state_type_resolution.
        if let Some(sym_id) = self.resolve_cross_file_export(module_specifier, member_name) {
            return Some(
                self.resolve_alias_symbol(sym_id, visited_aliases)
                    .unwrap_or(sym_id),
            );
        }

        // Check for named re-exports: `export { foo } from 'bar'`
        if let Some(file_reexports) = self.ctx.binder.reexports.get(module_specifier)
            && let Some((source_module, original_name)) = file_reexports.get(member_name)
        {
            let name_to_lookup = original_name.as_deref().unwrap_or(member_name);
            return self.resolve_reexported_member_symbol_inner(
                source_module,
                name_to_lookup,
                visited_aliases,
                visited_modules,
            );
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

    /// Get all type parameter bindings for passing to `TypeLowering`.
    ///
    /// Returns a vector of (name, `TypeId`) pairs for all type parameters in scope.
    pub(crate) fn get_type_param_bindings(&self) -> Vec<(tsz_common::interner::Atom, TypeId)> {
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
    /// Get the text representation of an expression (simple chains only).
    /// Handles Identifiers and `PropertyAccessExpressions` (e.g., `a.b.c`).
    pub(crate) fn expression_text(&self, idx: NodeIndex) -> Option<String> {
        let node = self.ctx.arena.get(idx)?;
        match node.kind {
            k if k == SyntaxKind::Identifier as u16 => self
                .ctx
                .arena
                .get_identifier(node)
                .map(|ident| ident.escaped_text.clone()),
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                let access = self.ctx.arena.get_access_expr(node)?;
                let left = self.expression_text(access.expression)?;
                let right = self.expression_text(access.name_or_argument)?;
                Some(format!("{left}.{right}"))
            }
            _ => None,
        }
    }

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

    /// Resolve a simple or qualified type name through the merged checker binder.
    ///
    /// Cross-arena lowering cannot trust raw `NodeIndex` values because the same
    /// index may refer to unrelated nodes in different declaration arenas. This
    /// helper uses the text form (`A` or `A.B.C`) and walks the merged binder's
    /// export graph to recover the correct `DefId`.
    pub(crate) fn resolve_entity_name_text_to_def_id_for_lowering(
        &self,
        name: &str,
    ) -> Option<tsz_solver::def::DefId> {
        if is_compiler_managed_type(name) {
            return None;
        }

        let mut segments = name.split('.');
        let root_name = segments.next()?;
        let mut current_sym = self.ctx.binder.file_locals.get(root_name)?;
        let lib_binders = self.get_lib_binders();

        for segment in segments {
            let mut visited_aliases = Vec::new();
            current_sym = self
                .resolve_alias_symbol(current_sym, &mut visited_aliases)
                .unwrap_or(current_sym);

            let symbol = self.get_cross_file_symbol(current_sym).or_else(|| {
                self.ctx
                    .binder
                    .get_symbol_with_libs(current_sym, &lib_binders)
            })?;

            if let Some(member_sym) = symbol
                .exports
                .as_ref()
                .and_then(|exports| exports.get(segment))
                .or_else(|| {
                    symbol
                        .members
                        .as_ref()
                        .and_then(|members| members.get(segment))
                })
            {
                current_sym = member_sym;
                continue;
            }

            if let Some(ref module_specifier) = symbol.import_module {
                let mut visited_aliases = Vec::new();
                if let Some(member_sym) = self.resolve_reexported_member_symbol(
                    module_specifier,
                    segment,
                    &mut visited_aliases,
                ) {
                    current_sym = member_sym;
                    continue;
                }
            }

            if symbol.flags & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE) != 0
                && let Some(member_sym) = self.resolve_namespace_member_from_all_binders(
                    symbol.escaped_name.as_str(),
                    segment,
                )
            {
                current_sym = member_sym;
                continue;
            }

            return None;
        }

        let mut visited_aliases = Vec::new();
        let resolved_sym = self
            .resolve_alias_symbol(current_sym, &mut visited_aliases)
            .unwrap_or(current_sym);
        Some(self.ctx.get_or_create_def_id(resolved_sym))
    }

    // =========================================================================
    // Symbol Resolution for Lowering
    // =========================================================================

    /// Resolve a type symbol for type lowering.
    ///
    /// Returns the symbol ID if the resolved symbol has the TYPE flag set.
    /// Returns None for built-in types that have special handling in `TypeLowering`.
    pub(crate) fn resolve_type_symbol_for_lowering(&self, idx: NodeIndex) -> Option<u32> {
        // Skip built-in types that have special handling in TypeLowering
        // These types use built-in TypeData representations instead of Refs
        if let Some(node) = self.ctx.arena.get(idx)
            && let Some(ident) = self.ctx.arena.get_identifier(node)
        {
            if is_compiler_managed_type(ident.escaped_text.as_str()) {
                return None;
            }
            if node.kind == SyntaxKind::Identifier as u16
                && let TypeSymbolResolution::Type(sym_id) =
                    self.resolve_identifier_symbol_in_type_position(idx)
            {
                let lib_binders = self.get_lib_binders();
                if let Some(symbol) = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders)
                    && (symbol.flags & symbol_flags::TYPE) != 0
                {
                    return Some(sym_id.0);
                }
            }
        }

        let sym_id = match self.resolve_qualified_symbol_in_type_position(idx) {
            TypeSymbolResolution::Type(sym_id) => sym_id,
            _ => return None,
        };
        let lib_binders = self.get_lib_binders();
        let symbol = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders)?;
        ((symbol.flags & symbol_flags::TYPE) != 0).then_some(sym_id.0)
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
                while let Some(node) = self.ctx.arena.get(current) {
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
            return Some(sym_id.0);
        }

        // The initial resolution found a TYPE-only symbol (e.g., `interface Promise<T>`
        // from one lib file). But the VALUE declaration (`declare var Promise`) may
        // exist in a different lib file. Search all lib binders by name for a symbol
        // that has the VALUE flag. This handles declaration merging across lib files.
        let name = self
            .ctx
            .arena
            .get(idx)
            .and_then(|n| self.ctx.arena.get_identifier(n))
            .map(|i| i.escaped_text.as_str());
        if let Some(name) = name {
            // Check file_locals first (may have merged value from lib)
            if let Some(val_sym_id) = self.ctx.binder.file_locals.get(name)
                && let Some(val_symbol) = self
                    .ctx
                    .binder
                    .get_symbol_with_libs(val_sym_id, &lib_binders)
                && (val_symbol.flags & (symbol_flags::VALUE | symbol_flags::ALIAS)) != 0
                && !val_symbol.is_type_only
            {
                return Some(val_sym_id.0);
            }
            // Search lib binders directly for a value declaration
            for lib_binder in &lib_binders {
                if let Some(val_sym_id) = lib_binder.file_locals.get(name)
                    && let Some(val_symbol) = lib_binder.get_symbol(val_sym_id)
                    && (val_symbol.flags & (symbol_flags::VALUE | symbol_flags::ALIAS)) != 0
                    && !val_symbol.is_type_only
                {
                    return Some(val_sym_id.0);
                }
            }
        }

        None
    }
}
