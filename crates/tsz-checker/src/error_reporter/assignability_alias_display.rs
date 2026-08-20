//! Alias display helpers for assignability diagnostics.

use crate::diagnostics::{diagnostic_messages, format_message};
use crate::query_boundaries::assignability_alias_display as alias_display_queries;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    fn generic_alias_name_from_display(display: &str) -> Option<&str> {
        let display = display.trim_start();
        let (name, _) = display.split_once('<')?;
        let name = name.trim();
        (!name.is_empty()
            && name
                .chars()
                .all(tsz_common::text_scan::is_ascii_identifier_continue_char))
        .then_some(name)
    }

    fn declared_generic_alias_annotation_matches_target_display(
        annotation: &str,
        target_display: &str,
    ) -> bool {
        let Some(annotation_name) = Self::generic_alias_name_from_display(annotation) else {
            return false;
        };
        let Some(target_name) = Self::generic_alias_name_from_display(target_display) else {
            return false;
        };
        annotation_name == target_name
    }

    fn bare_type_parameter_annotation_for_assignment_identifier(
        &mut self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let node = self.ctx.arena.get(expr_idx)?;
        if node.kind != tsz_scanner::SyntaxKind::Identifier as u16 {
            return None;
        }
        let annotation = self.declared_type_annotation_text_for_expression(expr_idx)?;
        let annotation = annotation.trim();
        if annotation.is_empty()
            || !annotation
                .chars()
                .all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric())
        {
            return None;
        }
        let sym_id = self.resolve_identifier_symbol(expr_idx)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        if !symbol.has_any_flags(tsz_binder::symbol_flags::VARIABLE) {
            return None;
        }
        let declared_type = self.get_type_of_symbol(sym_id);
        crate::query_boundaries::diagnostics::is_type_parameter(self.ctx.types, declared_type)
            .then(|| annotation.to_string())
    }

    fn anchor_is_within_object_literal_member(&self, anchor_idx: NodeIndex) -> bool {
        let mut current = anchor_idx;
        let mut guard = 0;

        while current.is_some() {
            guard += 1;
            if guard > 256 {
                break;
            }

            let Some(node) = self.ctx.arena.get(current) else {
                break;
            };
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                break;
            };
            if matches!(
                node.kind,
                k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT
                    || k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT
                    || k == syntax_kind_ext::METHOD_DECLARATION
            ) && let Some(parent_node) = self.ctx.arena.get(ext.parent)
                && parent_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
            {
                return true;
            }
            if matches!(
                node.kind,
                k if k == syntax_kind_ext::VARIABLE_DECLARATION
                    || k == syntax_kind_ext::PARAMETER
                    || k == syntax_kind_ext::RETURN_STATEMENT
                    || k == syntax_kind_ext::BINARY_EXPRESSION
            ) {
                break;
            }
            if ext.parent.is_none() {
                break;
            }
            current = ext.parent;
        }

        false
    }

    pub(in crate::error_reporter) fn declared_generic_alias_source_display_for_target_display(
        &self,
        anchor_idx: NodeIndex,
        source: TypeId,
        source_display: &str,
        target_display: &str,
    ) -> Option<String> {
        let expr_idx = self
            .direct_diagnostic_source_expression(anchor_idx)
            .or_else(|| self.assignment_source_expression(anchor_idx))?;
        let annotation_text = self.declared_type_annotation_text_for_expression(expr_idx)?;
        if annotation_text.contains('<')
            && let Some(annotation_name) = Self::generic_alias_name_from_display(&annotation_text)
            && Self::generic_alias_name_from_display(source_display) == Some(annotation_name)
            && Self::generic_alias_name_from_display(target_display) == Some(annotation_name)
        {
            if alias_display_queries::source_preserves_declared_generic_alias_display(
                self.ctx.types,
                source,
            ) {
                return Some(self.format_declared_annotation_for_diagnostic(&annotation_text));
            }
            return Some(source_display.to_string());
        }
        if !alias_display_queries::source_can_use_declared_generic_alias_annotation(
            self.ctx.types,
            self.ctx.definition_store.as_ref(),
            source,
        ) {
            return None;
        }
        Self::declared_generic_alias_annotation_matches_target_display(
            &annotation_text,
            target_display,
        )
        .then(|| self.format_declared_annotation_for_diagnostic(&annotation_text))
    }

    pub(in crate::error_reporter) fn declared_generic_alias_assignment_pair_display(
        &mut self,
        anchor_idx: NodeIndex,
        source: TypeId,
        target: TypeId,
        source_display: &str,
        target_display: &str,
    ) -> Option<(String, String)> {
        // A `unique symbol` source keeps whatever display the caller resolved:
        // the bare `unique symbol` keyword by default, or the `typeof <name>`
        // form when `render_type_mismatch` disambiguated a distinct
        // unique-symbol pair (`const x: typeof a = b`). Its declared annotation
        // is the bare `unique symbol` keyword, so the annotation/alias repaints
        // below would undo that pair-level disambiguation. Unique symbols are
        // never generic-alias applications, so declining here loses nothing.
        if crate::query_boundaries::type_predicates::is_unique_symbol_type(self.ctx.types, source) {
            return None;
        }
        // A source identifier narrowed away from a declared `unknown`/`any` must
        // keep its narrowed checked-type display (already in `source_display`);
        // the declared-annotation repaints below would otherwise restore the
        // stale top type. tsc renders the narrowed type. This guards both
        // callers (`render_type_mismatch` and the TS2322 message rewrite).
        if self.assignment_source_narrowed_from_declared_top_type(anchor_idx, source) {
            return None;
        }
        // A source identifier flow-narrowed to a strict subset of its declared
        // type (the canonical case being a declared union alias narrowed to a
        // sub-union) keeps its narrowed structural display: `tsc` drops the
        // `aliasSymbol` on `filterType` for a proper subset, so the declared
        // union-alias repaint below would restore a stale broader type. Mirrors
        // the top-type guard above and the source-display narrowing guard in
        // `format_assignment_source_type_for_diagnostic`.
        if let Some(expr_idx) = self
            .direct_diagnostic_source_expression(anchor_idx)
            .or_else(|| self.assignment_source_expression(anchor_idx))
            && let Some(declared_type) = self.declared_type_of_variable_identifier_source(expr_idx)
            && self.source_flow_type_strictly_narrows_declared(source, declared_type)
        {
            return None;
        }
        if let Some(expr_idx) = self
            .direct_diagnostic_source_expression(anchor_idx)
            .or_else(|| self.assignment_source_expression(anchor_idx))
            && let Some(source_display) =
                self.bare_type_parameter_annotation_for_assignment_identifier(expr_idx)
        {
            return Some((source_display, target_display.to_string()));
        }
        if let Some(expr_idx) = self.assignment_target_expression(anchor_idx)
            && let Some(target_display) =
                self.bare_type_parameter_annotation_for_assignment_identifier(expr_idx)
        {
            return Some((source_display.to_string(), target_display));
        }
        let source_fact = self.alias_display_source_fact_type(anchor_idx, source);
        if !self.anchor_is_within_object_literal_member(anchor_idx)
            && let Some(expr_idx) = self.assignment_target_expression(anchor_idx)
            && let Some(annotation_text) =
                self.declared_type_annotation_text_for_expression(expr_idx)
            && annotation_text.contains('<')
            && alias_display_queries::is_application_for_alias_display(self.ctx.types, target)
            && let Some(annotation_name) = Self::generic_alias_name_from_display(&annotation_text)
            && let Some(target_name) = Self::generic_alias_name_from_display(target_display)
            && annotation_name != target_name
        {
            let target_display = self.format_declared_annotation_for_diagnostic(&annotation_text);
            return Some((source_display.to_string(), target_display));
        }
        if let Some(source_display) = self.declared_generic_alias_source_display_for_target_display(
            anchor_idx,
            source_fact,
            source_display,
            target_display,
        ) {
            return Some((source_display, target_display.to_string()));
        }
        if self
            .static_schema_array_structural_display(source_fact, target)
            .is_some()
        {
            return None;
        }
        let expr_idx = self
            .direct_diagnostic_source_expression(anchor_idx)
            .or_else(|| self.assignment_source_expression(anchor_idx))?;
        let annotation_text = self.declared_type_annotation_text_for_expression(expr_idx)?;
        // A non-generic alias whose body is a computed operator collapsing to a
        // shared singleton (or a direct intrinsic/literal) drops its alias symbol
        // in tsc, which displays the underlying scalar. The resolved
        // `source_display` already holds that scalar; repainting it with the
        // alias annotation name (`X1`) would diverge from tsc, so suppress the
        // rewrite for such aliases.
        if self.declared_source_annotation_alias_displayed_as_underlying(expr_idx) {
            return None;
        }
        // A *concrete* indexed access (`Obj["m"]`) is resolved to its member
        // type during type construction in tsc, so the written access never
        // reaches a diagnostic — `source_display` already carries the reduced
        // member. Repainting it with the annotation as written would restore
        // the unreduced surface. Same rule, same owner helper as the
        // missing-property path's guard in
        // `should_prefer_declared_source_annotation_display`; a deferred
        // `T["m"]` still declines and keeps its written spelling.
        let declared_source_type = self.get_type_of_node(expr_idx);
        if self.source_declared_type_reduces_as_concrete_indexed_access(declared_source_type) {
            return None;
        }
        if annotation_text == source_display
            || annotation_text.trim_start().starts_with("typeof ")
            // No structural query for module-import types yet; keep as display fallback.
            || source_display.starts_with("import(")
            || (alias_display_queries::is_object_for_alias_display(self.ctx.types, source_fact)
                && !annotation_text.contains('{'))
            || (!annotation_text.contains('<')
                && alias_display_queries::is_application_for_alias_display(self.ctx.types, source_fact)
                && alias_display_queries::is_application_for_alias_display(self.ctx.types, target))
            || annotation_text.contains(" | ")
            || annotation_text.contains(" & ")
            || annotation_text.contains('<')
            || annotation_text.contains('.')
            || ((alias_display_queries::contains_undefined_for_alias_display(
                self.ctx.types,
                source_fact,
            ) || alias_display_queries::has_optional_parameter_undefined_surface(
                self.ctx.types,
                source_fact,
            ))
                && !annotation_text.contains("| undefined"))
            || alias_display_queries::is_literal_for_alias_display(self.ctx.types, source_fact)
            || alias_display_queries::is_string_intrinsic_for_alias_display(
                self.ctx.types,
                source_fact,
            )
        {
            return None;
        }
        let source_display = self.format_declared_annotation_for_diagnostic(&annotation_text);
        Some((source_display, target_display.to_string()))
    }

    fn alias_display_source_fact_type(
        &mut self,
        anchor_idx: NodeIndex,
        fallback: TypeId,
    ) -> TypeId {
        self.direct_diagnostic_source_expression(anchor_idx)
            .or_else(|| self.assignment_source_expression(anchor_idx))
            .map(|expr_idx| self.get_type_of_node(expr_idx))
            .filter(|type_id| !matches!(*type_id, TypeId::ERROR | TypeId::UNKNOWN))
            .unwrap_or(fallback)
    }

    pub(in crate::error_reporter) fn rewrite_declared_generic_alias_source_in_ts2322_message(
        &mut self,
        anchor_idx: NodeIndex,
        source: TypeId,
        target: TypeId,
        message: String,
    ) -> String {
        let Some(rest) = message.strip_prefix("Type '") else {
            return message;
        };
        let Some((source_display, target_part)) = rest.split_once("' is not assignable to type '")
        else {
            return message;
        };
        let Some(target_display) = target_part.strip_suffix("'.") else {
            return message;
        };
        if let Some((source_display, target_display)) = self
            .declared_generic_alias_assignment_pair_display(
                anchor_idx,
                source,
                target,
                source_display,
                target_display,
            )
        {
            return format_message(
                diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                &[&source_display, &target_display],
            );
        }
        message
    }

    pub(in crate::error_reporter) fn direct_type_param_alias_application_pair_display(
        &self,
        source: TypeId,
        target: TypeId,
    ) -> Option<(String, String)> {
        let (source_base, source_args) = self.application_info_or_display_alias(source)?;
        let (target_base, target_args) = self.application_info_or_display_alias(target)?;
        if source_base != target_base || source_args.len() != target_args.len() {
            return None;
        }
        let (source_arg, target_arg) = self.direct_type_param_alias_application_pair_args(
            source_base,
            &source_args,
            &target_args,
            0,
        )?;
        Some((
            self.format_type_diagnostic(source_arg),
            self.format_type_diagnostic(target_arg),
        ))
    }

    fn direct_type_param_alias_application_pair_args(
        &self,
        base: TypeId,
        source_args: &[TypeId],
        target_args: &[TypeId],
        depth: usize,
    ) -> Option<(TypeId, TypeId)> {
        if depth > 8 || source_args.len() != target_args.len() {
            return None;
        }

        let def_id = crate::query_boundaries::diagnostics::lazy_def_id(self.ctx.types, base)?;
        let def = self.ctx.definition_store.get(def_id)?;
        if def.kind != tsz_solver::def::DefKind::TypeAlias {
            return None;
        }
        let body = def.body?;
        if let Some(param) =
            crate::query_boundaries::diagnostics::type_param_info(self.ctx.types, body)
        {
            let arg_idx = def
                .type_params
                .iter()
                .position(|type_param| type_param.name == param.name)?;
            return Some((*source_args.get(arg_idx)?, *target_args.get(arg_idx)?));
        }

        let (next_base, body_args) =
            crate::query_boundaries::diagnostics::application_info(self.ctx.types, body)?;
        if next_base == base {
            return None;
        }
        let source_args = self.instantiate_alias_application_display_args(
            &def.type_params,
            source_args,
            &body_args,
        )?;
        let target_args = self.instantiate_alias_application_display_args(
            &def.type_params,
            target_args,
            &body_args,
        )?;
        self.direct_type_param_alias_application_pair_args(
            next_base,
            &source_args,
            &target_args,
            depth + 1,
        )
    }

    fn instantiate_alias_application_display_args(
        &self,
        type_params: &[tsz_solver::TypeParamInfo],
        alias_args: &[TypeId],
        body_args: &[TypeId],
    ) -> Option<Vec<TypeId>> {
        if alias_args.len() < type_params.len() {
            return None;
        }
        let substitution = crate::query_boundaries::diagnostics::TypeSubstitution::from_args(
            self.ctx.types,
            type_params,
            &alias_args[..type_params.len()],
        );
        Some(
            body_args
                .iter()
                .map(|&arg| {
                    crate::query_boundaries::diagnostics::instantiate_type(
                        self.ctx.types,
                        arg,
                        &substitution,
                    )
                })
                .collect(),
        )
    }

    /// True when `ty` is a concrete (already-reduced) type whose display-alias
    /// provenance points at a generic application of a conditional-bodied type
    /// alias. `tsc` drops the alias name in this case (showing the resolved
    /// structural form), but the provenance must stay in the interner because
    /// the solver's conditional evaluator reads it (the `Equal<X, Y>`
    /// `any`-distinction trick depends on it); so the caller suppresses only the
    /// application-alias chase when rendering.
    ///
    /// A still-deferred `Conditional`/`IndexAccess`/`Mapped`/generic-application
    /// `ty` is excluded: `tsc` keeps `Tail<T>` for an unreduced generic
    /// conditional.
    pub(in crate::error_reporter) fn reduced_conditional_alias_display_should_skip_application(
        &self,
        ty: TypeId,
    ) -> bool {
        use crate::query_boundaries::diagnostics;
        // Fast path: most formatted types carry no display alias, so this single
        // map lookup short-circuits before the type-kind classification below.
        let Some(alias) = self.ctx.types.get_display_alias(ty) else {
            return false;
        };
        if diagnostics::is_conditional_type(self.ctx.types, ty)
            || diagnostics::is_index_access_type(self.ctx.types, ty)
            || diagnostics::is_mapped_type(self.ctx.types, ty)
            || diagnostics::is_generic_application(self.ctx.types, ty)
        {
            return false;
        }
        crate::query_boundaries::diagnostics::application_base_has_conditional_alias_body(
            self.ctx.types.as_type_database(),
            &self.ctx.definition_store,
            alias,
        )
    }
}
