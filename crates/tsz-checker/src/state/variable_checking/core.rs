//! Variable declaration and destructuring checking.
//!
//! For-in / for-of loop variable checking is in `for_loop.rs`.

include!("core_large_methods/check_variable_declaration_with_request_16_1.rs");

use super::initializer_policy::VarDeclFacts;
use crate::computation::complex::is_contextually_sensitive;
use crate::context::{PendingImplicitAnyKind, PendingImplicitAnyVar, TypingRequest};
use crate::query_boundaries::flow as flow_boundary;
use crate::query_boundaries::state::checking as query;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {}

include!("core/annotation_context.rs");

impl<'a> CheckerState<'a> {
    pub(super) fn bare_type_alias_annotation_declared_type(
        &mut self,
        annotation_idx: NodeIndex,
        resolved_type: TypeId,
    ) -> Option<TypeId> {
        let node = self.ctx.arena.get(annotation_idx)?;
        if node.kind != syntax_kind_ext::TYPE_REFERENCE {
            return None;
        }
        let type_ref = self.ctx.arena.get_type_ref(node)?;
        if type_ref.type_arguments.is_some() {
            return None;
        }
        let crate::symbol_resolver::TypeSymbolResolution::Type(sym_id) =
            self.resolve_identifier_symbol_in_type_position_without_tracking(type_ref.type_name)
        else {
            return None;
        };
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        if !symbol.has_any_flags(tsz_binder::symbol_flags::TYPE_ALIAS) {
            return None;
        }
        // Suppress when the alias body has explicit type arguments
        // (e.g. `type B = A<X>;`). tsc unfolds such aliases at TS2739
        // source display to `A<X>`, so storing the bare alias would lose
        // the unfold target. Bare-reference bodies (`type B = A;` where
        // `A` carries defaults) keep the alias name.
        let body_has_explicit_type_args = symbol.declarations.iter().any(|&decl_idx| {
            let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                return false;
            };
            let Some(alias) = self.ctx.arena.get_type_alias(decl_node) else {
                return false;
            };
            let Some(body_node) = self.ctx.arena.get(alias.type_node) else {
                return false;
            };
            if body_node.kind != syntax_kind_ext::TYPE_REFERENCE {
                return false;
            }
            self.ctx
                .arena
                .get_type_ref(body_node)
                .is_some_and(|body_ref| body_ref.type_arguments.is_some())
        });
        if body_has_explicit_type_args {
            return None;
        }
        let resolves_to_application =
            crate::query_boundaries::common::application_info(self.ctx.types, resolved_type)
                .is_some();
        let resolves_to_named_object =
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, resolved_type)
                .and_then(|shape| shape.symbol)
                .and_then(|target_sym| self.ctx.binder.get_symbol(target_sym))
                .is_some_and(|target_symbol| {
                    target_symbol.has_any_flags(
                        tsz_binder::symbol_flags::CLASS | tsz_binder::symbol_flags::INTERFACE,
                    )
                });
        if !resolves_to_application && !resolves_to_named_object {
            return None;
        }
        let def_id = self.ctx.get_or_create_def_id(sym_id);
        Some(self.ctx.types.lazy(def_id))
    }

    pub(super) fn initializer_supports_binding_pattern_context(
        &self,
        pattern_idx: NodeIndex,
        initializer_idx: NodeIndex,
    ) -> bool {
        let contextual_init = self
            .ctx
            .arena
            .skip_parenthesized_and_assertions(initializer_idx);

        self.ctx
            .arena
            .get(contextual_init)
            .is_some_and(|init_node| match self.ctx.arena.kind_at(pattern_idx) {
                Some(kind) if kind == syntax_kind_ext::ARRAY_BINDING_PATTERN => {
                    init_node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION
                }
                Some(kind) if kind == syntax_kind_ext::OBJECT_BINDING_PATTERN => {
                    init_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                }
                _ => false,
            })
    }

    pub(super) fn declaration_pattern_initializer_request(
        &mut self,
        pattern_idx: NodeIndex,
        initializer_idx: NodeIndex,
        typing_request: &TypingRequest,
    ) -> TypingRequest {
        if !self.initializer_supports_binding_pattern_context(pattern_idx, initializer_idx) {
            return TypingRequest::NONE;
        }

        self.build_contextual_type_from_pattern_with_request(
            pattern_idx,
            &typing_request.read().contextual_opt(None),
        )
        .map_or(TypingRequest::NONE, TypingRequest::with_contextual_type)
    }

    pub(super) fn should_suppress_identifier_initializer_context_for_index_access(
        &mut self,
        initializer_idx: NodeIndex,
        contextual_type: TypeId,
    ) -> bool {
        if self
            .ctx
            .arena
            .get(initializer_idx)
            .is_none_or(|node| node.kind != SyntaxKind::Identifier as u16)
        {
            return false;
        }
        crate::query_boundaries::common::index_access_parts(self.ctx.types, contextual_type)
            .is_some()
    }

    pub(super) fn identifier_initializer_symbol_type_for_index_access_target(
        &mut self,
        initializer_idx: NodeIndex,
        contextual_type: TypeId,
    ) -> Option<TypeId> {
        if self
            .ctx
            .arena
            .get(initializer_idx)
            .is_none_or(|node| node.kind != SyntaxKind::Identifier as u16)
            || crate::query_boundaries::common::index_access_parts(self.ctx.types, contextual_type)
                .is_none()
        {
            return None;
        }
        let sym_id = self.resolve_identifier_symbol(initializer_idx)?;
        self.ctx
            .symbol_types
            .get(&sym_id)
            .copied()
            .filter(|&ty| ty != TypeId::ERROR && ty != TypeId::UNKNOWN)
    }
}

