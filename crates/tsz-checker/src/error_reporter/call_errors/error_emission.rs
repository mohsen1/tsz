//! Call error emission functions (TS2345, TS2554, TS2769, etc.).

use crate::diagnostics::{
    DiagnosticCategory, DiagnosticRelatedInformation, RelatedInformationKind, diagnostic_codes,
    diagnostic_messages, format_message,
};
use crate::error_reporter::fingerprint_policy::{
    DiagnosticAnchorKind, DiagnosticRenderRequest, RelatedInformationPolicy,
    ResolvedDiagnosticAnchor,
};
use crate::error_reporter::type_display_policy::DiagnosticTypeDisplayRole;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn should_suppress_argument_not_assignable_diagnostic(
        &mut self,
        arg_type: TypeId,
        param_type: TypeId,
    ) -> bool {
        let bare_rest_failure_visible =
            crate::query_boundaries::assignability::declared_bare_rest_relation_is_raw_sensitive(
                self.ctx.types,
                &self.ctx,
                arg_type,
                param_type,
            );
        // Suppress when types are identical or either is a special escape-hatch type.
        // `unknown` as the ARGUMENT is deliberately excluded: unlike `any`/`error`,
        // `unknown` is not an escape hatch — it's assignable only to `any`/`unknown`,
        // so a mismatch against a concrete or type-parameter target is a real TS2345.
        if arg_type == param_type
            || arg_type == TypeId::ERROR
            || param_type == TypeId::ERROR
            // `any` suppresses most call-site assignability errors, but tsc still
            // reports TS2345 for the bottom-type case `any -> never`.
            || (arg_type == TypeId::ANY && param_type != TypeId::NEVER)
            || param_type == TypeId::ANY
            || param_type == TypeId::UNKNOWN
        {
            return true;
        }
        if (param_type == TypeId::NEVER
            || self.evaluate_type_for_assignability(param_type) == TypeId::NEVER)
            && self.generic_indexed_access_argument_surface(arg_type)
        {
            return true;
        }

        if !bare_rest_failure_visible
            && crate::query_boundaries::assignability::are_types_structurally_identical(
                self.ctx.types,
                &self.ctx,
                arg_type,
                param_type,
            )
        {
            return true;
        }

        let evaluated_arg = self.evaluate_type_for_assignability(arg_type);
        let evaluated_param = self.evaluate_type_for_assignability(param_type);
        !bare_rest_failure_visible
            && crate::query_boundaries::diagnostics::same_non_class_nominal_application_surface(
                self.ctx.types,
                &self.ctx,
                &self.ctx.definition_store,
                &[arg_type, evaluated_arg],
                &[param_type, evaluated_param],
            )
    }

    /// Report an argument not assignable error using solver diagnostics with source tracking.
    /// When solver failure analysis identifies a specific reason (e.g. missing property),
    /// the detailed diagnostic is emitted as related information matching tsc's behavior.
    pub fn error_argument_not_assignable_at(
        &mut self,
        arg_type: TypeId,
        param_type: TypeId,
        idx: NodeIndex,
    ) {
        self.error_argument_not_assignable_at_impl(arg_type, param_type, idx, false);
    }

    pub(crate) fn error_argument_not_assignable_structural_tuple_at(
        &mut self,
        arg_type: TypeId,
        param_type: TypeId,
        idx: NodeIndex,
    ) {
        self.error_argument_not_assignable_at_impl(arg_type, param_type, idx, true);
    }

    /// Promote a sole/grouped missing-required-property argument failure to the
    /// PRIMARY diagnostic at the argument node — TS2741 (one property), TS2739
    /// (a few), TS2740 (many) — replacing the generic TS2345 head, exactly as
    /// tsc's `reportUnmatchedProperty` runs before the relation head message.
    ///
    /// The renderer owns the guard set (intersection targets, index-signature
    /// member compat, primitive/`object` sources keep the generic wording), so
    /// this promotes exactly when it selected the property-missing family.
    ///
    /// This is shared by every call-argument emitter so the promotion cannot be
    /// bypassed by a display-only branch: in particular
    /// `error_argument_not_assignable_preserving_param_display` (the "preserve
    /// the generic parameter display" fallback) must still honor it, otherwise a
    /// target that merely *contains* a free type parameter — e.g. a class merged
    /// with a generic-base interface — would drop the missing-property
    /// elaboration and emit a bare TS2345 (#17145). Returns `true` when it
    /// emitted a promoted diagnostic; the caller must then stop.
    pub(crate) fn try_promote_missing_property_argument(
        &mut self,
        analysis: &crate::query_boundaries::assignability::AssignabilityFailureAnalysis,
        arg_type: TypeId,
        param_type: TypeId,
        idx: NodeIndex,
    ) -> bool {
        let Some(reason) = analysis.failure_reason.as_ref() else {
            return false;
        };
        if !matches!(
            reason,
            tsz_solver::SubtypeFailureReason::MissingProperty { .. }
                | tsz_solver::SubtypeFailureReason::MissingProperties { .. }
        ) || !self.missing_property_head_promotion_applies(arg_type, param_type)
        {
            return false;
        }
        // Render the source through the argument-context display pipeline
        // (fresh object-literal widening included) — the same policy the
        // TS2345 head applied to its argument string; the renderer's
        // assignment-oriented role cannot reproduce it.
        let source_display = Some(self.format_type_for_diagnostic_role(
            arg_type,
            DiagnosticTypeDisplayRole::CallArgument {
                parameter: param_type,
                argument_idx: idx,
            },
        ));
        let diag = self.render_failure_reason_with_source_display(
            reason,
            arg_type,
            param_type,
            idx,
            0,
            source_display,
        );
        if matches!(
            diag.code,
            diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE
                | diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE
                | diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE_AND_MORE
        ) {
            self.ctx.push_diagnostic(diag);
            return true;
        }
        false
    }

    fn error_argument_not_assignable_at_impl(
        &mut self,
        arg_type: TypeId,
        param_type: TypeId,
        idx: NodeIndex,
        structural_tuple_display: bool,
    ) {
        let suppress_argument =
            self.should_suppress_argument_not_assignable_diagnostic(arg_type, param_type);
        if suppress_argument {
            return;
        }
        if self.should_suppress_constraint_cascade_constructor_argument(arg_type, param_type) {
            return;
        }

        if self.should_suppress_partial_self_argument_mismatch(arg_type, param_type) {
            return;
        }
        if self.should_suppress_self_referential_mapped_constraint_arg_mismatch(
            arg_type, param_type, idx,
        ) {
            return;
        }
        if self
            .should_suppress_promise_then_nullable_callback_arg_mismatch(arg_type, param_type, idx)
        {
            return;
        }
        if self.is_callback_like_argument(idx)
            && self.is_assignable_via_generator_never_yield_callback(arg_type, param_type)
        {
            return;
        }
        if self
            .numeric_enum_assignment_override_from_source(arg_type, param_type, idx)
            .is_some_and(|allowed| allowed)
        {
            return;
        }

        // Suppress cascading TS2345 when TS2353 (excess property) already covers this span.
        if let Some(anchor) = self.resolve_diagnostic_anchor(idx, DiagnosticAnchorKind::Exact) {
            let arg_end = anchor.start.saturating_add(anchor.length);
            if self.ctx.diagnostics.iter().any(|diag| {
                diag.code
                    == diagnostic_codes::OBJECT_LITERAL_MAY_ONLY_SPECIFY_KNOWN_PROPERTIES_AND_DOES_NOT_EXIST_IN_TYPE
                    && diag.start >= anchor.start
                    && diag.start < arg_end
            }) {
                return;
            }
        }
        // Suppress TS2345 for callbacks with unannotated parameters that rely on
        // contextual typing, but ONLY when contextual typing genuinely failed to
        // resolve parameter types (they remained `any`/`unknown`).
        // When contextual typing DID resolve concrete types and the mismatch
        // persists, the error is real — e.g., individual params `(a: 1|2, b: "1"|"2")`
        // vs a readonly tuple union rest parameter `(...args: readonly [1, "1"] | readonly [2, "2"])`.
        //
        // Additionally, only suppress when the target signature actually has a
        // parameter at every position the source callback declares. If the
        // target has fewer parameters than the source (and no rest), contextual
        // typing cannot supply types for the extra source parameters and the
        // parameter-count mismatch ("Target signature provides too few
        // arguments") must surface as TS2345 — see issue #4027.
        if self.arg_is_callback_with_unannotated_params(idx)
            && self.callback_type_params_are_unresolved(arg_type)
            && self.target_can_contextually_type_callback_params(idx, param_type)
        {
            return;
        }
        if self.try_elaborate_array_literal_mismatch_from_failure_reason(idx, arg_type, param_type)
        {
            return;
        }
        if self.try_elaborate_callback_body_diagnostics(idx, param_type) {
            return;
        }
        // Promote a readonly-array/tuple → mutable-array/tuple argument mismatch to
        // TS4104 ("The type 'X' is 'readonly' and cannot be assigned to the mutable
        // type 'Y'"), matching the direct-assignment path
        // (`check_assignable_or_report_at_with_options`) and tsc, which reports
        // TS4104 rather than the generic TS2345 for this reason.
        if let Some(reason) = self.readonly_to_mutable_array_or_tuple_reason(arg_type, param_type) {
            let source_display = Some(self.format_type_for_diagnostic_role(
                arg_type,
                DiagnosticTypeDisplayRole::CallArgument {
                    parameter: param_type,
                    argument_idx: idx,
                },
            ));
            let diag = self.render_failure_reason_with_source_display(
                &reason,
                arg_type,
                param_type,
                idx,
                0,
                source_display,
            );
            self.ctx.push_diagnostic(diag);
            return;
        }
        let analysis = self.analyze_assignability_failure(arg_type, param_type);

        // A private/`#`-private brand mismatch between argument and parameter
        // is a nominal-identity failure, not a structural one: tsc's
        // `checkTypeRelatedTo` attaches the "separate declarations of a
        // private property" (modifier-`private`/`protected`) or "refers to a
        // different member" (`#`-private) detail as an elaboration under the
        // TS2345 head, ahead of the generic structural-property rendering
        // below. This mirrors the assignment-statement path's
        // `private_brand_mismatch_error` interception in
        // `error_reporter/assignability.rs`.
        if let Some(detail) = self.private_brand_mismatch_error(arg_type, param_type) {
            let Some(anchor) = self.resolve_diagnostic_anchor(idx, DiagnosticAnchorKind::Exact)
            else {
                return;
            };
            let arg_str = self.format_type_for_diagnostic_role(
                arg_type,
                DiagnosticTypeDisplayRole::CallArgument {
                    parameter: param_type,
                    argument_idx: idx,
                },
            );
            let param_str = self.format_type_for_diagnostic_role(
                param_type,
                DiagnosticTypeDisplayRole::CallParameter {
                    argument: arg_type,
                    argument_idx: idx,
                },
            );
            let (code, msg_template) =
                self.argument_not_assignable_code_and_template(arg_type, param_type);
            let message = format_message(msg_template, &[&arg_str, &param_str]);
            let related = vec![DiagnosticRelatedInformation {
                category: DiagnosticCategory::Error,
                code,
                file: self.ctx.file_name.clone(),
                start: anchor.start,
                length: anchor.length,
                message_text: detail,
                depth: 0,
                kind: RelatedInformationKind::ChainLink,
            }];
            self.emit_render_request_at_anchor(
                anchor,
                DiagnosticRenderRequest::with_related(
                    DiagnosticAnchorKind::Exact,
                    code,
                    message,
                    related,
                    RelatedInformationPolicy::ELABORATION,
                ),
            );
            return;
        }

        // tsc promotes a sole/grouped missing-required-property failure to the
        // PRIMARY diagnostic at the argument node (TS2741/TS2739/TS2740).
        if self.try_promote_missing_property_argument(&analysis, arg_type, param_type, idx) {
            return;
        }

        // When the failure reason is NoCommonProperties (weak types with no
        // properties in common), tsc emits TS2559 directly instead of TS2345.
        // If the source is callable/constructable and calling it would produce a
        // compatible type, tsc emits TS2560 ("did you mean to call it?") instead.
        // Use the unwidened literal type for the diagnostic message — tsc preserves
        // literal types (e.g., "12" not "number", "false" not "boolean") in
        // "has no properties in common" messages.
        if matches!(
            &analysis.failure_reason,
            Some(tsz_solver::SubtypeFailureReason::NoCommonProperties { .. })
        ) {
            // Try to get the literal expression display (unwidened) from the AST
            let mut arg_str = self
                .literal_call_argument_display(idx)
                .unwrap_or_else(|| self.format_type_diagnostic(arg_type));
            arg_str = self.rewrite_source_display_for_non_literal_target_assignability(
                arg_type, param_type, arg_str,
            );
            let param_str = self.format_type_for_diagnostic_role(
                param_type,
                DiagnosticTypeDisplayRole::WeakCallParameter {
                    argument: arg_type,
                    argument_idx: idx,
                },
            );

            // Check if the source is callable/constructable and calling would fix
            // the type mismatch — if so, emit TS2560 instead of TS2559.
            let (msg_template, code) = if self
                .should_suggest_calling_for_weak_type(arg_type, param_type)
            {
                (
                    diagnostic_messages::VALUE_OF_TYPE_HAS_NO_PROPERTIES_IN_COMMON_WITH_TYPE_DID_YOU_MEAN_TO_CALL_IT,
                    diagnostic_codes::VALUE_OF_TYPE_HAS_NO_PROPERTIES_IN_COMMON_WITH_TYPE_DID_YOU_MEAN_TO_CALL_IT,
                )
            } else {
                (
                    diagnostic_messages::TYPE_HAS_NO_PROPERTIES_IN_COMMON_WITH_TYPE,
                    diagnostic_codes::TYPE_HAS_NO_PROPERTIES_IN_COMMON_WITH_TYPE,
                )
            };
            if code
                == diagnostic_codes::VALUE_OF_TYPE_HAS_NO_PROPERTIES_IN_COMMON_WITH_TYPE_DID_YOU_MEAN_TO_CALL_IT
            {
                arg_str = self.widen_weak_type_callable_source_display(arg_type, arg_str);
            }
            let (arg_str, param_str) =
                self.finalize_pair_display_for_diagnostic(arg_type, param_type, arg_str, param_str);
            let message = format_message(msg_template, &[&arg_str, &param_str]);
            let request =
                DiagnosticRenderRequest::simple(DiagnosticAnchorKind::Exact, code, message);
            self.emit_render_request(idx, request);
            return;
        }

        let mut arg_str = self.format_type_for_diagnostic_role(
            arg_type,
            DiagnosticTypeDisplayRole::CallArgument {
                parameter: param_type,
                argument_idx: idx,
            },
        );
        // The type whose plain render `arg_str` currently holds; `None` once a
        // display override replaced the render with a string that was not
        // produced from `arg_type` (overrides render their own types and
        // carry no literal annotations to widen).
        let mut arg_display_type = Some(arg_type);
        // An enum-member argument generalizes to its parent enum when the
        // parameter could not hold a top-level singleton type (tsc
        // `reportRelationError`), mirroring the TS2322 assignment surface:
        // `sip(g)` with `g: EG.A` against `boolean` renders `EG`, while a
        // literal/template/enum parameter preserves `EG.A`.
        if let Some(widened) = self.widened_enum_member_assignment_source(arg_type, param_type) {
            // Plain structural render: the `CallArgument` role would repaint
            // the display from the argument expression's declared annotation
            // (`EG.A`), undoing the widening.
            arg_str = self.format_type_for_assignability_message(widened);
            arg_display_type = Some(widened);
        }
        // Widen a fresh boolean-literal array source (`true[]`/`false[]`) to
        // `boolean[]` against a `boolean` parameter. The decision is structural;
        // the output string is plain rendering (no rendered-text decision, §25).
        if param_type == TypeId::BOOLEAN
            && crate::query_boundaries::diagnostics::boolean_literal_array_display_type(
                self.ctx.types,
                arg_type,
            )
            .is_some()
        {
            arg_str = "boolean[]".to_string();
            arg_display_type = None;
        }
        let mut param_str = self.format_type_for_diagnostic_role(
            param_type,
            DiagnosticTypeDisplayRole::CallParameter {
                argument: arg_type,
                argument_idx: idx,
            },
        );
        // The type whose plain render `param_str` currently holds (same
        // tracking discipline as `arg_display_type`).
        let mut param_display_type = Some(param_type);
        if let Some(display) = self.mapped_property_mismatch_parameter_display(
            &param_str,
            analysis.failure_reason.as_ref(),
        ) {
            param_str = display;
            param_display_type = None;
        }
        if let Some(display) =
            self.constrained_variadic_tuple_parameter_display(param_type, arg_type)
        {
            param_str = display;
            param_display_type = None;
        }
        if structural_tuple_display {
            if crate::query_boundaries::common::tuple_elements(self.ctx.types, arg_type).is_some() {
                arg_str = self
                    .format_type_for_assignability_message_anonymous_composite_structural(arg_type);
                arg_display_type = None;
            }
            if crate::query_boundaries::common::tuple_elements(self.ctx.types, param_type).is_some()
            {
                param_str = self
                    .format_type_for_assignability_message_anonymous_composite_structural(
                        param_type,
                    );
                param_display_type = None;
            }
        }
        if arg_str.starts_with('{')
            && param_str.contains("<{")
            && let Some(display_ty) = param_display_type
        {
            // Generic parameter displays widen string/boolean literal
            // annotations of objects nested in the application's type
            // arguments: widen at the type level and reprint (#13075).
            let widened = self.widen_annotation_literals_for_display(
                display_ty,
                crate::query_boundaries::diagnostics::AnnotationLiteralWideningPolicy::STRINGS_AND_BOOLEANS_INSIDE_APPLICATION_ARGS,
            );
            if widened.display_residue {
                // Literal spellings live only in display provenance; render
                // the canonical (display-property-free) form.
                param_str = self.format_type_diagnostic_widened(widened.type_id);
            } else if widened.type_id != display_ty {
                param_str = self.format_type_for_diagnostic_role(
                    widened.type_id,
                    DiagnosticTypeDisplayRole::CallParameter {
                        argument: arg_type,
                        argument_idx: idx,
                    },
                );
            }
        }
        if arg_str.starts_with('{')
            && let Some(display_ty) = param_display_type
            && self.ctx.types.get_display_properties(display_ty).is_some()
            && !self.target_preserves_literal_surface(param_type)
        {
            // Parameters inferred from another fresh object literal can carry
            // literal spellings only through display provenance (for example
            // `NoInfer<T>` when `T` was inferred from `{ x: 3, y: 2 }`).
            // tsc renders these parameter surfaces as annotation-like object
            // members, so widen the display-property literals at the type
            // level and print without the stale literal side table (#13075).
            let widened = self.widen_annotation_literals_for_display(
                display_ty,
                crate::query_boundaries::diagnostics::AnnotationLiteralWideningPolicy::ALL,
            );
            if widened.display_residue {
                param_str = self.format_type_diagnostic_widened(widened.type_id);
            } else if widened.type_id != display_ty {
                param_str = self.format_type_for_diagnostic_role(
                    widened.type_id,
                    DiagnosticTypeDisplayRole::CallParameter {
                        argument: arg_type,
                        argument_idx: idx,
                    },
                );
            }
        }
        if let Some((generic_arg_str, generic_param_str)) =
            self.generic_direct_primitive_mismatch_display(arg_type, param_type, idx)
        {
            arg_str = generic_arg_str;
            param_str = generic_param_str;
            arg_display_type = None;
        }
        if let Some(widened_arg_str) = self
            .widen_literal_call_argument_display_against_plain_primitive_parameter(
                arg_type, idx, &param_str,
            )
        {
            arg_str = widened_arg_str;
            arg_display_type = None;
        }
        if self.inline_literal_satisfies_has_permissive_target(idx)
            && let Some(display_ty) = arg_display_type
        {
            // Widen the argument display's literal annotations at the type
            // level and reprint (#13075).
            let widened = self.widen_annotation_literals_for_display(
                display_ty,
                crate::query_boundaries::diagnostics::AnnotationLiteralWideningPolicy::ALL,
            );
            if widened.display_residue {
                // Literal spellings live only in display provenance; render
                // the canonical (display-property-free) form.
                arg_str = self.format_type_diagnostic_widened(widened.type_id);
            } else if widened.type_id != display_ty {
                arg_str = self.format_type_for_diagnostic_role(
                    widened.type_id,
                    DiagnosticTypeDisplayRole::CallArgument {
                        parameter: param_type,
                        argument_idx: idx,
                    },
                );
            }
        }
        param_str = Self::trim_single_unbalanced_trailing_type_arg_close(param_str);
        let (arg_str, param_str) =
            self.finalize_pair_display_for_diagnostic(arg_type, param_type, arg_str, param_str);
        let (code, msg_template) =
            self.argument_not_assignable_code_and_template(arg_type, param_type);
        let message = format_message(msg_template, &[&arg_str, &param_str]);

        let request = if let Some(reason) = analysis.failure_reason {
            DiagnosticRenderRequest::with_failure_reason(
                DiagnosticAnchorKind::Exact,
                code,
                message,
                reason,
                arg_type,
                param_type,
            )
        } else {
            DiagnosticRenderRequest::simple(DiagnosticAnchorKind::Exact, code, message)
        };

        self.emit_render_request(idx, request);
    }

    /// Pick the diagnostic code and message template for an argument-not-assignable error.
    ///
    /// Under `exactOptionalPropertyTypes`, when the only reason the argument doesn't fit
    /// the parameter is that the argument has an explicit `| undefined` on a property
    /// the parameter declares as `?`-optional-without-undefined, tsc emits TS2379 with the
    /// "with 'exactOptionalPropertyTypes: true'. Consider adding 'undefined' to the types
    /// of the target's properties." helper text instead of TS2345. This mirrors the
    /// TS2375 (vs. TS2322) split on the assignment-context path.
    pub(crate) fn argument_not_assignable_code_and_template(
        &mut self,
        arg_type: TypeId,
        param_type: TypeId,
    ) -> (u32, &'static str) {
        if self.has_exact_optional_property_mismatch(arg_type, param_type) {
            (
                diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE_WITH_EXACTOPTIONALPROPER,
                diagnostic_messages::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE_WITH_EXACTOPTIONALPROPER,
            )
        } else {
            (
                diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE,
                diagnostic_messages::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE,
            )
        }
    }

    fn should_suppress_promise_then_nullable_callback_arg_mismatch(
        &mut self,
        arg_type: TypeId,
        param_type: TypeId,
        idx: NodeIndex,
    ) -> bool {
        if !self.is_callback_like_argument(idx)
            || !self.type_is_nullish_only(param_type)
            || matches!(arg_type, TypeId::ERROR | TypeId::ANY)
        {
            return false;
        }

        let Some(call_idx) = self.parent_call_containing_argument(idx) else {
            return false;
        };
        let Some(call_node) = self.ctx.arena.get(call_idx) else {
            return false;
        };
        let Some(call) = self.ctx.arena.get_call_expr(call_node) else {
            return false;
        };
        let callee_idx = self.ctx.arena.skip_parenthesized(call.expression);
        let Some(callee_node) = self.ctx.arena.get(callee_idx) else {
            return false;
        };
        if callee_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }
        let Some(access) = self.ctx.arena.get_access_expr(callee_node) else {
            return false;
        };
        let Some(name_node) = self.ctx.arena.get(access.name_or_argument) else {
            return false;
        };
        let Some(name) = self.ctx.arena.get_identifier(name_node) else {
            return false;
        };
        if name.escaped_text != "then" {
            return false;
        }

        let receiver_type = self.get_type_of_node(access.expression);
        let evaluated_receiver = self.evaluate_type_with_env(receiver_type);
        self.type_ref_is_promise_like(receiver_type)
            || self.type_ref_is_promise_like(evaluated_receiver)
    }

    fn type_is_nullish_only(&self, type_id: TypeId) -> bool {
        match type_id {
            TypeId::NULL | TypeId::UNDEFINED => true,
            _ => crate::query_boundaries::common::union_members(self.ctx.types, type_id)
                .is_some_and(|members| {
                    !members.is_empty()
                        && members
                            .iter()
                            .all(|&member| matches!(member, TypeId::NULL | TypeId::UNDEFINED))
                }),
        }
    }

    fn parent_call_containing_argument(&self, idx: NodeIndex) -> Option<NodeIndex> {
        let mut current = idx;
        for _ in 0..100 {
            let ext = self.ctx.arena.get_extended(current)?;
            if ext.parent.is_none() {
                return None;
            }
            let parent_idx = ext.parent;
            let parent = self.ctx.arena.get(parent_idx)?;
            match parent.kind {
                k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION
                    || k == syntax_kind_ext::NON_NULL_EXPRESSION
                    || k == syntax_kind_ext::TYPE_ASSERTION
                    || k == syntax_kind_ext::AS_EXPRESSION
                    || k == syntax_kind_ext::SATISFIES_EXPRESSION =>
                {
                    current = parent_idx;
                }
                k if k == syntax_kind_ext::CALL_EXPRESSION
                    || k == syntax_kind_ext::NEW_EXPRESSION =>
                {
                    return self
                        .ctx
                        .arena
                        .get_call_expr(parent)
                        .and_then(|call| call.arguments.as_ref())
                        .is_some_and(|args| args.nodes.contains(&current))
                        .then_some(parent_idx);
                }
                _ => return None,
            }
        }
        None
    }

    fn should_suppress_constraint_cascade_constructor_argument(
        &self,
        arg_type: TypeId,
        param_type: TypeId,
    ) -> bool {
        if !self
            .ctx
            .diagnostics
            .iter()
            .any(|diag| diag.code == diagnostic_codes::TYPE_DOES_NOT_SATISFY_THE_CONSTRAINT)
        {
            return false;
        }
        if !crate::query_boundaries::common::is_constructor_like_type(self.ctx.types, arg_type) {
            return false;
        }
        if crate::query_boundaries::common::is_constructor_like_type(self.ctx.types, param_type)
            || crate::query_boundaries::common::is_callable_type(self.ctx.types, param_type)
        {
            return true;
        }
        crate::query_boundaries::common::union_members(self.ctx.types, param_type).is_some_and(
            |members| {
                members.iter().all(|&member| {
                    crate::query_boundaries::common::is_constructor_like_type(
                        self.ctx.types,
                        member,
                    ) || crate::query_boundaries::common::is_callable_type(self.ctx.types, member)
                })
            },
        )
    }

    fn trim_single_unbalanced_trailing_type_arg_close(display: String) -> String {
        let Some(candidate) = display.strip_suffix('>') else {
            return display;
        };

        let mut opens = 0usize;
        let mut closes = 0usize;
        let mut prev = '\0';
        let mut in_single = false;
        let mut in_double = false;
        let mut escaped = false;

        for ch in display.chars() {
            if escaped {
                escaped = false;
                prev = ch;
                continue;
            }
            if in_single || in_double {
                if ch == '\\' {
                    escaped = true;
                } else if in_single && ch == '\'' {
                    in_single = false;
                } else if in_double && ch == '"' {
                    in_double = false;
                }
                prev = ch;
                continue;
            }
            match ch {
                '\'' => in_single = true,
                '"' => in_double = true,
                '<' => opens += 1,
                '>' if prev != '=' => closes += 1,
                _ => {}
            }
            prev = ch;
        }

        if closes == opens.saturating_add(1) {
            candidate.to_string()
        } else {
            display
        }
    }

    fn widen_literal_call_argument_display_against_plain_primitive_parameter(
        &mut self,
        arg_type: TypeId,
        arg_idx: NodeIndex,
        param_display: &str,
    ) -> Option<String> {
        let param_base = match param_display {
            "string" => TypeId::STRING,
            "number" => TypeId::NUMBER,
            "boolean" => TypeId::BOOLEAN,
            "bigint" => TypeId::BIGINT,
            "symbol" => TypeId::SYMBOL,
            _ => return None,
        };
        let source = self.expression_display_type_preferring_literal(arg_idx, arg_type);
        let source_base =
            crate::query_boundaries::common::widen_literal_to_primitive(self.ctx.types, source);
        if source_base == source || source_base == param_base {
            return None;
        }
        Some(self.format_type_for_assignability_message(source_base))
    }

    pub(in crate::error_reporter::call_errors) fn mapped_property_mismatch_parameter_display(
        &mut self,
        param_display: &str,
        failure_reason: Option<&tsz_solver::SubtypeFailureReason>,
    ) -> Option<String> {
        if !param_display.trim_start().starts_with("{ [") {
            return None;
        }
        let tsz_solver::SubtypeFailureReason::PropertyTypeMismatch {
            property_name,
            target_property_type,
            ..
        } = failure_reason?
        else {
            return None;
        };

        let display_type =
            crate::query_boundaries::diagnostics::mapped_property_mismatch_parameter_display_type(
                self.ctx.types,
                *property_name,
                *target_property_type,
            );
        Some(self.format_type_for_assignability_message(display_type))
    }

    /// Report an argument count mismatch error using solver diagnostics with source tracking.
    /// TS2554: Expected {0} arguments, but got {1}.
    ///
    /// When there are excess arguments (`got > expected_max`), tsc points the
    /// diagnostic span at the excess arguments rather than the call expression.
    /// The `args` slice provides the argument node indices so we can compute
    /// the span from the first excess argument to the last argument.
    pub fn error_argument_count_mismatch_at(
        &mut self,
        expected_min: usize,
        expected_max: usize,
        got: usize,
        idx: NodeIndex,
        args: &[NodeIndex],
    ) {
        // When there are excess arguments, point to them instead of the callee.
        let (start, length) = if let Some((s, l)) =
            self.resolve_excess_argument_span(args, expected_max)
        {
            (s, l)
        } else if self.is_new_expression(idx) {
            // For `new X()` with too few arguments, TSC uses the full
            // `new X(...)` span (starting from the `new` keyword).
            if let Some(anchor) = self.resolve_diagnostic_anchor(idx, DiagnosticAnchorKind::Exact) {
                (anchor.start, anchor.length)
            } else {
                return;
            }
        } else if let Some(anchor) =
            self.resolve_diagnostic_anchor(idx, DiagnosticAnchorKind::CallPrimary)
        {
            (anchor.start, anchor.length)
        } else {
            return;
        };

        let mut builder = tsz_solver::SpannedDiagnosticBuilder::with_symbols(
            self.ctx.types,
            &self.ctx.binder.symbols,
            self.ctx.file_name.as_str(),
        )
        .with_def_store(&self.ctx.definition_store);
        let diag = builder.argument_count_mismatch(expected_min, expected_max, got, start, length);
        self.ctx
            .diagnostics
            .push(diag.to_checker_diagnostic(&self.ctx.file_name));
    }

    /// TS2560 ("did you mean to call it?") in call-site weak-type comparisons
    /// widens *genuinely fresh* callable-source members for display
    /// (`() => { timeout: 1000 }` renders `() => { timeout: number }`) while
    /// leaving declared literal annotations literal (`… & { a: 1 }`). A fresh
    /// object literal keeps its `1000` spelling only in display provenance over
    /// an already-widened canonical shape, which the solver reports as
    /// `display_residue`; a declared literal is canonical and produces none.
    /// Mirrors the shared assignment/`satisfies` renderer so both diagnostic
    /// sites use one fresh-versus-declared policy (#13075).
    fn widen_weak_type_callable_source_display(&self, arg_type: TypeId, arg_str: String) -> String {
        let widened = self.widen_annotation_literals_for_display(
            self.widen_literal_type(arg_type),
            crate::query_boundaries::diagnostics::AnnotationLiteralWideningPolicy::ALL,
        );
        if widened.display_residue {
            // Fresh literal spellings live only in display provenance; render
            // the canonical (display-property-free) widened form.
            return self.format_type_diagnostic_widened(widened.type_id);
        }
        // No display residue: declared / `non_widening` literal annotations are
        // canonical and authoritative, so keep the original rendered source.
        arg_str
    }

    /// Check if a node is a `new` expression.
    fn is_new_expression(&self, idx: NodeIndex) -> bool {
        self.ctx
            .arena
            .get(idx)
            .is_some_and(|n| n.kind == syntax_kind_ext::NEW_EXPRESSION)
    }

    /// Report a spread argument type error (TS2556).
    /// TS2556: A spread argument must either have a tuple type or be passed to a rest parameter.
    pub fn error_spread_must_be_tuple_or_rest_at(&mut self, idx: NodeIndex) {
        self.error_at_node(
            idx,
            diagnostic_messages::A_SPREAD_ARGUMENT_MUST_EITHER_HAVE_A_TUPLE_TYPE_OR_BE_PASSED_TO_A_REST_PARAMETER,
            diagnostic_codes::A_SPREAD_ARGUMENT_MUST_EITHER_HAVE_A_TUPLE_TYPE_OR_BE_PASSED_TO_A_REST_PARAMETER,
        );
    }

    /// Report an "expected at least N arguments" error (TS2555).
    /// TS2555: Expected at least {0} arguments, but got {1}.
    pub fn error_expected_at_least_arguments_at(
        &mut self,
        expected_min: usize,
        got: usize,
        idx: NodeIndex,
    ) {
        let message = format!("Expected at least {expected_min} arguments, but got {got}.");
        // For `new` expressions, TSC uses the full `new X(...)` span.
        let anchor_kind = if self.is_new_expression(idx) {
            DiagnosticAnchorKind::Exact
        } else {
            DiagnosticAnchorKind::CallPrimary
        };
        self.error_at_anchor(
            idx,
            anchor_kind,
            &message,
            diagnostic_codes::EXPECTED_AT_LEAST_ARGUMENTS_BUT_GOT,
        );
    }

    /// Report "No overload matches this call" with related overload failures.
    ///
    /// Contract: `failures` carries exactly one entry per failed overload
    /// candidate, in declaration order — the `Overload {i} of {N}` total
    /// renders its length.
    pub fn error_no_overload_matches_at(
        &mut self,
        idx: NodeIndex,
        failures: &[tsz_solver::PendingDiagnostic],
    ) {
        tracing::debug!(
            "error_no_overload_matches_at: File name: {}",
            self.ctx.file_name
        );

        if self.should_suppress_concat_overload_error(idx) {
            return;
        }

        let argument_failures: Vec<_> = failures
            .iter()
            .filter(|failure| {
                failure.code
                    == diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE
            })
            .collect();
        let literal_anchor = self.overload_literal_argument_anchor(idx, failures);
        let shared_argument_anchor = self
            .shared_overload_argument_anchor_from_spans(idx, &argument_failures)
            .or_else(|| self.shared_overload_argument_anchor(idx, &argument_failures));
        let identical_argument_failures = {
            let mut formatter = self.ctx.create_type_formatter();
            argument_failures
                .first()
                .map(|first| {
                    let rendered_first = formatter.render(first);
                    argument_failures
                        .iter()
                        .skip(1)
                        .all(|failure| formatter.render(failure).message == rendered_first.message)
                })
                .unwrap_or(false)
        };
        let remaining_failures: Vec<_> = failures
            .iter()
            .filter(|failure| {
                failure.code
                    != diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE
            })
            .collect();
        let callback_body_failure_span = if !argument_failures.is_empty() {
            let callback_spans: Vec<(u32, u32)> = self
                .logical_call_argument_nodes(idx)
                .unwrap_or_default()
                .into_iter()
                .filter(|&arg_idx| self.is_callback_like_argument(arg_idx))
                .flat_map(|arg_idx| self.callback_body_spans(arg_idx))
                .collect();
            let mut shared = None;
            let mut all_callback_body_spans = !callback_spans.is_empty();
            for failure in &argument_failures {
                let Some(span) = failure.span.as_ref() else {
                    all_callback_body_spans = false;
                    break;
                };
                if !callback_spans
                    .iter()
                    .any(|(start, end)| span.start >= *start && span.start < *end)
                {
                    all_callback_body_spans = false;
                    break;
                }
                if let Some((start, length)) = shared {
                    if start != span.start || length != span.length {
                        all_callback_body_spans = false;
                        break;
                    }
                } else {
                    shared = Some((span.start, span.length));
                }
            }
            all_callback_body_spans
                .then_some(shared)
                .flatten()
                .map(|(start, length)| ResolvedDiagnosticAnchor {
                    node_idx: idx,
                    start,
                    length,
                })
        } else {
            None
        };
        let remaining_failures_are_count_mismatches = remaining_failures
            .iter()
            .all(|failure| failure.is_arity_failure());
        let all_failures_are_argument_mismatches = !failures.is_empty()
            && failures.iter().all(|failure| {
                failure.code
                    == diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE
            });
        let anchor_argument_from_first_argument_mismatch = all_failures_are_argument_mismatches
            && shared_argument_anchor.is_none()
            && !(self.overload_callee_is_property_like(idx)
                && self
                    .logical_call_argument_nodes(idx)
                    .is_some_and(|args| args.len() > 1))
            && self.first_argument_mismatches_all_overload_expected_types(idx, &argument_failures);
        let anchor_argument_from_mixed_failures = shared_argument_anchor.is_some()
            && !remaining_failures.is_empty()
            && remaining_failures_are_count_mismatches;
        // When all overload failures share the same argument anchor but the
        // failure messages disagree *and* the argument is an object literal,
        // tsc treats the overload set — not the argument — as the culprit and
        // anchors the top-level TS2769 at the callee. This covers cases like
        // `v({s:"", n:0})` against `(x:{s:string}) | (x:{n:number})`, where
        // each overload rejects a different excess property on the same
        // literal. For non-object-literal arguments (e.g., `fn(true)` vs
        // `(x:string)|(x:number)`), tsc still anchors at the argument.
        let is_tagged_template_call = self
            .ctx
            .arena
            .get(idx)
            .is_some_and(|node| node.kind == syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION);
        let shared_argument_is_object_literal = shared_argument_anchor.is_some_and(|anchor_idx| {
            self.ctx
                .arena
                .get(anchor_idx)
                .is_some_and(|node| node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION)
        });
        // For object-literal overload failures, tsc's anchor depends on the
        // actual excess-property culprit. If every overload rejects the same
        // property (`fn({ z: 3, a: 3 })` against `{x}`/`{y}`), anchor at that
        // property. If overloads reject different properties (`v({s,n})`
        // against `{s}`/`{n}`), anchor at the callee because no single property
        // explains the whole overload failure.
        let shared_excess_property_name = if shared_argument_is_object_literal
            && !argument_failures.is_empty()
        {
            let mut first_name = None;
            let mut all_same = true;
            for failure in &argument_failures {
                let Some((arg_type, param_type)) = failure.type_pair() else {
                    all_same = false;
                    break;
                };
                let analysis = self.analyze_assignability_failure(arg_type, param_type);
                let Some(tsz_solver::SubtypeFailureReason::ExcessProperty {
                    property_name, ..
                }) = analysis.failure_reason
                else {
                    all_same = false;
                    break;
                };
                match &first_name {
                    Some(first_name) if first_name != &property_name => {
                        all_same = false;
                        break;
                    }
                    Some(_) => {}
                    None => first_name = Some(property_name),
                }
            }
            all_same && first_name.is_some()
        } else {
            false
        };
        let anchor_argument_from_all_failures = all_failures_are_argument_mismatches
            && shared_argument_anchor.is_some()
            && (!shared_argument_is_object_literal
                || is_tagged_template_call
                || identical_argument_failures
                || shared_excess_property_name);
        let raw_argument_anchor =
            shared_argument_anchor.or_else(|| self.first_call_argument_anchor(idx));
        let argument_anchor_is_callback = raw_argument_anchor
            .is_some_and(|anchor_idx| self.is_callback_expression_argument(anchor_idx));
        let callback_overloads_are_callable_only = argument_failures.iter().all(|failure| {
            failure.type_pair().is_some_and(|(_, param_ty)| {
                crate::query_boundaries::common::function_shape_for_type(self.ctx.types, param_ty)
                    .is_some()
                    || crate::query_boundaries::common::callable_shape_for_type(
                        self.ctx.types,
                        param_ty,
                    )
                    .is_some()
            })
        });
        let callback_argument_has_prior_diagnostics =
            raw_argument_anchor.is_some_and(|anchor_idx| {
                self.ctx.arena.get(anchor_idx).is_some_and(|arg_node| {
                    self.ctx.diagnostics.iter().any(|diag| {
                        diag.code != diagnostic_codes::NO_OVERLOAD_MATCHES_THIS_CALL
                            && diag.start >= arg_node.pos
                            && diag.start < arg_node.end
                    })
                })
            });
        let single_callback_argument = self
            .ctx
            .arena
            .get(idx)
            .and_then(|call_node| self.ctx.arena.get_call_expr(call_node))
            .and_then(|call_expr| call_expr.arguments.as_ref())
            .is_some_and(|args| args.nodes.len() == 1);
        let is_new_call = self.is_new_expression(idx);
        let allow_callback_argument_anchor = argument_anchor_is_callback
            && single_callback_argument
            && all_failures_are_argument_mismatches
            && callback_overloads_are_callable_only
            && !callback_argument_has_prior_diagnostics;
        let allow_new_argument_anchor = is_new_call
            && anchor_argument_from_all_failures
            && !self.is_weak_collection_constructor_new(idx);
        let anchor_first_argument = (!is_new_call || allow_new_argument_anchor)
            && (!argument_anchor_is_callback || allow_callback_argument_anchor)
            && (identical_argument_failures
                && !remaining_failures.is_empty()
                && remaining_failures_are_count_mismatches
                || anchor_argument_from_mixed_failures
                || anchor_argument_from_all_failures
                || anchor_argument_from_first_argument_mismatch);
        let tagged_generic_overload_anchor = if is_tagged_template_call
            && self.tagged_template_callee_has_generic_call_signature(idx)
        {
            self.tagged_template_generic_overload_anchor(idx)
        } else {
            None
        };
        let anchor_kind = if let Some(anchor_idx) = tagged_generic_overload_anchor {
            if anchor_idx == idx {
                DiagnosticAnchorKind::OverloadPrimary
            } else {
                DiagnosticAnchorKind::Exact
            }
        } else if literal_anchor.is_some() {
            DiagnosticAnchorKind::Exact
        } else if anchor_first_argument {
            shared_argument_anchor
                .or_else(|| self.first_call_argument_anchor(idx))
                .map(|_| DiagnosticAnchorKind::Exact)
                .unwrap_or(DiagnosticAnchorKind::OverloadPrimary)
        } else {
            DiagnosticAnchorKind::OverloadPrimary
        };
        let anchor_idx = if let Some(anchor_idx) = tagged_generic_overload_anchor {
            anchor_idx
        } else if let Some(anchor_idx) = literal_anchor {
            anchor_idx
        } else if anchor_first_argument {
            let raw_anchor = raw_argument_anchor.unwrap_or(idx);
            // When the anchor is an object literal expression, tsc drills down
            // to the first property so the TS2769 diagnostic points at the
            // first property name (e.g. `z` in `{ z: 3 }`) rather than `{`.
            self.first_object_literal_property(raw_anchor)
                .unwrap_or(raw_anchor)
        } else {
            idx
        };
        // Implicit method-`this` provenance outranks literal/tagged argument
        // heuristics because `tsc` checks the receiver before explicit
        // arguments. Other indexed failures refine only the ordinary
        // `OverloadPrimary` fallback.
        let last_signed_failure_is_this = failures
            .iter()
            .rev()
            .find(|failure| !failure.is_arity_failure() && failure.overload_signature.is_some())
            .is_some_and(|failure| {
                failure.code
                    == diagnostic_codes::THE_THIS_CONTEXT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_METHODS_THIS_OF_TYPE
            });
        let indexed_argument_anchor = if last_signed_failure_is_this {
            self.this_type_mismatch_anchor(idx)
        } else if matches!(anchor_kind, DiagnosticAnchorKind::OverloadPrimary) {
            self.last_overload_failure_anchor(idx, failures)
        } else {
            None
        };
        let provenance_anchor = if last_signed_failure_is_this {
            indexed_argument_anchor.or(callback_body_failure_span)
        } else {
            callback_body_failure_span.or(indexed_argument_anchor)
        };
        let Some(anchor) =
            provenance_anchor.or_else(|| self.resolve_diagnostic_anchor(anchor_idx, anchor_kind))
        else {
            return;
        };
        let span =
            tsz_solver::SourceSpan::new(self.ctx.file_name.as_str(), anchor.start, anchor.length);

        tracing::debug!("File name: {}", self.ctx.file_name);

        // tsc's `resolveCall` partitions the failed candidates: overloads that
        // matched arity but failed argument checks (`candidatesForArgumentError`)
        // are elaborated, while arity failures never appear in the chain yet
        // still count toward the `{N}` of `Overload {i} of {N}`. Two or three
        // argument-error candidates each get a TS2772 header in declaration
        // order (`{i}` is the candidate's 1-based position among the
        // argument-error candidates); four or more collapse to a single
        // `The last overload gave the following error.` (TS2770) header
        // wrapping only the last candidate. A lone argument-error candidate
        // collapses to a plain TS2345 upstream (no TS2769 at all). Candidate
        // sets whose failures carry no declared signature (e.g. callback-body
        // sets) keep the historical flat rendering.
        let chain_candidates: Vec<&tsz_solver::PendingDiagnostic> = failures
            .iter()
            .filter(|failure| !failure.is_arity_failure())
            .collect();
        // Signature presence is the provenance proxy for "one failure per
        // candidate from real overload resolution" (callback-body sets carry
        // none and stay flat), so it gates both wrapped shapes.
        let every_candidate_signed = chain_candidates
            .iter()
            .all(|failure| failure.overload_signature.is_some());
        enum ChainShape {
            /// Fewer than 2 argument-error candidates, or a candidate without
            /// a declared signature: historical flat rendering.
            Flat,
            /// 2+ argument-error candidates: one `TS2770` header wrapping only
            /// the last candidate. tsc 7.0.2 (the native tsgo port) renders
            /// EVERY multi-candidate overload failure this way — the 6.0.x
            /// per-candidate `Overload {i} of {N}` (TS2772) elaboration for
            /// 2-3 candidates is unreachable in the pinned compiler (verified
            /// against the pinned binary across 2/3/4/5-overload calls,
            /// constructors, duplicates, generics, and union-combined sets).
            LastOverload,
        }
        let shape = if !every_candidate_signed {
            ChainShape::Flat
        } else {
            match chain_candidates.len() {
                0 | 1 => ChainShape::Flat,
                _ => ChainShape::LastOverload,
            }
        };
        let wrapped_candidates: &[&tsz_solver::PendingDiagnostic] = match shape {
            ChainShape::Flat => &[],
            ChainShape::LastOverload => &chain_candidates[chain_candidates.len() - 1..],
        };

        // Per-candidate relation reason chains (#15387), computed before the
        // type formatter takes its borrow of the checker context (failure
        // analysis needs `&mut self`). The chain anchors at the candidate's
        // failing *argument* node — exactly like the single-signature TS2345
        // path — never at the call node: the renderer's display probes climb
        // from their anchor to enclosing initializers and would type the
        // surrounding expression mid-flight.
        // Each candidate's reason chain plus whether its applicability failure
        // HEAD-PROMOTES: exactly as on the single-signature path, a failure
        // whose top elaboration is a property-level diagnostic
        // (TS2741/TS2739/TS2740, the `reportUnmatchedProperty` family) renders
        // that diagnostic directly under the overload header with no
        // `Argument of type ... is not assignable` wrapper:
        //   The last overload gave the following error.
        //     Property 'beta' is missing in type ... .
        // Decided here, before the type formatter takes its context borrow
        // (the promotion predicate needs `&mut self`).
        let candidate_chains: Vec<(Vec<DiagnosticRelatedInformation>, bool)> = wrapped_candidates
            .iter()
            .map(|failure| {
                let chain = self
                    .overload_failure_reason_anchor_node(idx, failure)
                    .map(|arg_idx| self.overload_candidate_reason_chain(failure, arg_idx, &span))
                    .unwrap_or_default();
                let source_target = matches!(
                    failure.code,
                    diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE
                        | diagnostic_codes::THE_THIS_CONTEXT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_METHODS_THIS_OF_TYPE
                )
                    .then(|| match (failure.args.first(), failure.args.get(1)) {
                        (
                            Some(&tsz_solver::DiagnosticArg::Type(source)),
                            Some(&tsz_solver::DiagnosticArg::Type(target)),
                        ) => Some((source, target)),
                        _ => None,
                    })
                    .flatten();
                let promoted = chain
                    .first()
                    .is_some_and(|line| matches!(line.code, 2739..=2741))
                    && source_target.is_some_and(|(source, target)| {
                        self.missing_property_head_promotion_applies(source, target)
                    });
                (chain, promoted)
            })
            .collect();

        let mut related = Vec::new();
        let mut formatter = self.ctx.create_type_formatter();

        // One elaboration line at `span_of` with the given message/code/depth.
        let related_line =
            |span_of: &tsz_solver::SourceSpan, message_text: String, code: u32, depth: u8| {
                DiagnosticRelatedInformation {
                    file: span_of.file.to_string(),
                    start: span_of.start,
                    length: span_of.length,
                    message_text,
                    category: DiagnosticCategory::Message,
                    code,
                    depth,
                    kind: RelatedInformationKind::ChainLink,
                }
            };

        let related_policy = if matches!(shape, ChainShape::Flat) {
            for failure in failures {
                let pending = self.overload_failure_generalized_pending(failure, &span);
                let diag = formatter.render(&pending);
                if let Some(diag_span) = diag.span.as_ref() {
                    related.push(related_line(diag_span, diag.message, diag.code, 0));
                }
            }
            RelatedInformationPolicy::OVERLOAD_FAILURES
        } else {
            for (ordinal, (failure, (chain, promoted))) in
                wrapped_candidates.iter().zip(candidate_chains).enumerate()
            {
                let (header_message, header_code) = if matches!(shape, ChainShape::LastOverload) {
                    (
                        diagnostic_messages::THE_LAST_OVERLOAD_GAVE_THE_FOLLOWING_ERROR.to_string(),
                        diagnostic_codes::THE_LAST_OVERLOAD_GAVE_THE_FOLLOWING_ERROR,
                    )
                } else {
                    let signature = failure
                        .overload_signature
                        .expect("wrapped shapes require every candidate to carry a signature");
                    // signatureToString colon form (`(x: number): number`); fall
                    // back to the plain render only if the type is not a signature.
                    let signature_display = formatter
                        .format_overload_signature(signature)
                        .unwrap_or_else(|| formatter.format(signature).into_owned());
                    (
                        format_message(
                            diagnostic_messages::OVERLOAD_OF_GAVE_THE_FOLLOWING_ERROR,
                            &[
                                &(ordinal + 1).to_string(),
                                // `{N}` counts every failed overload, arity
                                // failures included.
                                &failures.len().to_string(),
                                &signature_display,
                            ],
                        ),
                        diagnostic_codes::OVERLOAD_OF_GAVE_THE_FOLLOWING_ERROR,
                    )
                };
                related.push(related_line(&span, header_message, header_code, 0));
                if promoted {
                    for (position, mut line) in chain.into_iter().enumerate() {
                        line.depth = if position == 0 {
                            1
                        } else {
                            line.depth.saturating_add(1)
                        };
                        related.push(line);
                    }
                } else {
                    let pending = self.overload_failure_generalized_pending(failure, &span);
                    let diag = formatter.render(&pending);
                    let diag_span = diag.span.as_ref().unwrap_or(&span);
                    // The candidate's applicability error nests one level under
                    // its header; any deeper chain it carries nests below that.
                    related.push(related_line(diag_span, diag.message, diag.code, 1));
                    for nested in &diag.related {
                        related.push(related_line(
                            &nested.span,
                            nested.message.clone(),
                            0,
                            nested.depth.saturating_add(2),
                        ));
                    }
                    // The reason chain nests under the applicability error.
                    for mut line in chain {
                        line.depth = line.depth.saturating_add(2);
                        related.push(line);
                    }
                }
            }
            RelatedInformationPolicy::OVERLOAD_CHAINS
        };

        self.emit_render_request_at_anchor(
            anchor,
            DiagnosticRenderRequest::with_related(
                anchor_kind,
                diagnostic_codes::NO_OVERLOAD_MATCHES_THIS_CALL,
                diagnostic_messages::NO_OVERLOAD_MATCHES_THIS_CALL.to_string(),
                related,
                related_policy,
            ),
        );
    }

    /// Build the relation failure-reason elaboration chain for one overload
    /// candidate's argument mismatch (#15387).
    ///
    /// tsc nests the same reason chain under each candidate's `TS2772` header
    /// that the single-signature path renders under a plain `TS2345`
    /// (`getSignatureApplicabilityError` reuses
    /// `checkTypeRelatedToAndOptionallyElaborate`). Reuse the shared
    /// relation → reason → diagnostic gateway (`analyze_assignability_failure`,
    /// whose captured pass is memoized) and the single elaboration owner
    /// (`related_from_failure_reason`), then re-anchor every line onto the
    /// shared `TS2769` span: chain lines are message-chain text, not
    /// cross-location pointers. Candidates whose pending diagnostic already
    /// carries a related payload keep it (the caller renders it). Argument and
    /// `this` type failures share the relation-reason path; arity failures do
    /// not carry a structural chain.
    /// The AST node one overload candidate's relation failure describes.
    ///
    /// An implicit method-`this` failure belongs to the receiver regardless of
    /// explicit argument count. Argument failures use their source span, with
    /// the sole-argument fallback retained for solver-resolved candidates.
    fn overload_failure_reason_anchor_node(
        &self,
        idx: NodeIndex,
        failure: &tsz_solver::PendingDiagnostic,
    ) -> Option<NodeIndex> {
        if failure.code
            == diagnostic_codes::THE_THIS_CONTEXT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_METHODS_THIS_OF_TYPE
        {
            return self.this_type_mismatch_anchor_node(idx);
        }
        let arg_nodes = self.logical_call_argument_nodes(idx)?;
        let Some(span) = failure.span.as_ref() else {
            return match arg_nodes.as_slice() {
                [only] => Some(*only),
                _ => None,
            };
        };
        arg_nodes.into_iter().find(|&arg_idx| {
            self.get_source_location(arg_idx)
                .is_some_and(|loc| span.start >= loc.start && span.start < loc.end)
        })
    }

    fn overload_candidate_reason_chain(
        &mut self,
        failure: &tsz_solver::PendingDiagnostic,
        anchor_idx: NodeIndex,
        span: &tsz_solver::SourceSpan,
    ) -> Vec<DiagnosticRelatedInformation> {
        if !failure.related.is_empty()
            || !matches!(
                failure.code,
                diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE
                    | diagnostic_codes::THE_THIS_CONTEXT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_METHODS_THIS_OF_TYPE
            )
        {
            return Vec::new();
        }
        let Some((source, target)) = failure.type_pair() else {
            return Vec::new();
        };
        let Some(reason) = self
            .analyze_assignability_failure(source, target)
            .failure_reason
        else {
            return Vec::new();
        };
        let chain = self
            .related_from_failure_reason(&reason, source, target, anchor_idx)
            .unwrap_or_default();
        Self::reanchor_chain_lines(chain, span.start, span.length)
    }

    /// Build the per-overload failure diagnostic anchored at the shared
    /// `TS2769` span, applying tsc's `reportRelationError` source
    /// generalization: a fresh literal source (`true`, `1`) is widened to its
    /// base (`boolean`, `number`) unless the parameter target could hold a
    /// top-level singleton type. The pre-built solver diagnostic carries the
    /// raw literal source, so without this the overload elaboration diverges
    /// from tsc (and from the single-overload TS2345 display).
    fn overload_failure_generalized_pending(
        &self,
        failure: &tsz_solver::PendingDiagnostic,
        span: &tsz_solver::SourceSpan,
    ) -> tsz_solver::PendingDiagnostic {
        let mut pending = tsz_solver::PendingDiagnostic {
            span: Some(span.clone()),
            ..failure.clone()
        };
        if pending.code == diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE
            && let Some((source, target)) = pending.type_pair()
        {
            let display_source =
                crate::query_boundaries::diagnostics::generalized_literal_source_for_display(
                    self.ctx.types,
                    source,
                    target,
                );
            if display_source != source {
                pending.args[0] = tsz_solver::DiagnosticArg::Type(display_source);
            }
        }
        pending
    }

    fn tagged_template_callee_has_generic_call_signature(&mut self, idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };
        let Some(tagged) = self.ctx.arena.get_tagged_template(node).cloned() else {
            return false;
        };
        let tag_type = self.get_type_of_node(tagged.tag);
        let tag_type = self.resolve_ref_type(tag_type);
        let tag_type = self.resolve_lazy_type(tag_type);
        crate::query_boundaries::common::callable_shape_for_type(self.ctx.types, tag_type)
            .is_some_and(|shape| {
                shape
                    .call_signatures
                    .iter()
                    .any(|sig| !sig.type_params.is_empty())
            })
    }

    fn is_weak_collection_constructor_new(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::NEW_EXPRESSION {
            return false;
        }
        self.ctx
            .arena
            .get_call_expr(node)
            .and_then(|call| self.ctx.arena.get_identifier_text(call.expression))
            .is_some_and(|name| matches!(name, "WeakMap" | "WeakSet"))
    }

    fn tagged_template_generic_overload_anchor(&self, idx: NodeIndex) -> Option<NodeIndex> {
        let args = self.logical_call_argument_nodes(idx)?;
        let first_substitution = args.get(1).copied()?;
        let first_node = self.ctx.arena.get(first_substitution)?;
        if first_node.kind == tsz_scanner::SyntaxKind::NullKeyword as u16
            || first_node.kind == tsz_scanner::SyntaxKind::TrueKeyword as u16
            || first_node.kind == tsz_scanner::SyntaxKind::FalseKeyword as u16
        {
            Some(first_substitution)
        } else {
            Some(idx)
        }
    }

    /// Report TS2693: type parameter used as value
    pub fn error_type_parameter_used_as_value(&mut self, name: &str, idx: NodeIndex) {
        use tsz_common::diagnostics::diagnostic_codes;

        let message = format!("'{name}' only refers to a type, but is being used as a value here.");

        self.error_at_node(
            idx,
            &message,
            diagnostic_codes::ONLY_REFERS_TO_A_TYPE_BUT_IS_BEING_USED_AS_A_VALUE_HERE,
        );
    }

    /// Report a "this type mismatch" error using solver diagnostics with source tracking.
    pub fn error_this_type_mismatch_at(
        &mut self,
        expected_this: TypeId,
        actual_this: TypeId,
        idx: NodeIndex,
    ) {
        let Some(anchor) = self.resolve_diagnostic_anchor(idx, DiagnosticAnchorKind::Exact) else {
            return;
        };
        self.error_this_type_mismatch_at_anchor(expected_this, actual_this, anchor);
    }

    fn error_this_type_mismatch_at_anchor(
        &mut self,
        expected_this: TypeId,
        actual_this: TypeId,
        anchor: ResolvedDiagnosticAnchor,
    ) {
        let failure_reason = self
            .analyze_assignability_failure(actual_this, expected_this)
            .failure_reason;
        if let Some(reason) = failure_reason.as_ref()
            && matches!(
                reason,
                tsz_solver::SubtypeFailureReason::MissingProperty { .. }
                    | tsz_solver::SubtypeFailureReason::MissingProperties { .. }
            )
            && self.missing_property_head_promotion_applies(actual_this, expected_this)
        {
            let mut diag =
                self.render_failure_reason(reason, actual_this, expected_this, anchor.node_idx, 0);
            if matches!(
                diag.code,
                diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE
                    | diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE
                    | diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE_AND_MORE
            ) {
                diag.start = anchor.start;
                diag.length = anchor.length;
                self.ctx.push_diagnostic(diag);
                return;
            }
        }

        let mut builder = tsz_solver::SpannedDiagnosticBuilder::with_symbols(
            self.ctx.types,
            &self.ctx.binder.symbols,
            self.ctx.file_name.as_str(),
        )
        .with_def_store(&self.ctx.definition_store);
        let diag =
            builder.this_type_mismatch(expected_this, actual_this, anchor.start, anchor.length);
        if let Some(reason) = failure_reason {
            self.emit_render_request_at_anchor(
                anchor,
                DiagnosticRenderRequest::with_failure_reason(
                    DiagnosticAnchorKind::Exact,
                    diag.code,
                    diag.message,
                    reason,
                    actual_this,
                    expected_this,
                ),
            );
        } else {
            self.ctx
                .push_diagnostic(diag.to_checker_diagnostic(&self.ctx.file_name));
        }
    }

    /// Report a direct call's implicit-method-`this` mismatch at the receiver.
    /// Overload resolution may collapse one applicability failure past
    /// arity-only candidates, but the diagnostic still belongs to the receiver
    /// rather than the member/callee token.
    pub(crate) fn error_call_this_type_mismatch_at(
        &mut self,
        expected_this: TypeId,
        actual_this: TypeId,
        call_idx: NodeIndex,
        callee_idx: NodeIndex,
    ) {
        let Some(anchor) = self
            .this_type_mismatch_anchor(call_idx)
            .or_else(|| self.resolve_diagnostic_anchor(callee_idx, DiagnosticAnchorKind::Exact))
        else {
            return;
        };
        self.error_this_type_mismatch_at_anchor(expected_this, actual_this, anchor);
    }

    /// Report a "type is not callable" error using solver diagnostics with source tracking.
    pub fn error_not_callable_at(&mut self, type_id: TypeId, idx: NodeIndex) {
        use tsz_parser::parser::syntax_kind_ext;

        // Suppress cascade errors from unresolved types.
        // In strictNullChecks mode, TS18046 is preferred for `unknown`;
        // in non-strict mode, `unknown` should emit a TS2349 callability error.
        if type_id == TypeId::ERROR
            || (type_id == TypeId::UNKNOWN && self.ctx.compiler_options.strict_null_checks)
        {
            return;
        }

        // For property access expressions (e.g., `obj.notMethod`), narrow the error
        // span to just the property name, matching tsc's behavior for chained calls.
        let report_idx = if let Some(node) = self.ctx.arena.get(idx)
            && node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && let Some(access) = self.ctx.arena.get_access_expr(node)
        {
            access.name_or_argument
        } else {
            idx
        };

        if let Some(loc) = self.get_source_location(report_idx) {
            let (start, length) = (loc.start, loc.length());
            let mut checker_diag = {
                let mut builder = tsz_solver::SpannedDiagnosticBuilder::with_symbols(
                    self.ctx.types,
                    &self.ctx.binder.symbols,
                    self.ctx.file_name.as_str(),
                )
                .with_def_store(&self.ctx.definition_store);
                builder
                    .not_callable(type_id, start, length)
                    .to_checker_diagnostic(&self.ctx.file_name)
            };
            // tsc appends `Type 'X' has no call signatures.` beneath the
            // `This expression is not callable.` headline (`invocationErrorDetails`).
            if let Some(detail) = self.invocation_signature_detail(
                type_id,
                crate::error_reporter::operator_errors::InvocationSignatureKind::Call,
                start,
                length,
            ) {
                checker_diag.related_information.push(detail);
            }
            self.ctx.diagnostics.push(checker_diag);
        }
    }

    /// Report TS6234: "This expression is not callable because it is a 'get' accessor.
    /// Did you mean to access it without '()'?"
    pub fn error_get_accessor_not_callable_at(&mut self, idx: NodeIndex) {
        use tsz_parser::parser::syntax_kind_ext;

        let report_idx = self
            .ctx
            .arena
            .get(idx)
            .and_then(|node| {
                if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                    self.ctx
                        .arena
                        .get_access_expr(node)
                        .map(|access| access.name_or_argument)
                } else {
                    None
                }
            })
            .unwrap_or(idx);

        self.error_at_node(
            report_idx,
            "This expression is not callable because it is a 'get' accessor. Did you mean to use it without '()'?",
            diagnostic_codes::THIS_EXPRESSION_IS_NOT_CALLABLE_BECAUSE_IT_IS_A_GET_ACCESSOR_DID_YOU_MEAN_TO_USE,
        );
    }

    /// Report TS2348: "Value of type '{0}' is not callable. Did you mean to include 'new'?"
    /// This is specifically for class constructors called without 'new'.
    pub fn error_class_constructor_without_new_at(&mut self, type_id: TypeId, idx: NodeIndex) {
        // Suppress cascade errors from unresolved types
        if type_id == TypeId::ERROR || type_id == TypeId::UNKNOWN {
            return;
        }

        let type_str = self.format_type_for_assignability_message(type_id);

        let message =
            diagnostic_messages::VALUE_OF_TYPE_IS_NOT_CALLABLE_DID_YOU_MEAN_TO_INCLUDE_NEW
                .replace("{0}", type_str.as_str());

        self.error_at_node(
            idx,
            &message,
            diagnostic_codes::VALUE_OF_TYPE_IS_NOT_CALLABLE_DID_YOU_MEAN_TO_INCLUDE_NEW,
        );
    }

    /// Report TS2350: "Only a void function can be called with the 'new' keyword."
    ///
    /// `new f()` where `f`'s apparent type has call signatures but no construct
    /// signatures resolves as a plain call returning `any`. tsc
    /// (`resolveNewExpression`) reports this only when the resolved signature's
    /// return type is not `void` **and `noImplicitAny` is off** — with
    /// `noImplicitAny` on, the implicit-`any` result is reported as TS7009
    /// instead, so the two are mutually exclusive and callers gate on that.
    pub fn error_non_void_function_called_with_new_at(&mut self, idx: NodeIndex) {
        self.error_at_node(
            idx,
            diagnostic_messages::ONLY_A_VOID_FUNCTION_CAN_BE_CALLED_WITH_THE_NEW_KEYWORD,
            diagnostic_codes::ONLY_A_VOID_FUNCTION_CAN_BE_CALLED_WITH_THE_NEW_KEYWORD,
        );
    }

    /// Report TS2721/TS2722/TS2723: "Cannot invoke an object which is possibly 'null'/'undefined'/'null or undefined'."
    /// Emitted when strictNullChecks is on and the callee type includes null/undefined.
    pub fn error_cannot_invoke_possibly_nullish_at(
        &mut self,
        nullish_cause: TypeId,
        idx: NodeIndex,
    ) {
        let (message, code) = if nullish_cause == TypeId::NULL {
            (
                diagnostic_messages::CANNOT_INVOKE_AN_OBJECT_WHICH_IS_POSSIBLY_NULL,
                diagnostic_codes::CANNOT_INVOKE_AN_OBJECT_WHICH_IS_POSSIBLY_NULL,
            )
        } else if nullish_cause == TypeId::UNDEFINED {
            (
                diagnostic_messages::CANNOT_INVOKE_AN_OBJECT_WHICH_IS_POSSIBLY_UNDEFINED,
                diagnostic_codes::CANNOT_INVOKE_AN_OBJECT_WHICH_IS_POSSIBLY_UNDEFINED,
            )
        } else {
            // Union of null and undefined (or void)
            (
                diagnostic_messages::CANNOT_INVOKE_AN_OBJECT_WHICH_IS_POSSIBLY_NULL_OR_UNDEFINED,
                diagnostic_codes::CANNOT_INVOKE_AN_OBJECT_WHICH_IS_POSSIBLY_NULL_OR_UNDEFINED,
            )
        };

        self.error_at_node(idx, message, code);
    }
}
