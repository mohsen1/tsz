//! Generic-type assignability error reporting (TS2322 generic family).
//!
//! Extracted from `assignability.rs` to keep both modules under the 2000-LOC
//! arch-guard limit. These methods remain inherent methods on `CheckerState`,
//! so call sites are unchanged.

use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
use crate::error_reporter::fingerprint_policy::{
    DiagnosticAnchorKind, DiagnosticRenderRequest, RelatedInformationPolicy,
};
use crate::error_reporter::type_display_policy::DiagnosticTypeDisplayRole;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

use super::assignability_type_helpers::{
    display_is_literal_value, has_own_signature_type_params, is_callable_application_type,
};

impl<'a> CheckerState<'a> {
    pub(crate) fn error_type_not_assignable_generic_at(
        &mut self,
        source: TypeId,
        target: TypeId,
        idx: NodeIndex,
    ) {
        let anchor_idx =
            self.resolve_diagnostic_anchor_node(idx, DiagnosticAnchorKind::RewriteAssignment);
        self.error_type_not_assignable_generic_with_anchor(source, target, anchor_idx);
    }

    pub(super) fn error_type_not_assignable_generic_with_anchor(
        &mut self,
        source: TypeId,
        target: TypeId,
        anchor_idx: NodeIndex,
    ) {
        if source == target {
            return;
        }

        // Suppress cascade errors from unresolved types
        if source == TypeId::ERROR
            || target == TypeId::ERROR
            // any is assignable to everything except never — tsc reports TS2322 for any→never
            || (source == TypeId::ANY && target != TypeId::NEVER)
            || target == TypeId::ANY
            || source == TypeId::UNKNOWN
            || target == TypeId::UNKNOWN
        {
            return;
        }

        // Suppress TS2322 for callable types with generic type parameters from outer
        // context. Skip the suppression when both sides have their own signature-level
        // type params — the solver handles generic-to-generic comparison correctly.
        let src_callable = is_callable_application_type(self.ctx.types, source);
        let tgt_callable = is_callable_application_type(self.ctx.types, target);
        let has_type_params =
            crate::query_boundaries::diagnostics::contains_type_parameters(self.ctx.types, source);
        let both_have_own_sig_params = has_own_signature_type_params(self.ctx.types, source)
            && has_own_signature_type_params(self.ctx.types, target);
        let bare_rest_failure_visible =
            crate::query_boundaries::assignability::declared_bare_rest_relation_is_raw_sensitive(
                self.ctx.types,
                &self.ctx,
                source,
                target,
            );
        if src_callable
            && tgt_callable
            && has_type_params
            && !both_have_own_sig_params
            && !bare_rest_failure_visible
        {
            return;
        }

        if let Some(anchor) =
            self.resolve_diagnostic_anchor(anchor_idx, DiagnosticAnchorKind::Exact)
        {
            if is_callable_application_type(self.ctx.types, source)
                && is_callable_application_type(self.ctx.types, target)
                && self.should_suppress_outer_callback_return_assignability(target, anchor_idx)
            {
                return;
            }

            // Precedence gate: suppress fallback TS2322 when a more specific
            // diagnostic is already present at the same span.
            if self.has_more_specific_diagnostic_at_span(anchor.start, anchor.length) {
                return;
            }

            if self.is_nested_same_wrapper_assignment_display_provenance(source, target, anchor_idx)
            {
                return;
            }

            if let Some(missing_props) =
                self.missing_required_properties_from_index_signature_source(source, target)
            {
                // TS2739/TS2741 unfold `type B = A<X>` sources to `A<X>`;
                // otherwise fall through to normal source-role formatting.
                let src_str = if let Some(display) =
                    self.ts2739_alias_of_application_source_display_text(source)
                {
                    display
                } else {
                    self.format_type_for_diagnostic_role(
                        source,
                        DiagnosticTypeDisplayRole::AssignmentSource { target, anchor_idx },
                    )
                };
                let tgt_str = self
                    .checked_js_global_element_access_fallback_target_display(anchor_idx)
                    .unwrap_or_else(|| {
                        self.format_type_for_diagnostic_role(
                            target,
                            DiagnosticTypeDisplayRole::AssignmentTarget { source, anchor_idx },
                        )
                    });
                let (message, code) = if missing_props.len() == 1 {
                    let prop_name = self
                        .ctx
                        .types
                        .resolve_atom_ref(missing_props[0])
                        .to_string();
                    if prop_name.starts_with("__js_ctor_brand_") {
                        // Synthetic brand from JS constructor functions — TSC
                        // doesn't report these as missing properties.
                        return;
                    }
                    if tsz_solver::utils::is_synthetic_private_brand_name(&prop_name) {
                        if let Some((display_prop, owner_name, visibility)) =
                            self.private_or_protected_brand_backing_member_display(target, None)
                        {
                            (
                                self.private_or_protected_assignability_message(
                                    &src_str,
                                    &tgt_str,
                                    &display_prop,
                                    &owner_name,
                                    visibility,
                                    self.property_info_for_display(
                                        source,
                                        self.ctx.types.intern_string(&display_prop),
                                    )
                                    .map(|prop| prop.visibility),
                                ),
                                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                            )
                        } else {
                            (
                                format_message(
                                    diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                                    &[&src_str, &tgt_str],
                                ),
                                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                            )
                        }
                    } else {
                        (
                            format_message(
                                diagnostic_messages::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE,
                                &[&prop_name, &src_str, &tgt_str],
                            ),
                            diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE,
                        )
                    }
                } else {
                    let prop_list: Vec<String> = missing_props
                        .iter()
                        .take(4)
                        .map(|name| self.ctx.types.resolve_atom_ref(*name).to_string())
                        .collect();
                    let props_joined = prop_list.join(", ");
                    if missing_props.len() > 4 {
                        let more_count = (missing_props.len() - 4).to_string();
                        (
                            format_message(
                                diagnostic_messages::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE_AND_MORE,
                                &[&src_str, &tgt_str, &props_joined, &more_count],
                            ),
                            diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE_AND_MORE,
                        )
                    } else {
                        (
                            format_message(
                                diagnostic_messages::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE,
                                &[&src_str, &tgt_str, &props_joined],
                            ),
                            diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE,
                        )
                    }
                };
                self.emit_render_request_at_anchor(
                    anchor,
                    DiagnosticRenderRequest::simple(DiagnosticAnchorKind::Exact, code, message),
                );
                return;
            }

            let src_str = self.format_type_for_diagnostic_role(
                source,
                DiagnosticTypeDisplayRole::AssignmentSource { target, anchor_idx },
            );
            let tgt_str = self.format_type_for_diagnostic_role(
                target,
                DiagnosticTypeDisplayRole::AssignmentTarget { source, anchor_idx },
            );
            let (src_str, tgt_str) =
                self.finalize_pair_display_for_diagnostic(source, target, src_str, tgt_str);
            let mut src_str = src_str;
            let mut tgt_str = tgt_str;
            let source_is_direct_type_query_primitive = self
                .direct_diagnostic_source_expression(anchor_idx)
                .or_else(|| self.assignment_source_expression(anchor_idx))
                .and_then(|expr_idx| {
                    self.direct_type_query_primitive_source_display(expr_idx, source)
                })
                .is_some_and(|display| {
                    if display != src_str {
                        src_str = display;
                    }
                    true
                });
            let source_expr_idx = self
                .assignment_source_expression(anchor_idx)
                .or_else(|| self.direct_diagnostic_source_expression(anchor_idx));
            if !source_is_direct_type_query_primitive
                && let Some(expr_idx) = source_expr_idx
                && !self.declared_identifier_has_literal_only_alias_source(expr_idx)
                && let Some(display) =
                    self.declared_identifier_source_display(expr_idx, target, source)
                && self.declared_identifier_candidate_preserves_source_surface(&src_str, &display)
            {
                src_str = display;
            }
            if self.ctx.compiler_options.exact_optional_property_types
                && let Some(expr_idx) = source_expr_idx
                && let Some(annotation_text) =
                    self.declared_type_annotation_text_for_expression(expr_idx)
                && annotation_text.contains("?:")
                && annotation_text.contains("=>")
            {
                let display = self.format_declared_annotation_for_diagnostic(&annotation_text);
                if display != tgt_str {
                    src_str = display;
                }
            }
            if !source_is_direct_type_query_primitive
                && let Some(display) = self.nonmissing_ts2739_alias_source_display_text(source)
            {
                src_str = display;
            }
            if tgt_str.trim() != "{}"
                && let Some(unfolded) = self.ts2739_alias_target_display(target, &tgt_str)
            {
                tgt_str = self.format_type_diagnostic(unfolded);
            }
            if let Some(display) = self.declared_generic_alias_source_display_for_target_display(
                anchor_idx, source, &src_str, &tgt_str,
            ) {
                src_str = display;
            }
            let mut static_schema_display = false;
            if let Some(display) = self.static_schema_array_structural_display(source, target) {
                src_str = display;
                static_schema_display = true;
            }
            if let Some(display) = self.static_schema_array_structural_display(target, source) {
                tgt_str = display;
                static_schema_display = true;
            }
            if let Some(display) =
                self.static_schema_array_structural_display_text(&tgt_str, source)
            {
                tgt_str = display;
                static_schema_display = true;
            }
            if let Some(display) =
                self.static_schema_type_parameter_array_constraint_display(target, &tgt_str, source)
            {
                tgt_str = display;
                static_schema_display = true;
            }
            if !static_schema_display
                && let Some((direct_source, direct_target)) =
                    self.direct_type_param_alias_application_pair_display(source, target)
            {
                src_str = direct_source;
                tgt_str = direct_target;
            }
            if let Some(display) = self.type_query_static_array_structural_display(&src_str) {
                src_str = display;
            }
            let source_from_annotation = self
                .direct_diagnostic_source_expression(anchor_idx)
                .or_else(|| self.assignment_source_expression(anchor_idx))
                .and_then(|expr_idx| {
                    self.declared_numeric_literal_union_alias_source_display(expr_idx, source)
                })
                .map(|display| {
                    src_str = display;
                })
                .is_some();
            if !source_from_annotation {
                src_str = self
                    .canonicalize_assignment_numeric_literal_union_display(source, target, src_str);
            }
            tgt_str =
                self.canonicalize_assignment_numeric_literal_union_display(target, source, tgt_str);
            // A source union whose top-level `null`/`undefined` was stripped by
            // the target-only display policy can collapse to the target's
            // display (e.g. `string[] | undefined` rendered as `string[]`).
            // `tsc` keeps the source nullish; restore it so the duplicate-name
            // TS2719 gate below does not misfire where `tsc` reports a plain
            // TS2322.
            if let Some(restored) = self.source_display_preserving_nullish_if_collapsed_to_target(
                source, target, &src_str, &tgt_str,
            ) {
                src_str = restored;
            }

            // TS2719: when both types display identically but are different,
            // emit "Two different types with this name exist" instead of TS2322.
            let authoritative_src = self.authoritative_assignability_def_name(source);
            let authoritative_tgt = self.authoritative_assignability_def_name(target);
            let authoritative_names_differ = authoritative_src
                .as_ref()
                .zip(authoritative_tgt.as_ref())
                .is_some_and(|(src, tgt)| src != tgt);

            // Do not repaint literal displays as boxed/wrapper interfaces via
            // authoritative-name fallback.
            let display_is_literal_value = display_is_literal_value;

            // Literal-value display pairs are not distinct nominal types; use
            // the regular TS2322 path instead of TS2719.
            let pair_is_literal_value =
                display_is_literal_value(&src_str) && display_is_literal_value(&tgt_str);
            let exact_optional_structural_pair =
                self.ctx.compiler_options.exact_optional_property_types
                    && src_str.contains("?:")
                    && tgt_str.contains("?:");
            let (message, code) = if src_str == tgt_str
                && !authoritative_names_differ
                && !pair_is_literal_value
                && !exact_optional_structural_pair
                && !bare_rest_failure_visible
            {
                (
                    format_message(
                        diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE_TWO_DIFFERENT_TYPES_WITH_THIS_NAME_EXIST_BUT_THEY,
                        &[&src_str, &tgt_str],
                    ),
                    diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE_TWO_DIFFERENT_TYPES_WITH_THIS_NAME_EXIST_BUT_THEY,
                )
            } else {
                let source_generic_base = src_str.split_once('<').map(|(base, _)| base);
                let target_generic_base = tgt_str.split_once('<').map(|(base, _)| base);
                let preserve_generic_nominal_pair = src_str.contains('<')
                    && tgt_str.contains('<')
                    && authoritative_src == authoritative_tgt
                    && source_generic_base == target_generic_base
                    && authoritative_src.as_deref() == source_generic_base;
                let source_name = if src_str.starts_with("typeof ")
                    || src_str.starts_with("import(")
                    || src_str.starts_with('{')
                    || src_str.contains('<')
                    || source_is_direct_type_query_primitive
                    || preserve_generic_nominal_pair
                    || display_is_literal_value(&src_str)
                {
                    src_str.as_str()
                } else {
                    authoritative_src.as_deref().unwrap_or(&src_str)
                };
                let target_name = if tgt_str.starts_with("typeof ")
                    || tgt_str.starts_with("import(")
                    || tgt_str.starts_with('{')
                    || tgt_str.contains('<')
                    || preserve_generic_nominal_pair
                    || display_is_literal_value(&tgt_str)
                {
                    tgt_str.as_str()
                } else {
                    authoritative_tgt.as_deref().unwrap_or(&tgt_str)
                };
                (
                    format_message(
                        diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                        &[source_name, target_name],
                    ),
                    diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                )
            };
            if let Some(related) = self.unrelated_type_parameter_target_related_info(
                source,
                target,
                &src_str,
                &tgt_str,
                anchor.start,
                anchor.length,
                0,
            ) {
                self.emit_render_request_at_anchor(
                    anchor,
                    DiagnosticRenderRequest::with_related(
                        DiagnosticAnchorKind::Exact,
                        code,
                        message,
                        vec![related],
                        RelatedInformationPolicy::ELABORATION,
                    ),
                );
            } else {
                self.emit_render_request_at_anchor(
                    anchor,
                    DiagnosticRenderRequest::simple(DiagnosticAnchorKind::Exact, code, message),
                );
            }
        }
    }
}