include!("core/jsdoc_enum_and_prior_values.rs");

impl<'a> CheckerState<'a> {
    fn cached_inferred_variable_type(
        &self,
        decl_idx: NodeIndex,
        name_idx: NodeIndex,
    ) -> Option<TypeId> {
        let name_is_binding_pattern = self.ctx.arena.kind_at(name_idx).is_some_and(|kind| {
            kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                || kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
        });

        self.ctx
            .binder
            .get_node_symbol(decl_idx)
            .and_then(|sym_id| self.ctx.symbol_types.get(&sym_id).copied())
            .or_else(|| {
                self.ctx
                    .binder
                    .get_node_symbol(name_idx)
                    .and_then(|sym_id| self.ctx.symbol_types.get(&sym_id).copied())
            })
            .or_else(|| {
                name_is_binding_pattern
                    .then(|| self.ctx.node_types.get(&decl_idx.0).copied())
                    .flatten()
            })
            .or_else(|| {
                name_is_binding_pattern
                    .then(|| self.ctx.node_types.get(&name_idx.0).copied())
                    .flatten()
            })
            .filter(|&type_id| type_id != TypeId::ERROR)
    }

    fn has_prior_value_declaration_for_symbol(&self, decl_idx: NodeIndex) -> bool {
        self.has_prior_value_declaration_for_symbol_impl(decl_idx, false)
    }

    // TS2502 variant: alias-style declarations (imports, namespace exports) do not
    // establish a value-typed binding in the redeclaring scope, so `typeof X` inside
    // a later same-named declaration is genuinely circular.  Use this variant only for
    // the circularity check; the general variant is used for symbol-type caching so
    // that module augmentations cannot overwrite a prior JS-export type.
    fn has_prior_value_declaration_for_ts2502(&self, decl_idx: NodeIndex) -> bool {
        self.has_prior_value_declaration_for_symbol_impl(decl_idx, true)
    }

