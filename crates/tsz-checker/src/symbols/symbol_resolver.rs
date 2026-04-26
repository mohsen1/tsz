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
use crate::symbols_domain::alias_cycle::AliasCycleTracker;
use crate::symbols_domain::name_text::entity_name_text_in_arena;
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

        let mut current = self.ctx.arena.parent_of(idx);
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
        if !symbol.has_any_flags(symbol_flags::MODULE) {
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
        if !symbol.has_any_flags(symbol_flags::ALIAS) {
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
                            && symbol.has_any_flags(symbol_flags::EXPORT_VALUE);
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
                || symbol.has_any_flags(symbol_flags::VALUE)
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
        //
        // # Why this fallback exists (and why it is NOT a bug)
        //
        // The binder's `resolve_identifier_with_filter` deliberately gates its
        // `lib_binders` traversal on `!self.lib_symbols_merged`. After
        // `merge_lib_contexts_into_binder` runs, the main binder's `file_locals`
        // is supposed to carry every globally-visible lib symbol — so the
        // binder skips re-walking `lib_binders` to avoid re-introducing the
        // symbols it just merged.
        //
        // Phase 3 of the merge intentionally EXCLUDES file_locals belonging to
        // external-module lib files unless the name appears in the lib's
        // `global_augmentations` map (`crates/tsz-binder/src/state/lib_merge.rs`,
        // around the `is_external_module && !global_augmentations.contains_key`
        // check). This prevents module-scoped names like the `class Iterator`
        // in `es2025.iterator.d.ts` from contaminating the global scope of
        // user code that doesn't explicitly augment.
        //
        // BUT: some lookups DO need access to those module-scoped lib symbols
        // (e.g. when generators.rs walks the iterator chain). The fallback
        // below queries `lib_contexts.file_locals` directly so those callers
        // can find the symbol. `should_skip_lib_symbol` filters the candidates
        // to keep the global pollution boundary intact.
        //
        // Robustness audit (PR #B, item 2 in
        // `docs/architecture/ROBUSTNESS_AUDIT_2026-04-26.md`): the audit's
        // initial framing ("the binder has a bug") was misleading — the
        // skip-after-merge is deliberate, and the divergence is a coordinated
        // policy. A future restructure should hoist the merge-phase filter and
        // the checker-side fallback into a single declarative resolver
        // boundary so the policy is co-located.
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
                    // A symbol declared in another external module is
                    // only reachable via explicit import. Reject cross-file
                    // fallback resolutions to such symbols, except where:
                    //  * the owning file is a declaration/script/global
                    //    augmentation source (legitimate global), or
                    //  * the symbol is exported from its module (downstream
                    //    diagnostic paths such as the class initializer
                    //    TS2663 detector rely on resolving these here so
                    //    they can emit a more specific diagnostic).
                    let is_cross_module_private = !self.ctx.symbol_is_from_lib(sym_id)
                        && !symbol.is_umd_export
                        && symbol.decl_file_idx != u32::MAX
                        && symbol.decl_file_idx != self.ctx.current_file_idx as u32
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
                    let is_private_external_module_type = identifier_is_type_position
                        && !symbol.has_any_flags(symbol_flags::VALUE)
                        && is_cross_module_private;
                    // For value position, downstream diagnostic paths (e.g. the
                    // class initializer TS2663 detector) rely on resolving
                    // exported cross-module values here so they can emit a
                    // more specific diagnostic. Only reject truly private
                    // (non-exported) cross-module values.
                    let is_private_external_module_value = !identifier_is_type_position
                        && symbol.has_any_flags(symbol_flags::VALUE)
                        && !symbol.is_exported
                        && is_cross_module_private;
                    if is_private_external_module_type || is_private_external_module_value {
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
                            && symbol.has_any_flags(symbol_flags::EXPORT_VALUE);
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
                let mut visited_aliases = AliasCycleTracker::new();
                Some(
                    self.resolve_alias_symbol(sym_id, &mut visited_aliases)
                        .unwrap_or(sym_id),
                )
            }
            TypeSymbolResolution::ValueOnly(sym_id)
                if self.is_import_equals_type_anchor(sym_id, &lib_binders) =>
            {
                self.ctx.referenced_symbols.borrow_mut().insert(sym_id);
                Some(sym_id)
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

        if let Some(sym_id) =
            self.resolve_unqualified_name_in_enclosing_namespace_for_type_position(idx, name)
        {
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
        //
        // The binder's `resolve_identifier_with_filter` skips `lib_binders` when
        // `self.lib_symbols_merged == true`. The skip is deliberate (see the
        // long comment at `resolve_identifier_symbol` above for why), but the
        // merge phase intentionally excludes external-module lib file_locals
        // unless the name is in `global_augmentations`. For type-position
        // resolution we still need access to those module-scoped lib symbols
        // (e.g. lib types referenced from user augmentations), so we probe
        // `lib_contexts.file_locals` directly here.
        //
        // The `name_in_local_scope` short-circuit ensures local declarations
        // (namespaces, modules) shadow global lib types — without it, an
        // ambient `class Iterator` in a target lib would mask a user-defined
        // namespace-local `Iterator`.
        //
        // Robustness audit (PR #B, item 2): see the matching comment at
        // `resolve_identifier_symbol`. This is the type-position twin of
        // that bypass.
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
                            let mut visited = AliasCycleTracker::new();
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
                let mut visited = AliasCycleTracker::new();
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
            let classify_target_resolution = |target_sym_id: SymbolId| {
                let mut effective_target_id = target_sym_id;
                let target_symbol_has_declared_type_meaning = |sym_id: SymbolId| {
                    let Some(symbol) = self
                        .get_cross_file_symbol(sym_id)
                        .or_else(|| self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders))
                    else {
                        return false;
                    };

                    if !symbol.has_any_flags(symbol_flags::ALIAS)
                        && symbol.has_any_flags(symbol_flags::TYPE)
                    {
                        return true;
                    }

                    symbol.declarations.iter().copied().any(|decl_idx| {
                        let arena = self
                            .ctx
                            .resolve_symbol_file_index(sym_id)
                            .and_then(|file_idx| self.ctx.get_binder_for_file(file_idx))
                            .and_then(|binder| binder.get_arena_for_declaration(sym_id, decl_idx))
                            .or_else(|| self.ctx.binder.get_arena_for_declaration(sym_id, decl_idx))
                            .map_or(self.ctx.arena, |arena| arena.as_ref());

                        arena.get(decl_idx).is_some_and(|node| {
                            node.kind == syntax_kind_ext::INTERFACE_DECLARATION
                                || node.kind == syntax_kind_ext::CLASS_DECLARATION
                                || node.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION
                                || node.kind == syntax_kind_ext::ENUM_DECLARATION
                        })
                    })
                };
                let mut target_flags = self
                    .get_cross_file_symbol(effective_target_id)
                    .or_else(|| {
                        self.ctx
                            .binder
                            .get_symbol_with_libs(effective_target_id, &lib_binders)
                    })
                    .map_or(0, |s| s.flags);

                // Synthetic default-export symbols often exist as bare aliases
                // with no direct TYPE/VALUE flags. Follow the alias before
                // deciding whether the import is usable in type position.
                if (target_flags & symbol_flags::ALIAS) != 0 {
                    if target_symbol_has_declared_type_meaning(effective_target_id) {
                        return TypeSymbolResolution::Type(effective_target_id);
                    }
                    let mut visited_target_aliases = AliasCycleTracker::new();
                    if let Some(alias_target_id) =
                        self.resolve_alias_symbol(effective_target_id, &mut visited_target_aliases)
                        && alias_target_id != effective_target_id
                    {
                        effective_target_id = alias_target_id;
                        target_flags = self
                            .get_cross_file_symbol(effective_target_id)
                            .or_else(|| {
                                self.ctx
                                    .binder
                                    .get_symbol_with_libs(effective_target_id, &lib_binders)
                            })
                            .map_or(0, |s| s.flags);
                    }
                }

                let target_is_namespace_module = (target_flags
                    & (symbol_flags::MODULE
                        | symbol_flags::NAMESPACE_MODULE
                        | symbol_flags::VALUE_MODULE))
                    != 0;
                let target_has_type =
                    (target_flags & (symbol_flags::TYPE | symbol_flags::TYPE_ALIAS)) != 0;
                let target_has_value = (target_flags & symbol_flags::VALUE) != 0;
                let target_is_value_only =
                    target_has_value && !target_has_type && !target_is_namespace_module;

                if target_is_value_only {
                    TypeSymbolResolution::ValueOnly(effective_target_id)
                } else {
                    TypeSymbolResolution::Type(effective_target_id)
                }
            };

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
                    let export_surface_meanings = (expected_name != "*")
                        .then(|| {
                            self.ctx
                                .resolve_import_target_from_file(source_file_idx, module_name)
                        })
                        .flatten()
                        .map(|target_file_idx| {
                            let declarations = self.export_surface_declarations_in_file(
                                target_file_idx,
                                expected_name,
                            );
                            let has_type_position_meaning =
                                declarations.iter().any(|(_, flags, _)| {
                                    (*flags
                                        & (symbol_flags::TYPE
                                            | symbol_flags::NAMESPACE_MODULE
                                            | symbol_flags::VALUE_MODULE))
                                        != 0
                                });
                            let has_runtime_value = declarations
                                .iter()
                                .any(|(_, flags, _)| (*flags & symbol_flags::VALUE) != 0);
                            (has_type_position_meaning, has_runtime_value)
                        });
                    if let Some((has_type_position_meaning, has_runtime_value)) =
                        export_surface_meanings
                        && !has_type_position_meaning
                        && has_runtime_value
                    {
                        return Some(TypeSymbolResolution::ValueOnly(target_sym_id));
                    }
                    // Use get_cross_file_symbol first, then fall back to
                    // get_symbol_with_libs. When the target comes from a
                    // different binder (ambient module, cross-file export),
                    // SymbolId values can collide with the current binder's
                    // symbols, causing incorrect flag lookups.
                    return Some(classify_target_resolution(target_sym_id));
                }
            }
            let mut visited_aliases = AliasCycleTracker::new();
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
                    classify_target_resolution(target_sym_id)
                })
        };

        let should_preserve_alias_symbol_in_type_position = |sym_id: SymbolId| {
            let Some(symbol) = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders) else {
                return false;
            };
            if !symbol.has_any_flags(symbol_flags::ALIAS) {
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
                || symbol.has_any_flags(symbol_flags::VALUE)
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
                && symbol.has_any_flags(symbol_flags::ALIAS)
            {
                if let Some(type_alias_id) = self
                    .ctx
                    .alias_partner_reverse(self.ctx.binder, local_sym_id)
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
                && let Some(ns_sym_id) = self
                    .resolve_unqualified_name_in_enclosing_namespace_for_type_position(idx, name)
                && ns_sym_id != local_sym_id
            {
                return TypeSymbolResolution::Type(ns_sym_id);
            }
            return TypeSymbolResolution::Type(local_sym_id);
        }

        if let Some(sym_id) =
            self.resolve_unqualified_name_in_enclosing_namespace_for_type_position(idx, name)
        {
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
                && symbol.has_any_flags(symbol_flags::ALIAS)
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
            return TypeSymbolResolution::ValueOnly(value_only);
        }

        // Last-resort fallback for `import X = require(...)` namespace
        // anchors in qualified-name type position.
        //
        // When this identifier is the left qualifier of a qualified name
        // (e.g. `server.IServer` where `server` comes from
        // `import server = require('./server')`), upstream filters can
        // reject the alias because cross-arena resolution intermittently
        // loses track of the import-equals target's module flags.  The
        // binder maps this node's identifier to a stable symbol via
        // `get_node_symbol`; fall back to that mapping only when the
        // resolved symbol is an IMPORT_EQUALS_DECLARATION (not a general
        // namespace import, which has its own value/type distinction that
        // must not be bypassed).
        if let Some(sym_id) = self
            .ctx
            .binder
            .get_node_symbol(idx)
            .or_else(|| self.ctx.binder.resolve_identifier(self.ctx.arena, idx))
            && let Some(symbol) = self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders)
            && symbol.has_any_flags(symbol_flags::ALIAS)
            && symbol.declarations.iter().copied().any(|decl_idx| {
                self.ctx
                    .arena
                    .get(decl_idx)
                    .is_some_and(|node| node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION)
            })
        {
            return TypeSymbolResolution::Type(sym_id);
        }

        TypeSymbolResolution::NotFound
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
        entity_name_text_in_arena(self.ctx.arena, idx)
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

        if let Some(cached) = self
            .ctx
            .lowering_entity_name_resolution_cache
            .borrow()
            .get(name)
            .copied()
        {
            return cached;
        }

        let mut segments = name.split('.');
        let root_name = segments.next()?;
        let lib_binders = self.get_lib_binders();
        let mut current_sym = self
            .ctx
            .binder
            .file_locals
            .get(root_name)
            .or_else(|| {
                self.ctx
                    .binder
                    .get_global_type_with_libs(root_name, &lib_binders)
            })
            .or_else(|| {
                self.ctx
                    .global_file_locals_index
                    .as_ref()
                    .and_then(|idx| idx.get(root_name))
                    .and_then(|entries| entries.iter().max_by_key(|(_, sym)| sym.0))
                    .map(|&(_, sym)| sym)
            })
            .or_else(|| {
                lib_binders
                    .iter()
                    .find_map(|binder| binder.file_locals.get(root_name))
            })?;

        for segment in segments {
            let mut visited_aliases = AliasCycleTracker::new();
            current_sym = self
                .resolve_alias_symbol(current_sym, &mut visited_aliases)
                .unwrap_or(current_sym);

            let Some(symbol) = self.get_cross_file_symbol(current_sym).or_else(|| {
                self.ctx
                    .binder
                    .get_symbol_with_libs(current_sym, &lib_binders)
            }) else {
                self.ctx
                    .lowering_entity_name_resolution_cache
                    .borrow_mut()
                    .insert(name.to_string(), None);
                return None;
            };

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
                let mut visited_aliases = AliasCycleTracker::new();
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

            self.ctx
                .lowering_entity_name_resolution_cache
                .borrow_mut()
                .insert(name.to_string(), None);
            return None;
        }

        let mut visited_aliases = AliasCycleTracker::new();
        let resolved_sym = self
            .resolve_alias_symbol(current_sym, &mut visited_aliases)
            .unwrap_or(current_sym);
        let canonical_name = name.rsplit('.').next().unwrap_or(name);
        let def_id = self
            .ctx
            .get_canonical_lib_def_id(canonical_name, resolved_sym);
        self.ctx
            .lowering_entity_name_resolution_cache
            .borrow_mut()
            .insert(name.to_string(), Some(def_id));
        Some(def_id)
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
                    if symbol.has_any_flags(symbol_flags::ALIAS) {
                        let mut visited_aliases = AliasCycleTracker::new();
                        if let Some(target_sym_id) =
                            self.resolve_alias_symbol(sym_id, &mut visited_aliases)
                            && target_sym_id != sym_id
                            && self
                                .ctx
                                .binder
                                .get_symbol_with_libs(target_sym_id, &lib_binders)
                                .is_some_and(|target_symbol| {
                                    target_symbol.has_any_flags(symbol_flags::TYPE)
                                })
                        {
                            return Some(target_sym_id.0);
                        }
                    }
                    if symbol.has_any_flags(symbol_flags::TYPE) {
                        return Some(sym_id.0);
                    }
                }
            }
        }

        let mut sym_id = match self.resolve_qualified_symbol_in_type_position(idx) {
            TypeSymbolResolution::Type(sym_id) => sym_id,
            _ => return None,
        };
        // Use get_cross_file_symbol to avoid SymbolId collisions across binders.
        // When resolving qualified names like `server.IWorkspace`, the SymbolId
        // belongs to server.ts's binder, not the current file's binder. Without
        // this, we'd look up the SymbolId in the wrong binder and potentially
        // get a different symbol with a colliding ID.
        let lib_binders = self.get_lib_binders();
        let mut symbol = self
            .get_cross_file_symbol(sym_id)
            .or_else(|| self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders))?;
        if symbol.has_any_flags(symbol_flags::ALIAS) {
            let mut visited_aliases = AliasCycleTracker::new();
            if let Some(target_sym_id) = self.resolve_alias_symbol(sym_id, &mut visited_aliases)
                && target_sym_id != sym_id
            {
                sym_id = target_sym_id;
                symbol = self
                    .get_cross_file_symbol(sym_id)
                    .or_else(|| self.ctx.binder.get_symbol_with_libs(sym_id, &lib_binders))?;
            }
        }
        symbol.has_any_flags(symbol_flags::TYPE).then_some(sym_id.0)
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
