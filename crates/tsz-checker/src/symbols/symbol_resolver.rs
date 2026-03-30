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
    fn resolve_enclosing_type_parameter_symbol(
        &self,
        idx: NodeIndex,
        name: &str,
    ) -> Option<SymbolId> {
        use tsz_parser::parser::syntax_kind_ext;

        let mut current = self.ctx.arena.get_extended(idx).map(|ext| ext.parent);
        // Track whether we've passed through a ComputedPropertyName. If so,
        // the enclosing class member's type parameters must be skipped because
        // computed property names are evaluated in the class scope, not the
        // method scope. In `[foo<T>(a)]<T>(a: T) {}`, `T` inside `[...]`
        // must NOT resolve to the method's own type parameter.
        let mut inside_computed_property_name = false;
        while let Some(parent_idx) = current {
            let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
                break;
            };

            if parent_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
                inside_computed_property_name = true;
            }

            // Skip type parameters of class members when inside their computed property name
            let skip_type_params = inside_computed_property_name
                && matches!(
                    parent_node.kind,
                    k if k == syntax_kind_ext::METHOD_DECLARATION
                        || k == syntax_kind_ext::CONSTRUCTOR
                        || k == syntax_kind_ext::GET_ACCESSOR
                        || k == syntax_kind_ext::SET_ACCESSOR
                );

            let type_params = if skip_type_params {
                // Clear the flag once we've skipped the class member
                inside_computed_property_name = false;
                None
            } else {
                self.ctx
                    .arena
                    .get_function(parent_node)
                    .and_then(|data| data.type_parameters.as_ref())
                    .or_else(|| {
                        self.ctx
                            .arena
                            .get_class(parent_node)
                            .and_then(|data| data.type_parameters.as_ref())
                    })
                    .or_else(|| {
                        self.ctx
                            .arena
                            .get_interface(parent_node)
                            .and_then(|data| data.type_parameters.as_ref())
                    })
                    .or_else(|| {
                        self.ctx
                            .arena
                            .get_type_alias(parent_node)
                            .and_then(|data| data.type_parameters.as_ref())
                    })
                    .or_else(|| {
                        self.ctx
                            .arena
                            .get_signature(parent_node)
                            .and_then(|data| data.type_parameters.as_ref())
                    })
                    .or_else(|| {
                        self.ctx
                            .arena
                            .get_method_decl(parent_node)
                            .and_then(|data| data.type_parameters.as_ref())
                    })
                    .or_else(|| {
                        self.ctx
                            .arena
                            .get_accessor(parent_node)
                            .and_then(|data| data.type_parameters.as_ref())
                    })
                    .or_else(|| {
                        self.ctx
                            .arena
                            .get_constructor(parent_node)
                            .and_then(|data| data.type_parameters.as_ref())
                    })
                    .or_else(|| {
                        self.ctx
                            .arena
                            .get_function_type(parent_node)
                            .and_then(|data| data.type_parameters.as_ref())
                    })
            };

            if let Some(type_params) = type_params {
                for &param_idx in &type_params.nodes {
                    let Some(param_node) = self.ctx.arena.get(param_idx) else {
                        continue;
                    };
                    let Some(param_data) = self.ctx.arena.get_type_parameter(param_node) else {
                        continue;
                    };
                    let Some(name_node) = self.ctx.arena.get(param_data.name) else {
                        continue;
                    };
                    let Some(ident) = self.ctx.arena.get_identifier(name_node) else {
                        continue;
                    };
                    if ident.escaped_text == name
                        && let Some(sym_id) = self.ctx.binder.get_node_symbol(param_idx)
                    {
                        return Some(sym_id);
                    }
                }
            }

            current = self
                .ctx
                .arena
                .get_extended(parent_idx)
                .map(|ext| ext.parent);
        }

        None
    }

    // =========================================================================
    // Symbol Type Resolution
    // =========================================================================

    // =========================================================================
    // Identifier Resolution
    // =========================================================================

    /// Collect lib binders from `lib_contexts` for cross-arena symbol lookup.
    /// This enables symbol resolution across lib.d.ts files when `lib_binders`
    /// is not populated in the binder (e.g., in the driver.rs path).
    ///
    /// Returns an `Arc`-wrapped vec for O(1) cloning. The `Arc<Vec<_>>` auto-derefs
    /// to `&[Arc<BinderState>]` so callers using `&lib_binders` work unchanged.
    pub(crate) fn get_lib_binders(&self) -> Arc<Vec<Arc<tsz_binder::BinderState>>> {
        // O(1) Arc::clone — the entire vec is shared, not individual elements.
        Arc::clone(&self.ctx.lib_binders_cached)
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
    pub(crate) fn is_import_equals_type_anchor(
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
            && self.ctx.arena.get(decl_idx).is_some_and(|node| {
                // `import X = require(...)` or `import X = A.B.C`
                node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                    // `import * as X from "..."` — namespace import creates a
                    // namespace-like binding usable as a qualified type anchor
                    // (e.g., `X.SomeType`)
                    || node.kind == syntax_kind_ext::NAMESPACE_IMPORT
            })
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

    /// Resolve an identifier without mutating unused-reference tracking.
    pub(crate) fn resolve_identifier_symbol_without_tracking(
        &self,
        idx: NodeIndex,
    ) -> Option<SymbolId> {
        self.resolve_identifier_symbol_inner(idx)
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
        if let Some(sym_id) = self.resolve_for_of_header_expression_symbol(idx) {
            return Some(sym_id);
        }

        let ignore_libs = !self.ctx.has_lib_loaded();
        let empty_binders: Arc<Vec<Arc<tsz_binder::BinderState>>> = Arc::new(Vec::new());
        let lib_binders = if ignore_libs {
            empty_binders
        } else {
            self.get_lib_binders()
        };
        let in_decorator_expr = self.is_in_decorator_expression(idx);
        let decorator_owner = in_decorator_expr
            .then(|| self.decorator_owner_declaration(idx))
            .flatten();
        let is_from_lib = |sym_id: SymbolId| self.ctx.symbol_is_from_lib(sym_id);
        let should_skip_lib_symbol = |sym_id: SymbolId| ignore_libs && is_from_lib(sym_id);

        // PERF: ident_name is only used by trace! calls which are compiled out
        // in release builds (release_max_level_warn). The to_string() allocation
        // is eliminated by the compiler since ident_name becomes dead code.
        let ident_name = self
            .ctx
            .arena
            .get_identifier_at(idx)
            .map(|i| i.escaped_text.as_str().to_string());

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
                    if let Some(owner_idx) = decorator_owner
                        && symbol.declarations.iter().any(|&decl_idx| {
                            // Allow the decorator owner itself — only filter out
                            // declarations strictly inside it (e.g., class members).
                            // The class name should be resolvable from its own
                            // decorator; TDZ checks handle validity.
                            decl_idx != owner_idx
                                && self.node_is_within_decorator_owner(decl_idx, owner_idx)
                        })
                    {
                        return false;
                    }
                    let is_class_member = Self::is_class_member_symbol(symbol.flags);
                    if is_class_member {
                        if in_decorator_expr {
                            return false;
                        }
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
        let identifier_is_type_position = self.is_identifier_in_type_position(idx);
        let result = result.filter(|&sym_id| {
            if !identifier_is_type_position {
                return true;
            }
            let Some(symbol) = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders) else {
                return true;
            };
            if !self.ctx.binder.is_external_module()
                || self.is_in_declare_namespace_or_module(idx)
                || self.ctx.symbol_is_from_lib(sym_id)
                || symbol.is_umd_export
                || symbol.decl_file_idx == u32::MAX
                || symbol.decl_file_idx == self.ctx.current_file_idx as u32
                || (symbol.flags & symbol_flags::VALUE) != 0
            {
                return true;
            }
            let Some(owner_binder) = self.ctx.get_binder_for_file(symbol.decl_file_idx as usize)
            else {
                return true;
            };
            let owner_is_declaration_file = self
                .ctx
                .get_arena_for_file(symbol.decl_file_idx)
                .source_files
                .first()
                .is_some_and(|sf| sf.is_declaration_file);
            owner_is_declaration_file
                || !owner_binder.is_external_module()
                || owner_binder
                    .global_augmentations
                    .contains_key(symbol.escaped_name.as_str())
        });

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
                        // Filter out string-literal ambient module symbols (e.g., `declare module "foobar"`)
                        // — they should not resolve as bare identifiers.
                        if self.is_string_literal_module_symbol(file_sym_id, &lib_binders) {
                            continue;
                        }
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
                    let is_private_external_module_type = identifier_is_type_position
                        && self.ctx.binder.is_external_module()
                        && !self.ctx.symbol_is_from_lib(sym_id)
                        && !symbol.is_umd_export
                        && symbol.decl_file_idx != u32::MAX
                        && symbol.decl_file_idx != self.ctx.current_file_idx as u32
                        && (symbol.flags & symbol_flags::VALUE) == 0
                        && self
                            .ctx
                            .get_binder_for_file(symbol.decl_file_idx as usize)
                            .is_some_and(|binder| {
                                binder.is_external_module()
                                    && !binder.global_augmentations.contains_key(name)
                            })
                        && !self
                            .ctx
                            .get_arena_for_file(symbol.decl_file_idx)
                            .source_files
                            .first()
                            .is_some_and(|sf| sf.is_declaration_file);
                    if is_private_external_module_type {
                        return false;
                    }
                    // NOTE: We intentionally skip the decorator_owner check here.
                    // Cross-file symbols have NodeIndex values from different arenas,
                    // so `node_is_within_decorator_owner` would walk parent pointers
                    // in the wrong arena, causing false positives when indices
                    // coincidentally overlap with nodes inside the class declaration.
                    // Cross-file symbols can never be inside the current file's
                    // decorator owner, so this filter is unnecessary.

                    let is_class_member = Self::is_class_member_symbol(symbol.flags);
                    if is_class_member {
                        if in_decorator_expr {
                            return false;
                        }
                        return is_from_lib(sym_id)
                            && (symbol.flags & symbol_flags::EXPORT_VALUE) != 0;
                    }
                    true
                })
            {
                // Filter out string-literal ambient module symbols (e.g., `declare module "foobar"`)
                // — they should not resolve as bare identifiers.
                if !self.is_string_literal_module_symbol(sym_id, &lib_binders) {
                    return Some(sym_id);
                }
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

    /// Resolve a type-position identifier without mutating unused-reference tracking.
    pub(crate) fn resolve_identifier_symbol_in_type_position_without_tracking(
        &self,
        idx: NodeIndex,
    ) -> TypeSymbolResolution {
        self.resolve_identifier_symbol_in_type_position_inner(idx)
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

        if let Some(sym_id) = self.resolve_enclosing_type_parameter_symbol(idx, name) {
            return TypeSymbolResolution::Type(sym_id);
        }

        if let Some(sym_id) = self.resolve_unqualified_name_in_enclosing_namespace(idx, name) {
            return TypeSymbolResolution::Type(sym_id);
        }

        let ignore_libs = !self.ctx.has_lib_loaded();
        // Collect lib binders for cross-arena symbol lookup
        let empty_binders: Arc<Vec<Arc<tsz_binder::BinderState>>> = Arc::new(Vec::new());
        let lib_binders = if ignore_libs {
            empty_binders
        } else {
            self.get_lib_binders()
        };
        let should_skip_lib_symbol =
            |sym_id: SymbolId| ignore_libs && self.ctx.symbol_is_from_lib(sym_id);
        let value_only_candidate = std::cell::Cell::new(None::<SymbolId>);

        // Check if this name exists in a local scope (namespace/module) that would shadow
        // the global lib symbol. If so, we skip the early lib_contexts check and let the
        // binder's scope-based resolution find the local symbol first.
        // PERF: Use the cached resolve_identifier (which caches results per (arena, node_idx))
        // instead of resolve_identifier_with_filter which is uncached.
        let name_in_local_scope = if !ignore_libs {
            self.ctx
                .binder
                .resolve_identifier(self.ctx.arena, idx)
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
            for lib_ctx in self.ctx.lib_contexts.iter() {
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
                            if value_only_candidate.get().is_none() {
                                value_only_candidate.set(Some(sym_id));
                            }
                        } else {
                            // Valid type symbol found in lib
                            return TypeSymbolResolution::Type(sym_id);
                        }
                    }
                }
            }
        }

        let accept_type_symbol = |sym_id: SymbolId| -> bool {
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

            // When a symbol is merged from an import alias and a local value declaration
            // (e.g., `import { FC } from "./types"; let FC: FC | null = null;`),
            // the type meaning comes from the alias chain. If the alias resolves to a
            // type (not value-only), accept the symbol in type position.
            let alias_is_type = (flags & symbol_flags::ALIAS) != 0
                && !self.alias_resolves_to_value_only(sym_id, None);
            if alias_is_type && (flags & symbol_flags::VALUE) != 0 {
                return true;
            }

            let is_value_only = (self.alias_resolves_to_value_only(sym_id, None)
                || self.symbol_is_value_only(sym_id, None))
                && !self.symbol_is_type_only(sym_id, None);
            if is_value_only {
                if value_only_candidate.get().is_none() {
                    value_only_candidate.set(Some(sym_id));
                }
                return false;
            }
            true
        };

        let resolve_alias_type_position_result = |sym_id: SymbolId| {
            if let Some(alias_symbol) = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders)
                && let Some(module_name) = alias_symbol.import_module.as_ref()
                && alias_symbol.import_name.is_some()
            {
                let expected_name = alias_symbol
                    .import_name
                    .as_deref()
                    .unwrap_or(&alias_symbol.escaped_name);
                let source_file_idx = self
                    .ctx
                    .resolve_symbol_file_index(sym_id)
                    .unwrap_or(self.ctx.current_file_idx);
                if let Some(target_sym_id) = self.resolve_cross_file_export_from_file(
                    module_name,
                    expected_name,
                    Some(source_file_idx),
                ) {
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
                    return Some(if target_is_value_only && !target_is_namespace_module {
                        TypeSymbolResolution::ValueOnly(target_sym_id)
                    } else {
                        TypeSymbolResolution::Type(target_sym_id)
                    });
                }
            }
            let mut visited_aliases = Vec::new();
            self.resolve_alias_symbol(sym_id, &mut visited_aliases)
                .map(|target_sym_id| {
                    if let Some(alias_symbol) =
                        self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders)
                        && let Some(module_name) = alias_symbol.import_module.as_ref()
                    {
                        let expected_name = alias_symbol
                            .import_name
                            .as_deref()
                            .unwrap_or(&alias_symbol.escaped_name);
                        self.record_cross_file_symbol_if_needed(
                            target_sym_id,
                            expected_name,
                            module_name,
                        );
                    }
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

        let should_preserve_alias_symbol_in_type_position = |sym_id: SymbolId| {
            let Some(symbol) = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders) else {
                return false;
            };
            if (symbol.flags & symbol_flags::ALIAS) == 0 {
                return false;
            }

            let has_local_type_meaning = self.symbol_has_declared_type_meaning(sym_id);
            let is_namespace_import_alias = symbol.import_module.is_some()
                && matches!(symbol.import_name.as_deref(), Some("*"));

            has_local_type_meaning || is_namespace_import_alias
        };

        let is_private_external_module_type_symbol = |sym_id: SymbolId| -> bool {
            let Some(symbol) = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders) else {
                return false;
            };
            if !self.ctx.binder.is_external_module()
                || self.is_in_declare_namespace_or_module(idx)
                || self.ctx.symbol_is_from_lib(sym_id)
                || symbol.is_umd_export
                || symbol.decl_file_idx == u32::MAX
                || symbol.decl_file_idx == self.ctx.current_file_idx as u32
                || (symbol.flags & symbol_flags::VALUE) != 0
            {
                return false;
            }
            let Some(owner_binder) = self.ctx.get_binder_for_file(symbol.decl_file_idx as usize)
            else {
                return false;
            };
            let owner_is_declaration_file = self
                .ctx
                .get_arena_for_file(symbol.decl_file_idx)
                .source_files
                .first()
                .is_some_and(|sf| sf.is_declaration_file);
            if owner_is_declaration_file {
                return false;
            }
            owner_binder.is_external_module()
                && !owner_binder.global_augmentations.contains_key(name)
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
            && !is_private_external_module_type_symbol(local_sym_id)
        {
            if let Some(symbol) = self.ctx.binder.get_symbol(local_sym_id)
                && symbol.flags & symbol_flags::ALIAS != 0
            {
                if let Some((&type_alias_id, _)) = self
                    .ctx
                    .binder
                    .alias_partners
                    .iter()
                    .find(|&(_, &alias_id)| alias_id == local_sym_id)
                {
                    return TypeSymbolResolution::Type(type_alias_id);
                }
                if should_preserve_alias_symbol_in_type_position(local_sym_id) {
                    return TypeSymbolResolution::Type(local_sym_id);
                }
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

        let resolved = self
            .ctx
            .binder
            .resolve_identifier_with_filter(self.ctx.arena, idx, &lib_binders, |sym_id| {
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
            })
            .filter(|&sym_id| !is_private_external_module_type_symbol(sym_id));
        let has_value_only = value_only_candidate.get().is_some();
        if resolved.is_none()
            && !has_value_only
            && let Some(sym_id) =
                self.resolve_identifier_symbol_from_all_binders(name, |sym_id, symbol| {
                    if should_skip_lib_symbol(sym_id) {
                        return false;
                    }
                    if is_private_external_module_type_symbol(sym_id) {
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
                if should_preserve_alias_symbol_in_type_position(sym_id) {
                    return TypeSymbolResolution::Type(sym_id);
                }
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

        if let Some(value_only) = value_only_candidate.get() {
            TypeSymbolResolution::ValueOnly(value_only)
        } else {
            TypeSymbolResolution::NotFound
        }
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
                if let Some(symbol) = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders) {
                    if (symbol.flags & symbol_flags::ALIAS) != 0 {
                        let mut visited_aliases = Vec::new();
                        if let Some(target_sym_id) =
                            self.resolve_alias_symbol(sym_id, &mut visited_aliases)
                            && target_sym_id != sym_id
                            && self
                                .ctx
                                .binder
                                .get_symbol_with_libs(target_sym_id, &lib_binders)
                                .is_some_and(|target_symbol| {
                                    (target_symbol.flags & symbol_flags::TYPE) != 0
                                })
                        {
                            return Some(target_sym_id.0);
                        }
                    }
                    if (symbol.flags & symbol_flags::TYPE) != 0 {
                        return Some(sym_id.0);
                    }
                }
            }
        }

        let mut sym_id = match self.resolve_qualified_symbol_in_type_position(idx) {
            TypeSymbolResolution::Type(sym_id) => sym_id,
            _ => return None,
        };
        let lib_binders = self.get_lib_binders();
        let mut symbol = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders)?;
        if (symbol.flags & symbol_flags::ALIAS) != 0 {
            let mut visited_aliases = Vec::new();
            if let Some(target_sym_id) = self.resolve_alias_symbol(sym_id, &mut visited_aliases)
                && target_sym_id != sym_id
            {
                sym_id = target_sym_id;
                symbol = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders)?;
            }
        }
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
            for lib_binder in lib_binders.iter() {
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

    /// Resolve a `DefId` from a node index for type lowering.
    ///
    /// This is the canonical stable-identity helper for `def_id_resolver` closures.
    /// It encapsulates the common pattern:
    ///   `resolve_type_symbol_for_lowering(node_idx) → SymbolId → get_or_create_def_id`
    ///
    /// Use this instead of inlining the SymbolId wrapping + DefId creation at each
    /// lowering call site.
    pub(crate) fn resolve_def_id_for_lowering(
        &self,
        node_idx: NodeIndex,
    ) -> Option<tsz_solver::def::DefId> {
        self.resolve_type_symbol_for_lowering(node_idx)
            .map(|sym_id| self.ctx.get_or_create_def_id(tsz_binder::SymbolId(sym_id)))
    }
}