    fn has_prior_value_declaration_for_symbol_impl(
        &self,
        decl_idx: NodeIndex,
        exclude_aliases: bool,
    ) -> bool {
        let Some(sym_id) = self.ctx.binder.get_node_symbol(decl_idx).or_else(|| {
            self.ctx
                .arena
                .get(decl_idx)
                .and_then(|node| self.ctx.arena.get_variable_declaration(node))
                .and_then(|decl| self.ctx.binder.get_node_symbol(decl.name))
        }) else {
            return false;
        };
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };
        let current_pos = self
            .ctx
            .arena
            .get(decl_idx)
            .map_or(u32::MAX, |node| node.pos);
        // Use source position to find prior declarations rather than
        // relying on declaration-list order. Hoisted `var` declarations
        // appear first in the list (before parameters) even though the
        // parameter appears earlier in source. Source-position ordering
        // correctly identifies the parameter as a prior declaration.
        //
        // Exclude block-scoped (let/const) declarations: when a `const`
        // precedes a `var` of the same name, they occupy different scoping
        // realms and the const should not be treated as a "prior value
        // declaration" for the var (that case is TS2451, not TS2403).
        symbol.declarations.iter().any(|&other| {
            if other == decl_idx || !other.is_some() {
                return false;
            }
            let has_earlier_pos = self
                .ctx
                .arena
                .get(other)
                .is_some_and(|node| node.pos < current_pos);
            if !has_earlier_pos {
                return false;
            }
            // Filter out block-scoped prior declarations (let/const/using).
            // These don't establish a prior value type for function-scoped vars.
            if let Some(other_node) = self.ctx.arena.get(other)
                && other_node.kind == syntax_kind_ext::VARIABLE_DECLARATION
                && let Some(other_ext) = self.ctx.arena.get_extended(other)
                && let Some(other_parent) = self.ctx.arena.get(other_ext.parent)
                && other_parent.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST
            {
                let flags = other_parent.flags as u32;
                use tsz_parser::parser::node_flags;
                if node_flags::is_block_scoped(flags) {
                    return false;
                }
            }
            // When checking for TS2502 circular references, alias-style prior
            // declarations (imports / UMD namespace exports) do not establish a
            // value-typed binding in the redeclaring scope, so `typeof X` inside
            // a later same-named `const X` declaration is genuinely circular.
            // For symbol-type caching we keep imports as valid prior declarations
            // so that module augmentations cannot overwrite a JS-export type.
            if exclude_aliases && let Some(other_node) = self.ctx.arena.get(other) {
                let kind = other_node.kind;
                if kind == syntax_kind_ext::NAMESPACE_IMPORT
                    || kind == syntax_kind_ext::IMPORT_CLAUSE
                    || kind == syntax_kind_ext::IMPORT_SPECIFIER
                    || kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                    || kind == syntax_kind_ext::NAMESPACE_EXPORT_DECLARATION
                    || kind == syntax_kind_ext::NAMESPACE_EXPORT
                    || kind == syntax_kind_ext::EXPORT_SPECIFIER
                {
                    return false;
                }
                // The UMD `export as namespace foo` (and a few namespace-export
                // forms) record the export_clause identifier as the declaration
                // node; check the parent kind to filter that case as well.
                if kind == SyntaxKind::Identifier as u16
                    && let Some(other_ext) = self.ctx.arena.get_extended(other)
                    && let Some(parent_node) = self.ctx.arena.get(other_ext.parent)
                    && (parent_node.kind == syntax_kind_ext::NAMESPACE_EXPORT_DECLARATION
                        || parent_node.kind == syntax_kind_ext::NAMESPACE_EXPORT
                        || parent_node.kind == syntax_kind_ext::IMPORT_CLAUSE
                        || parent_node.kind == syntax_kind_ext::NAMESPACE_IMPORT
                        || parent_node.kind == syntax_kind_ext::IMPORT_SPECIFIER
                        || parent_node.kind == syntax_kind_ext::EXPORT_SPECIFIER)
                {
                    return false;
                }
            }
            true
        })
    }
}

include!("core/precheck_helpers.rs");

impl<'a> CheckerState<'a> {
    /// Check a single variable declaration.
    #[tracing::instrument(level = "trace", skip(self), fields(decl_idx = ?decl_idx))]
    pub(crate) fn check_variable_declaration(&mut self, decl_idx: NodeIndex) {
        self.check_variable_declaration_with_request(decl_idx, &TypingRequest::NONE);
    }

    __tsz_split_core_check_variable_declaration_with_request_16_1!();
}

include!("core/async_jsdoc_return.rs");

#[cfg(test)]
#[path = "core_tests.rs"]
mod core_tests;
