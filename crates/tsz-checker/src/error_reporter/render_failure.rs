//! Render `SubtypeFailureReason` values into diagnostics.
//! Split from `assignability.rs` for maintainability.
use crate::diagnostics::{
    Diagnostic, DiagnosticCategory, DiagnosticRelatedInformation, diagnostic_codes,
    diagnostic_messages, format_message,
};
use crate::error_reporter::fingerprint_policy::DiagnosticAnchorKind;
use crate::error_reporter::type_display_policy::DiagnosticTypeDisplayRole;
use crate::query_boundaries::type_checking_utilities as query_utils;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

use super::assignability::{
    is_builtin_wrapper_name, is_function_type_display, is_object_prototype_method,
    is_object_prototype_method_for_array_target, is_primitive_type_name,
};
mod type_mismatch;
impl<'a> CheckerState<'a> {
    /// Recursively render a `SubtypeFailureReason` into a Diagnostic.
    pub(crate) fn render_failure_reason(
        &mut self,
        reason: &tsz_solver::SubtypeFailureReason,
        source: TypeId,
        target: TypeId,
        idx: NodeIndex,
        depth: u32,
    ) -> Diagnostic {
        use crate::query_boundaries::common::SubtypeFailureReason;

        let source = self.recover_unknown_array_source_type_for_display(source, idx, depth);
        let (start, length) = self
            .resolve_diagnostic_anchor(idx, DiagnosticAnchorKind::Exact)
            .map(|anchor| (anchor.start, anchor.length))
            .unwrap_or_else(|| {
                // get_node_span returns (pos, end); convert to (start, length)
                // and apply the same span normalization as the primary path.
                let (pos, end) = self.get_node_span(idx).unwrap_or((0, 0));
                self.normalized_anchor_span(idx, pos, end.saturating_sub(pos))
            });
        let file_name = self.ctx.file_name.clone();

        // TS2696: property-only failures from the `Object` wrapper use the
        // specialized message unless the target is callable/constructable.
        if depth == 0 {
            let is_property_failure = matches!(
                reason,
                SubtypeFailureReason::MissingProperty { .. }
                    | SubtypeFailureReason::MissingProperties { .. }
                    | SubtypeFailureReason::PropertyTypeMismatch { .. }
                    | SubtypeFailureReason::OptionalPropertyRequired { .. }
                    | SubtypeFailureReason::NoCommonProperties { .. }
            );
            if is_property_failure {
                let src_str = self.format_type_diagnostic(source);
                if src_str == "Object" {
                    return Diagnostic::error(
                        file_name,
                        start,
                        length,
                        diagnostic_messages::THE_OBJECT_TYPE_IS_ASSIGNABLE_TO_VERY_FEW_OTHER_TYPES_DID_YOU_MEAN_TO_USE_THE_AN
                            .to_string(),
                        diagnostic_codes::THE_OBJECT_TYPE_IS_ASSIGNABLE_TO_VERY_FEW_OTHER_TYPES_DID_YOU_MEAN_TO_USE_THE_AN,
                    );
                }
            }
        }
        match reason {
            SubtypeFailureReason::MissingProperty {
                property_name,
                source_type,
                target_type,
            } => self.render_missing_property(
                reason,
                source,
                target,
                idx,
                depth,
                start,
                length,
                file_name,
                *property_name,
                *source_type,
                *target_type,
            ),
            SubtypeFailureReason::MissingProperties {
                property_names,
                source_type,
                target_type,
            } => self.render_missing_properties(
                reason,
                source,
                target,
                idx,
                depth,
                start,
                length,
                file_name,
                property_names,
                *source_type,
                *target_type,
            ),
            SubtypeFailureReason::PropertyTypeMismatch {
                property_name,
                source_property_type,
                target_property_type,
                nested_reason,
            } => self.render_property_type_mismatch(
                reason,
                source,
                target,
                idx,
                depth,
                start,
                length,
                file_name,
                *property_name,
                *source_property_type,
                *target_property_type,
                nested_reason.as_deref(),
            ),
            SubtypeFailureReason::OptionalPropertyRequired { property_name } => self
                .render_optional_property_required(
                    reason,
                    source,
                    target,
                    idx,
                    depth,
                    start,
                    length,
                    file_name,
                    *property_name,
                ),
            SubtypeFailureReason::ReadonlyPropertyMismatch { property_name } => {
                let prop_name = self.ctx.types.resolve_atom_ref(*property_name);
                let message = format_message(
                    diagnostic_messages::CANNOT_ASSIGN_TO_BECAUSE_IT_IS_A_READ_ONLY_PROPERTY,
                    &[&prop_name],
                );
                Diagnostic::error(file_name, start, length, message, reason.diagnostic_code())
            }
            SubtypeFailureReason::PropertyVisibilityMismatch {
                property_name,
                source_visibility,
                target_visibility,
            } => {
                let (source_str, target_str) =
                    self.format_top_level_assignability_message_types_at(source, target, idx);
                let prop_name = self.ctx.types.resolve_atom_ref(*property_name);
                let base = self.property_visibility_assignability_message(
                    &source_str,
                    &target_str,
                    &prop_name,
                    *source_visibility,
                    *target_visibility,
                );
                Diagnostic::error(
                    file_name,
                    start,
                    length,
                    base,
                    diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                )
            }
            SubtypeFailureReason::PropertyNominalMismatch { property_name } => self
                .render_property_nominal_mismatch(
                    reason,
                    source,
                    target,
                    idx,
                    depth,
                    start,
                    length,
                    file_name,
                    *property_name,
                ),
            SubtypeFailureReason::ExcessProperty {
                property_name,
                target_type: _,
            } => {
                let prop_name = self.ctx.types.resolve_atom_ref(*property_name);
                let (code, message) =
                    self.excess_property_diagnostic_message(&prop_name, target, idx);
                let (excess_start, excess_length) = self
                    .find_excess_property_anchor(idx, *property_name)
                    .unwrap_or((start, length));
                Diagnostic::error(file_name, excess_start, excess_length, message, code)
            }
            SubtypeFailureReason::ReturnTypeMismatch {
                source_return,
                target_return,
                nested_reason,
            } => self.render_return_type_mismatch(
                reason,
                source,
                target,
                idx,
                depth,
                start,
                length,
                file_name,
                *source_return,
                *target_return,
                nested_reason.as_deref(),
            ),
            SubtypeFailureReason::TooManyParameters { .. } => {
                let (source_str, target_str) =
                    self.format_top_level_assignability_message_types_at(source, target, idx);
                let message = format_message(
                    diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    &[&source_str, &target_str],
                );
                Diagnostic::error(
                    file_name,
                    start,
                    length,
                    message,
                    diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                )
            }
            SubtypeFailureReason::TupleElementMismatch {
                source_count,
                target_count,
            } => {
                if depth == 0 {
                    let (source_str, target_str) =
                        self.format_top_level_assignability_message_types_at(source, target, idx);
                    let base = format_message(
                        diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                        &[&source_str, &target_str],
                    );
                    Diagnostic::error(
                        file_name,
                        start,
                        length,
                        base,
                        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    )
                } else {
                    let message = format!(
                        "Tuple type has {source_count} elements but target requires {target_count}."
                    );
                    Diagnostic::error(file_name, start, length, message, reason.diagnostic_code())
                }
            }
            SubtypeFailureReason::TupleElementTypeMismatch {
                index,
                source_element,
                target_element,
            } => {
                if depth == 0 {
                    let (source_str, target_str) =
                        self.format_top_level_assignability_message_types_at(source, target, idx);
                    let base = format_message(
                        diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                        &[&source_str, &target_str],
                    );
                    Diagnostic::error(
                        file_name,
                        start,
                        length,
                        base,
                        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    )
                } else {
                    let source_str = self.format_type_diagnostic(*source_element);
                    let target_str = self.format_type_diagnostic(*target_element);
                    let message = format!(
                        "Type of element at index {index} is incompatible: '{source_str}' is not assignable to '{target_str}'."
                    );
                    Diagnostic::error(file_name, start, length, message, reason.diagnostic_code())
                }
            }

            SubtypeFailureReason::ArrayElementMismatch {
                source_element,
                target_element,
            } => {
                if depth == 0 {
                    let (source_str, target_str) =
                        self.format_top_level_assignability_message_types_at(source, target, idx);
                    let base = format_message(
                        diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                        &[&source_str, &target_str],
                    );
                    Diagnostic::error(
                        file_name,
                        start,
                        length,
                        base,
                        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    )
                } else {
                    let source_str = self.format_type_diagnostic(*source_element);
                    let target_str = self.format_type_diagnostic(*target_element);
                    let message = format!(
                        "Array element type '{source_str}' is not assignable to '{target_str}'."
                    );
                    Diagnostic::error(file_name, start, length, message, reason.diagnostic_code())
                }
            }

            SubtypeFailureReason::IndexSignatureMismatch {
                index_kind,
                source_value_type,
                target_value_type,
            } => {
                if depth == 0 {
                    let source_str = self.format_type_for_diagnostic_role(
                        source,
                        DiagnosticTypeDisplayRole::AssignmentSource {
                            target,
                            anchor_idx: idx,
                        },
                    );
                    let target_str = self.format_assignability_type_for_message(target, source);
                    let message = format_message(
                        diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                        &[&source_str, &target_str],
                    );
                    return Diagnostic::error(
                        file_name,
                        start,
                        length,
                        message,
                        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    );
                }
                let source_str = self.format_type_diagnostic(*source_value_type);
                let target_str = self.format_type_diagnostic(*target_value_type);
                let message = format!(
                    "{index_kind} index signature is incompatible: '{source_str}' is not assignable to '{target_str}'."
                );
                Diagnostic::error(file_name, start, length, message, reason.diagnostic_code())
            }

            SubtypeFailureReason::MissingIndexSignature { index_kind } => {
                if depth == 0 {
                    let source_str = self.format_type_for_diagnostic_role(
                        source,
                        DiagnosticTypeDisplayRole::AssignmentSource {
                            target,
                            anchor_idx: idx,
                        },
                    );
                    let target_str = self.format_assignability_type_for_message(target, source);
                    let message = format_message(
                        diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                        &[&source_str, &target_str],
                    );
                    return Diagnostic::error(
                        file_name,
                        start,
                        length,
                        message,
                        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    );
                }
                let source_str = self.format_type_diagnostic(source);
                let message = format_message(
                    diagnostic_messages::INDEX_SIGNATURE_FOR_TYPE_IS_MISSING_IN_TYPE,
                    &[index_kind, &source_str],
                );
                Diagnostic::error(file_name, start, length, message, reason.diagnostic_code())
            }

            SubtypeFailureReason::NoUnionMemberMatches {
                source_type,
                target_union_members: _,
            } => {
                let (mut source_str, target_str) = if depth == 0 {
                    let use_structural_source_display =
                        crate::query_boundaries::common::enum_def_id(self.ctx.types, source)
                            .is_none();
                    (
                        if use_structural_source_display {
                            self.format_type_for_diagnostic_role(
                                source,
                                DiagnosticTypeDisplayRole::AssignmentSource {
                                    target,
                                    anchor_idx: idx,
                                },
                            )
                        } else {
                            self.format_type_diagnostic(*source_type)
                        },
                        if use_structural_source_display {
                            self.format_type_for_diagnostic_role(
                                target,
                                DiagnosticTypeDisplayRole::AssignmentTarget {
                                    source,
                                    anchor_idx: idx,
                                },
                            )
                        } else {
                            self.format_type_diagnostic(target)
                        },
                    )
                } else {
                    (
                        self.format_type_diagnostic(*source_type),
                        self.format_type_diagnostic(target),
                    )
                };
                if let Some(widened) = self.rewrite_standalone_literal_source_for_keyof_display(
                    &source_str,
                    &target_str,
                    target,
                ) {
                    source_str = widened;
                }
                // TS2820 prefers "did you mean X?" and uses the expanded union
                // form rather than the alias name.
                let evaluated_target_for_suggestion = self.evaluate_type_with_env(target);
                if let Some(suggestion) = self.find_string_literal_spelling_suggestion(
                    source,
                    evaluated_target_for_suggestion,
                ) {
                    let expanded_target_str =
                        self.format_type_diagnostic(evaluated_target_for_suggestion);
                    let display_target_str = if expanded_target_str != target_str {
                        &expanded_target_str
                    } else {
                        &target_str
                    };
                    let msg = format_message(
                        diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE_DID_YOU_MEAN,
                        &[&source_str, display_target_str, &suggestion],
                    );
                    return Diagnostic::error(
                        file_name,
                        start,
                        length,
                        msg,
                        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE_DID_YOU_MEAN,
                    );
                }
                let message = format_message(
                    diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    &[&source_str, &target_str],
                );
                Diagnostic::error(
                    file_name,
                    start,
                    length,
                    message,
                    diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                )
            }

            SubtypeFailureReason::NoCommonProperties {
                source_type: _,
                target_type: _,
            } => {
                // Use the unwidened source: tsc preserves literal spellings in
                // "has no properties in common" messages.
                let mut source_str =
                    if (crate::query_boundaries::common::has_call_signatures(
                        self.ctx.types,
                        source,
                    ) || crate::query_boundaries::common::has_construct_signatures(
                        self.ctx.types,
                        source,
                    )) && depth == 0
                    {
                        let widened_source = self.widen_type_for_display(source);
                        let widened_source = self.widen_function_like_display_type(widened_source);
                        self.format_type_for_assignability_message(widened_source)
                    } else {
                        self.format_type_diagnostic(source)
                    };
                let target_str = self.format_type_for_assignability_message(target);

                // If calling the source would fix the mismatch, emit TS2560 instead.
                let (msg_template, code) = if self
                    .should_suggest_calling_for_weak_type(source, target)
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
                    source_str = Self::widen_member_literals_in_display_text(&source_str);
                }
                let (source_str, target_str) = self
                    .finalize_pair_display_for_diagnostic(source, target, source_str, target_str);
                let message = format_message(msg_template, &[&source_str, &target_str]);
                Diagnostic::error(file_name, start, length, message, code)
            }

            SubtypeFailureReason::TypeMismatch {
                source_type: _,
                target_type: _,
            } => self
                .render_type_mismatch(reason, source, target, idx, depth, start, length, file_name),

            SubtypeFailureReason::ReadonlyToMutableAssignment {
                source_type,
                target_type,
            } => {
                let source_str = self.format_type_diagnostic(*source_type);
                let target_str = self.format_type_diagnostic(*target_type);
                let message = format_message(
                    diagnostic_messages::THE_TYPE_IS_READONLY_AND_CANNOT_BE_ASSIGNED_TO_THE_MUTABLE_TYPE,
                    &[&source_str, &target_str],
                );
                Diagnostic::error(
                    file_name,
                    start,
                    length,
                    message,
                    diagnostic_codes::THE_TYPE_IS_READONLY_AND_CANNOT_BE_ASSIGNED_TO_THE_MUTABLE_TYPE,
                )
            }

            SubtypeFailureReason::ParameterTypeMismatch {
                param_index,
                source_param,
                target_param,
            } => {
                let source_str = self.format_type_for_diagnostic_role(
                    source,
                    DiagnosticTypeDisplayRole::AssignmentSource {
                        target,
                        anchor_idx: idx,
                    },
                );
                let target_str = self.format_assignability_type_for_message(target, source);
                let message = format_message(
                    diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    &[&source_str, &target_str],
                );
                let primary = Diagnostic::error(
                    file_name.clone(),
                    start,
                    length,
                    message,
                    diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                );

                // TS2328 is emitted separately only for top-level direct callable
                // mismatches whose parameter types are callable and non-generic.
                let is_callable =
                    |ty| crate::query_boundaries::common::is_callable_type(self.ctx.types, ty);
                let contains_type_params = |ty| {
                    crate::query_boundaries::common::contains_type_parameters(self.ctx.types, ty)
                };
                let source_is_direct_callable = is_callable(source);
                let target_is_direct_callable = is_callable(target);
                let source_param_is_callable = is_callable(*source_param);
                let target_param_is_callable = is_callable(*target_param);
                let source_param_is_generic = contains_type_params(*source_param);
                let target_param_is_generic = contains_type_params(*target_param);

                if depth == 0
                    && source_is_direct_callable
                    && target_is_direct_callable
                    && source_param_is_callable
                    && target_param_is_callable
                    && !source_param_is_generic
                    && !target_param_is_generic
                {
                    let source_name = crate::query_boundaries::common::get_callable_shape_for_type(
                        self.ctx.types,
                        source,
                    )
                    .and_then(|shape| {
                        shape
                            .call_signatures
                            .first()
                            .and_then(|sig| sig.params.get(*param_index).and_then(|p| p.name))
                    })
                    .map(|a| self.ctx.types.resolve_atom(a))
                    .unwrap_or_else(|| format!("arg{param_index}"));

                    let target_name = crate::query_boundaries::common::get_callable_shape_for_type(
                        self.ctx.types,
                        target,
                    )
                    .and_then(|shape| {
                        shape
                            .call_signatures
                            .first()
                            .and_then(|sig| sig.params.get(*param_index).and_then(|p| p.name))
                    })
                    .map(|a| self.ctx.types.resolve_atom(a))
                    .unwrap_or_else(|| format!("arg{param_index}"));

                    let ts2328_message = format_message(
                        diagnostic_messages::TYPES_OF_PARAMETERS_AND_ARE_INCOMPATIBLE,
                        &[&source_name, &target_name],
                    );
                    let ts2328_diag = Diagnostic::error(
                        file_name,
                        start,
                        length,
                        ts2328_message,
                        diagnostic_codes::TYPES_OF_PARAMETERS_AND_ARE_INCOMPATIBLE,
                    );
                    self.ctx.push_diagnostic(ts2328_diag);
                }

                primary
            }

            _ => {
                let source_str = self.format_type_for_diagnostic_role(
                    source,
                    DiagnosticTypeDisplayRole::AssignmentSource {
                        target,
                        anchor_idx: idx,
                    },
                );
                let target_str = self.format_assignability_type_for_message(target, source);
                let message = format_message(
                    diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    &[&source_str, &target_str],
                );
                Diagnostic::error(
                    file_name,
                    start,
                    length,
                    message,
                    diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                )
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn render_missing_property(
        &mut self,
        _reason: &tsz_solver::SubtypeFailureReason,
        source: TypeId,
        target: TypeId,
        idx: NodeIndex,
        depth: u32,
        start: u32,
        length: u32,
        file_name: String,
        property_name: tsz_common::interner::Atom,
        source_type: TypeId,
        target_type: TypeId,
    ) -> Diagnostic {
        // Primitive sources use TS2322 rather than missing-property wording.
        let display_src_str = if depth == 0 && source_type != tsz_solver::TypeId::OBJECT {
            self.format_type_for_diagnostic_role(
                source,
                DiagnosticTypeDisplayRole::AssignmentSource {
                    target,
                    anchor_idx: idx,
                },
            )
        } else {
            self.format_type_diagnostic(source_type)
        };
        // Distinguish "outer source is primitive" (e.g. `let y: Foo = 42`) from
        // "inner source_type is primitive" (e.g. assigning `{ one: number }` to
        // `{ [k: string]: Foo }`, where the solver reports `MissingProperty(foo,
        // src_ty=number, tgt_ty=Foo)` describing the failed nested check). In
        // the first case we want the primitive-vs-target message; in the second
        // we want the OUTER source/target shown, not the inner property types.
        let outer_source_is_primitive =
            crate::query_boundaries::common::is_primitive_type(self.ctx.types, source)
                || is_primitive_type_name(&display_src_str);
        let inner_source_type_is_primitive = source_type != tsz_solver::TypeId::OBJECT
            && crate::query_boundaries::common::is_primitive_type(self.ctx.types, source_type);
        let is_source_primitive =
            outer_source_is_primitive || (depth > 0 && inner_source_type_is_primitive);
        if is_source_primitive {
            let tgt_str = self.format_type_diagnostic(target_type);
            let message = format_message(
                diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                &[&display_src_str, &tgt_str],
            );
            return Diagnostic::error(
                file_name,
                start,
                length,
                message,
                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            );
        }

        // Pure function sources against non-callable targets use TS2322; class
        // constructors still keep the missing-property path.
        let source_eval_for_fn = self.evaluate_type_with_env(source);
        let target_eval_for_fn = self.evaluate_type_with_env(target);
        let is_source_fn =
            crate::query_boundaries::common::has_call_signatures(self.ctx.types, source)
                || crate::query_boundaries::common::has_call_signatures(
                    self.ctx.types,
                    source_eval_for_fn,
                )
                || crate::query_boundaries::common::has_call_signatures(
                    self.ctx.types,
                    source_type,
                )
                || crate::query_boundaries::common::has_call_signatures(
                    self.ctx.types,
                    self.evaluate_type_with_env(source_type),
                )
                || is_function_type_display(&display_src_str);
        let target_has_call_sigs =
            crate::query_boundaries::common::has_call_signatures(self.ctx.types, target)
                || crate::query_boundaries::common::has_call_signatures(
                    self.ctx.types,
                    target_eval_for_fn,
                );
        if is_source_fn && !target_has_call_sigs {
            let src_str = if depth == 0 {
                self.format_type_for_diagnostic_role(
                    source,
                    DiagnosticTypeDisplayRole::AssignmentSource {
                        target,
                        anchor_idx: idx,
                    },
                )
            } else {
                self.format_type_diagnostic(source_type)
            };
            let tgt_str = if depth == 0 {
                self.format_assignability_type_for_message(target, source)
            } else {
                self.format_type_diagnostic(target_type)
            };
            let message = format_message(
                diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                &[&src_str, &tgt_str],
            );
            return Diagnostic::error(
                file_name,
                start,
                length,
                message,
                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            );
        }

        // TSC emits TS2322 instead of TS2741 when the target has an index signature but the
        // missing property is not a direct named property of the target. In this case, the
        // "missing" property comes from the index signature value type, not from a required
        // named property, so the generic assignability error is more appropriate.
        // Skip this check for array/tuple targets: their properties (like `length`) come
        // from the Array interface and ARE named properties even though the array also has
        // a numeric index signature.
        {
            use crate::query_boundaries::common::{IndexKind, IndexSignatureResolver};
            let resolver = IndexSignatureResolver::new(self.ctx.types);
            let target_is_array_or_tuple =
                crate::query_boundaries::common::array_element_type(self.ctx.types, target)
                    .is_some()
                    || crate::query_boundaries::common::tuple_list_id(self.ctx.types, target)
                        .is_some();
            let target_has_index = !target_is_array_or_tuple
                && (resolver.has_index_signature(target, IndexKind::String)
                    || resolver.has_index_signature(target, IndexKind::Number));
            if target_has_index {
                let prop_name_str = self.ctx.types.resolve_atom_ref(property_name);
                let target_has_named_prop = crate::query_boundaries::common::find_property_by_str(
                    self.ctx.types,
                    target,
                    &prop_name_str,
                )
                .is_some();
                if !target_has_named_prop {
                    let src_str = if depth == 0 {
                        self.format_type_for_diagnostic_role(
                            source,
                            DiagnosticTypeDisplayRole::AssignmentSource {
                                target,
                                anchor_idx: idx,
                            },
                        )
                    } else {
                        self.format_type_diagnostic(source_type)
                    };
                    let tgt_str = if depth == 0 {
                        self.format_assignability_type_for_message(target, source)
                    } else {
                        self.format_type_diagnostic(target_type)
                    };
                    let message = format_message(
                        diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                        &[&src_str, &tgt_str],
                    );
                    return Diagnostic::error(
                        file_name,
                        start,
                        length,
                        message,
                        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    );
                }
            }
        }

        // TSC emits TS2322 instead of TS2741 when both source and target have index signatures.
        // For index signature to index signature assignments, the more general assignability error
        // is preferred over specific missing property errors.
        // Skip for array/tuple targets — their numeric index is implicit and missing named
        // properties (like `length`) should still produce TS2741.
        use crate::query_boundaries::common::{IndexKind, IndexSignatureResolver};
        let resolver = IndexSignatureResolver::new(self.ctx.types);
        // Check both original and evaluated types (needed for generic class instances)
        let source_evaluated = self.evaluate_type_with_env(source);
        let target_evaluated = self.evaluate_type_with_env(target);
        let target_is_array_or_tuple_for_idx =
            crate::query_boundaries::common::array_element_type(self.ctx.types, target).is_some()
                || crate::query_boundaries::common::tuple_list_id(self.ctx.types, target).is_some();
        let source_has_index = [source, source_evaluated].iter().any(|t| {
            resolver.has_index_signature(*t, IndexKind::String)
                || resolver.has_index_signature(*t, IndexKind::Number)
        });
        let target_has_index = !target_is_array_or_tuple_for_idx
            && [target, target_evaluated].iter().any(|t| {
                resolver.has_index_signature(*t, IndexKind::String)
                    || resolver.has_index_signature(*t, IndexKind::Number)
            });
        if source_has_index && target_has_index {
            let src_str = self.format_type_diagnostic(source);
            let tgt_str = self.format_type_diagnostic(target);
            let message = format_message(
                diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                &[&src_str, &tgt_str],
            );
            return Diagnostic::error(
                file_name,
                start,
                length,
                message,
                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            );
        }

        // Also emit TS2322 for wrapper-like built-ins (Boolean, Number, String, Object)
        let tgt_str = self.format_type_diagnostic(target_type);
        let original_tgt_str = self.format_type_diagnostic(target);
        if is_builtin_wrapper_name(&tgt_str) || is_builtin_wrapper_name(&original_tgt_str) {
            let src_str = self.format_type_diagnostic(source_type);
            let display_tgt = if is_builtin_wrapper_name(&original_tgt_str) {
                &original_tgt_str
            } else {
                &tgt_str
            };
            let message = format_message(
                diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                &[&src_str, display_tgt],
            );
            return Diagnostic::error(
                file_name,
                start,
                length,
                message,
                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            );
        }

        // TSC emits TS2322 instead of TS2741 when the target type is an intersection type.
        let target_evaluated_for_intersection = self.evaluate_type_with_env(target);
        if crate::query_boundaries::common::is_intersection_type(self.ctx.types, target_type)
            || crate::query_boundaries::common::is_intersection_type(self.ctx.types, target)
            || crate::query_boundaries::common::is_intersection_type(
                self.ctx.types,
                target_evaluated_for_intersection,
            )
        {
            let src_str = self.format_type_diagnostic(source_type);
            let tgt_str = if crate::query_boundaries::common::is_intersection_type(
                self.ctx.types,
                target_evaluated_for_intersection,
            ) {
                self.format_type_diagnostic(target_evaluated_for_intersection)
            } else if crate::query_boundaries::common::is_intersection_type(self.ctx.types, target)
            {
                self.format_type_diagnostic(target)
            } else {
                self.format_type_diagnostic(target_type)
            };
            let message = format_message(
                diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                &[&src_str, &tgt_str],
            );
            return Diagnostic::error(
                file_name,
                start,
                length,
                message,
                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            );
        }

        // TSC emits TS2322 instead of TS2741 when the *source* type is an intersection.
        // This covers type aliases like `LinkedList<T> = T & { next: ... }` that may have
        // been evaluated to an intersection by the time we reach diagnostic rendering.
        // Check both the type data and the source's declaration annotation, since
        // intersections may be flattened into Object types by the solver.
        let source_evaluated_for_intersection = self.evaluate_type_with_env(source);
        if crate::query_boundaries::common::is_intersection_type(self.ctx.types, source)
            || crate::query_boundaries::common::is_intersection_type(
                self.ctx.types,
                source_evaluated_for_intersection,
            )
            || (depth == 0 && self.anchor_source_has_intersection_annotation(idx))
        {
            let src_str = if depth == 0 {
                self.format_type_for_diagnostic_role(
                    source,
                    DiagnosticTypeDisplayRole::AssignmentSource {
                        target,
                        anchor_idx: idx,
                    },
                )
            } else {
                self.format_type_diagnostic(source_type)
            };
            let tgt_str = if depth == 0 {
                self.format_assignability_type_for_message(target, source)
            } else {
                self.format_type_diagnostic(target_type)
            };
            let message = format_message(
                diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                &[&src_str, &tgt_str],
            );
            return Diagnostic::error(
                file_name,
                start,
                length,
                message,
                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            );
        }

        // TSC emits TS2322 instead of TS2741 when the source is a type application
        // (generic type alias) whose base type resolves to an intersection. For example,
        // `LinkedList<Entity>` where `type LinkedList<T> = T & { next: LinkedList<T> }`.
        // Named type aliases expanding to intersections are reported as general
        // assignability failures, not property-level "missing" errors.
        if let Some((base, _args)) =
            crate::query_boundaries::common::application_info(self.ctx.types, source)
        {
            let base_eval = self.evaluate_type_with_env(base);
            let base_is_intersection =
                crate::query_boundaries::common::is_intersection_type(self.ctx.types, base)
                    || crate::query_boundaries::common::is_intersection_type(
                        self.ctx.types,
                        base_eval,
                    );
            if base_is_intersection {
                let src_str = if depth == 0 {
                    self.format_type_for_diagnostic_role(
                        source,
                        DiagnosticTypeDisplayRole::AssignmentSource {
                            target,
                            anchor_idx: idx,
                        },
                    )
                } else {
                    self.format_type_diagnostic(source_type)
                };
                let tgt_str = if depth == 0 {
                    self.format_assignability_type_for_message(target, source)
                } else {
                    self.format_type_diagnostic(target_type)
                };
                let message = format_message(
                    diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    &[&src_str, &tgt_str],
                );
                return Diagnostic::error(
                    file_name,
                    start,
                    length,
                    message,
                    diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                );
            }
        }

        // Private brand properties handling
        let prop_name = self.ctx.types.resolve_atom_ref(property_name).to_string();
        if prop_name.starts_with("__private_brand") {
            let src_str = if depth == 0 {
                self.format_type_for_diagnostic_role(
                    source,
                    DiagnosticTypeDisplayRole::AssignmentSource {
                        target,
                        anchor_idx: idx,
                    },
                )
            } else {
                self.format_type_for_assignability_message(source_type)
            };
            let tgt_str = if depth == 0 {
                self.format_assignability_type_for_message(target, source)
            } else {
                self.format_type_for_assignability_message(target_type)
            };
            // Try to find the backing private/protected member for a detailed message.
            if depth == 0
                && let Some((member_name, owner_name, visibility)) =
                    self.private_or_protected_member_missing_display(source, target, None)
            {
                let message = self.private_or_protected_assignability_message(
                    &src_str,
                    &tgt_str,
                    &member_name,
                    &owner_name,
                    visibility,
                    None,
                );
                return Diagnostic::error(
                    file_name,
                    start,
                    length,
                    message,
                    diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                );
            }
            // Source HAS the property but with wrong visibility/nominal identity.
            if depth == 0
                && let Some((display_prop, owner_name, visibility)) =
                    self.private_or_protected_brand_backing_member_display(target, None)
            {
                let message = self.private_or_protected_assignability_message(
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
                );
                return Diagnostic::error(
                    file_name,
                    start,
                    length,
                    message,
                    diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                );
            }
            let message = format_message(
                diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                &[&src_str, &tgt_str],
            );
            return Diagnostic::error(
                file_name,
                start,
                length,
                message,
                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            );
        }

        // TSC emits TS2322 instead of TS2741 when the target is an intersection type.
        if crate::query_boundaries::common::is_intersection_type(self.ctx.types, target_type)
            || crate::query_boundaries::common::is_intersection_type(self.ctx.types, target)
        {
            let src_str = self.format_type_diagnostic(source);
            let tgt_str_full = self.format_type_diagnostic(target);
            let message = format_message(
                diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                &[&src_str, &tgt_str_full],
            );
            return Diagnostic::error(
                file_name,
                start,
                length,
                message,
                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            );
        }

        // TSC emits TS2322 when the target's declared type annotation is an intersection type.
        if self.anchor_target_has_intersection_annotation(idx) {
            let src_str = self.format_type_diagnostic(source);
            let tgt_str_full = self.format_type_diagnostic(target);
            let message = format_message(
                diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                &[&src_str, &tgt_str_full],
            );
            return Diagnostic::error(
                file_name,
                start,
                length,
                message,
                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            );
        }

        // Object.prototype methods → emit TS2322 instead of TS2741.
        if is_object_prototype_method(&prop_name) {
            let src_str = self.format_type_diagnostic(source_type);
            let tgt_str = self.format_type_diagnostic(target_type);
            let message = format_message(
                diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                &[&src_str, &tgt_str],
            );
            return Diagnostic::error(
                file_name,
                start,
                length,
                message,
                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            );
        }

        // When the source has an index signature, upgrade TS2741 → TS2739 when needed.
        if depth == 0
            && let Some(all_missing) =
                self.missing_required_properties_from_index_signature_source(source, target)
            && all_missing.len() > 1
        {
            let src_str = self.format_type_for_diagnostic_role(
                source,
                DiagnosticTypeDisplayRole::AssignmentSource {
                    target,
                    anchor_idx: idx,
                },
            );
            let tgt_str = self.format_assignability_type_for_message(target, source);
            let prop_list: Vec<String> = all_missing
                .iter()
                .take(4)
                .map(|name| self.missing_property_name_for_display(*name, target))
                .collect();
            let props_joined = prop_list.join(", ");
            let (message, code) = if all_missing.len() > 4 {
                let more_count = (all_missing.len() - 4).to_string();
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
            };
            return Diagnostic::error(file_name, start, length, message, code);
        }

        // TS2741: Property 'x' is missing in type 'A' but required in type 'B'.
        let widened_source = self.widen_type_for_display(source_type);
        let (mut src_str, mut tgt_str_qualified) = if depth == 0 {
            let src = if source_type == TypeId::OBJECT {
                "{}".to_string()
            } else {
                self.format_type_for_diagnostic_role(
                    source,
                    DiagnosticTypeDisplayRole::AssignmentSource {
                        target,
                        anchor_idx: idx,
                    },
                )
            };
            let widened_target = self.widen_fresh_object_literal_properties_for_display(target);
            (
                src,
                self.format_type_for_diagnostic_role(
                    widened_target,
                    DiagnosticTypeDisplayRole::FlattenedDiagnostic,
                ),
            )
        } else if source_type == TypeId::OBJECT {
            ("{}".to_string(), tgt_str)
        } else {
            self.format_type_pair_diagnostic(widened_source, target)
        };
        // When source and target collapse to the same short name (e.g. two
        // same-named classes from different modules), re-qualify them so the
        // reader can tell them apart. The formatter's pair-disambiguation
        // path adds namespace or `import("<specifier>")` prefixes only when
        // the bare names collide.
        //
        // Two cases:
        //   1. `src_str == tgt_str_qualified`: both formatted to the same
        //      short name — disambiguate both sides.
        //   2. `src_str` was already qualified by expression text (e.g.
        //      `N.A` from `new N.A()`) but the underlying source and target
        //      types still share a bare formatted name (e.g. both "A").
        //      Keep the source text as-is and only qualify the target.
        if widened_source != target {
            if src_str == tgt_str_qualified {
                let (da, db) = self.format_type_pair_diagnostic(widened_source, target);
                src_str = da;
                tgt_str_qualified = db;
            } else {
                let fmt_src_bare = self.format_type_diagnostic(widened_source);
                let fmt_tgt_bare = self.format_type_diagnostic(target);
                if fmt_src_bare == fmt_tgt_bare {
                    let (_, db) = self.format_type_pair_diagnostic(widened_source, target);
                    if db != tgt_str_qualified {
                        tgt_str_qualified = db;
                    }
                }
            }
        }
        let prop_name_display = self.missing_property_name_for_display(property_name, target);
        let message = format_message(
            diagnostic_messages::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE,
            &[&prop_name_display, &src_str, &tgt_str_qualified],
        );
        Diagnostic::error(
            file_name,
            start,
            length,
            message,
            diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn render_missing_properties(
        &mut self,
        _reason: &tsz_solver::SubtypeFailureReason,
        source: TypeId,
        target: TypeId,
        idx: NodeIndex,
        depth: u32,
        start: u32,
        length: u32,
        file_name: String,
        property_names: &[tsz_common::interner::Atom],
        source_type: TypeId,
        target_type: TypeId,
    ) -> Diagnostic {
        // TSC emits TS2322 instead of TS2739/TS2740 when the source is a primitive type.
        if crate::query_boundaries::common::is_primitive_type(self.ctx.types, source_type) {
            let src_str = self.format_type_diagnostic(source_type);
            let tgt_str = self.format_type_diagnostic(target_type);
            let message = format_message(
                diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                &[&src_str, &tgt_str],
            );
            return Diagnostic::error(
                file_name,
                start,
                length,
                message,
                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            );
        }

        // TSC emits TS2322 instead of TS2739/TS2740/TS2741 when the source has call
        // signatures (pure function type, NOT class constructor) and the target does NOT
        // have call signatures. Class constructors (with construct-only signatures) should
        // still produce TS2741 for missing properties.
        {
            let source_eval = self.evaluate_type_with_env(source);
            let target_eval = self.evaluate_type_with_env(target);
            let display_src = self.format_type_diagnostic(source_type);
            let is_src_fn =
                crate::query_boundaries::common::has_call_signatures(self.ctx.types, source)
                    || crate::query_boundaries::common::has_call_signatures(
                        self.ctx.types,
                        source_eval,
                    )
                    || crate::query_boundaries::common::has_call_signatures(
                        self.ctx.types,
                        source_type,
                    )
                    || crate::query_boundaries::common::has_call_signatures(
                        self.ctx.types,
                        self.evaluate_type_with_env(source_type),
                    )
                    || is_function_type_display(&display_src);
            // Types with construct signatures (class constructors like DateConstructor)
            // are NOT pure function types — they should still produce TS2740/TS2741
            // for missing properties instead of being downgraded to TS2322.
            let src_has_construct =
                crate::query_boundaries::common::has_construct_signatures(self.ctx.types, source)
                    || crate::query_boundaries::common::has_construct_signatures(
                        self.ctx.types,
                        source_eval,
                    )
                    || crate::query_boundaries::common::has_construct_signatures(
                        self.ctx.types,
                        source_type,
                    );
            let tgt_has_call =
                crate::query_boundaries::common::has_call_signatures(self.ctx.types, target)
                    || crate::query_boundaries::common::has_call_signatures(
                        self.ctx.types,
                        target_eval,
                    );
            if is_src_fn && !tgt_has_call && !src_has_construct {
                let src_str = if depth == 0 {
                    self.format_type_for_diagnostic_role(
                        source,
                        DiagnosticTypeDisplayRole::AssignmentSource {
                            target,
                            anchor_idx: idx,
                        },
                    )
                } else {
                    self.format_type_diagnostic(source_type)
                };
                let tgt_str = if depth == 0 {
                    self.format_assignability_type_for_message(target, source)
                } else {
                    self.format_type_diagnostic(target_type)
                };
                let message = format_message(
                    diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    &[&src_str, &tgt_str],
                );
                return Diagnostic::error(
                    file_name,
                    start,
                    length,
                    message,
                    diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                );
            }
        }

        // Note: TS2696 for `Object` source is handled at the top of render_failure_reason.

        // Emit TS2322 instead of TS2739/TS2740 when the SOURCE is a wrapper-like built-in.
        let src_str_check = self.format_type_diagnostic(source_type);
        let original_src_check = self.format_type_diagnostic(source);
        if is_builtin_wrapper_name(&src_str_check) || is_builtin_wrapper_name(&original_src_check) {
            let display_src = if is_builtin_wrapper_name(&original_src_check) {
                &original_src_check
            } else {
                &src_str_check
            };
            let tgt_str = self.format_type_diagnostic(target_type);
            let message = format_message(
                diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                &[display_src, &tgt_str],
            );
            return Diagnostic::error(
                file_name,
                start,
                length,
                message,
                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            );
        }

        // TSC emits TS2322 instead of TS2739/TS2740 when the target is an intersection type.
        if crate::query_boundaries::common::is_intersection_type(self.ctx.types, target_type)
            || crate::query_boundaries::common::is_intersection_type(self.ctx.types, target)
            || self.anchor_jsdoc_type_tag_targets_intersection_alias(idx)
        {
            let src_str = self.format_type_diagnostic(source);
            let tgt_str = self.format_type_diagnostic(target);
            let message = format_message(
                diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                &[&src_str, &tgt_str],
            );
            return Diagnostic::error(
                file_name,
                start,
                length,
                message,
                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            );
        }

        // TSC emits TS2322 instead of TS2739/TS2740 when both source and target have
        // string index signatures. For number index signatures, suppress only when the
        // target has no explicit named properties (i.e., it's purely an index-signature
        // type like `{ [x: number]: T }`). Named interfaces that happen to have number
        // index signatures (like String, Array) should still get TS2739/TS2740.
        use crate::query_boundaries::common::{IndexKind, IndexSignatureResolver};
        let resolver = IndexSignatureResolver::new(self.ctx.types);
        // Check both original and evaluated types (needed for generic class instances)
        let source_evaluated = self.evaluate_type_with_env(source);
        let target_evaluated = self.evaluate_type_with_env(target);
        let source_has_string_index = [source, source_evaluated]
            .iter()
            .any(|t| resolver.has_index_signature(*t, IndexKind::String));
        let target_has_string_index = [target, target_evaluated]
            .iter()
            .any(|t| resolver.has_index_signature(*t, IndexKind::String));
        let source_has_number_index = [source, source_evaluated]
            .iter()
            .any(|t| resolver.has_index_signature(*t, IndexKind::Number));
        let target_has_number_index = [target, target_evaluated]
            .iter()
            .any(|t| resolver.has_index_signature(*t, IndexKind::Number));
        // For number index signatures, only suppress when the missing properties are
        // NOT explicitly declared on the target (they came from index value type expansion).
        // We detect this by checking if none of the missing property names match a real
        // named member of the target type's object shape.
        let number_index_suppress =
            source_has_number_index && target_has_number_index && !property_names.is_empty() && {
                let target_shape = crate::query_boundaries::common::object_shape_for_type(
                    self.ctx.types,
                    target_type,
                );
                property_names.iter().all(|name| {
                    // If none of the missing properties are real named members of the
                    // target type, the "missing properties" came from index value type
                    // comparison, not from actual missing named members.
                    match &target_shape {
                        Some(shape) => !shape.properties.iter().any(|p| p.name == *name),
                        None => true,
                    }
                })
            };
        // When the target is an array/tuple type, the missing properties (length, push,
        // pop, etc.) are real named members, not artifacts of index signature comparison.
        // Don't suppress TS2739/TS2740 in that case — tsc correctly emits them.
        let is_array_target = matches!(
            query_utils::classify_array_like(self.ctx.types, target_type),
            query_utils::ArrayLikeKind::Array(_)
                | query_utils::ArrayLikeKind::Tuple
                | query_utils::ArrayLikeKind::Readonly(_)
        );
        if !is_array_target
            && ((source_has_string_index && target_has_string_index) || number_index_suppress)
        {
            let src_str = self.format_type_diagnostic(source);
            let tgt_str = self.format_type_diagnostic(target);
            let message = format_message(
                diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                &[&src_str, &tgt_str],
            );
            return Diagnostic::error(
                file_name,
                start,
                length,
                message,
                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            );
        }
        let _has_non_proto_missing = property_names.iter().any(|name| {
            let s = self.ctx.types.resolve_atom_ref(*name);
            !s.starts_with("__private_brand")
                && if is_array_target {
                    !is_object_prototype_method_for_array_target(&s)
                } else {
                    !is_object_prototype_method(&s)
                }
        });
        let filtered_names: Vec<_> = property_names
            .iter()
            .filter(|name| {
                let s = self.ctx.types.resolve_atom_ref(**name);
                if s.starts_with("__private_brand") {
                    return false;
                }
                if is_array_target {
                    !is_object_prototype_method_for_array_target(&s)
                } else {
                    !is_object_prototype_method(&s)
                }
            })
            .copied()
            .collect();

        // If all missing properties are numeric indices, emit TS2322.
        let all_numeric = !filtered_names.is_empty()
            && filtered_names.iter().all(|name| {
                let s = self.ctx.types.resolve_atom_ref(*name);
                s.parse::<usize>().is_ok()
            });

        if all_numeric {
            let src_str = self.format_type_diagnostic(source_type);
            let tgt_str = self.format_type_diagnostic(target_type);
            let message = format_message(
                diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                &[&src_str, &tgt_str],
            );
            return Diagnostic::error(
                file_name,
                start,
                length,
                message,
                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            );
        }

        // If all missing properties were private brands, emit TS2322 instead.
        if filtered_names.is_empty() {
            if let Some((prop_name, owner_name, visibility)) =
                self.private_or_protected_member_missing_display(source_type, target_type, None)
            {
                let widened_source = self.widen_type_for_display(source_type);
                let src_str = if source_type == TypeId::OBJECT {
                    "{}".to_string()
                } else {
                    self.format_type_diagnostic(widened_source)
                };
                let tgt_str = self.format_type_diagnostic(target_type);
                let message = self.private_or_protected_assignability_message(
                    &src_str,
                    &tgt_str,
                    &prop_name,
                    &owner_name,
                    visibility,
                    None,
                );
                return Diagnostic::error(
                    file_name,
                    start,
                    length,
                    message,
                    diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                );
            }
            let src_str = if depth == 0 {
                if source_type == TypeId::OBJECT {
                    "{}".to_string()
                } else {
                    let source_display = self.format_type_for_diagnostic_role(
                        source,
                        DiagnosticTypeDisplayRole::AssignmentSource {
                            target,
                            anchor_idx: idx,
                        },
                    );
                    self.rewrite_source_display_for_non_literal_target_assignability(
                        source,
                        target,
                        source_display,
                    )
                }
            } else if source_type == TypeId::OBJECT {
                "{}".to_string()
            } else {
                let widened_source = self.widen_type_for_display(source_type);
                self.format_type_diagnostic(widened_source)
            };
            let tgt_str = if depth == 0 {
                self.format_assignability_type_for_message(target, source)
            } else {
                self.format_type_diagnostic(target_type)
            };
            let message = format_message(
                diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                &[&src_str, &tgt_str],
            );
            return Diagnostic::error(
                file_name,
                start,
                length,
                message,
                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            );
        }

        // When filtering removed brand/prototype properties and only 1 remains, emit TS2741.
        if filtered_names.len() == 1 {
            let prop_name = self
                .ctx
                .types
                .resolve_atom_ref(filtered_names[0])
                .to_string();

            // When the source is a function/callable type and the remaining property is
            // private or protected, the function fundamentally can't satisfy the class's
            // nominal brand requirement. TSC emits TS2322 (general mismatch) here, not
            // TS2741 (missing property). For class-to-class assignments, TSC keeps TS2741.
            let source_is_function =
                crate::query_boundaries::common::is_function_type(self.ctx.types, source)
                    || crate::query_boundaries::common::is_function_type(
                        self.ctx.types,
                        source_type,
                    );
            if source_is_function
                && let Some(prop_info) =
                    self.property_info_for_display(target_type, filtered_names[0])
                && prop_info.visibility != tsz_solver::Visibility::Public
            {
                let src_str = if depth == 0 {
                    self.format_type_for_diagnostic_role(
                        source,
                        DiagnosticTypeDisplayRole::AssignmentSource {
                            target,
                            anchor_idx: idx,
                        },
                    )
                } else {
                    self.format_type_diagnostic(source_type)
                };
                let tgt_str = if depth == 0 {
                    self.format_assignability_type_for_message(target, source)
                } else {
                    self.format_type_diagnostic(target_type)
                };
                let message = format_message(
                    diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    &[&src_str, &tgt_str],
                );
                return Diagnostic::error(
                    file_name,
                    start,
                    length,
                    message,
                    diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                );
            }

            let src_str = if depth == 0 {
                if source_type == TypeId::OBJECT {
                    "{}".to_string()
                } else {
                    self.format_type_for_diagnostic_role(
                        source,
                        DiagnosticTypeDisplayRole::AssignmentSource {
                            target,
                            anchor_idx: idx,
                        },
                    )
                }
            } else if source_type == TypeId::OBJECT {
                "{}".to_string()
            } else {
                let widened_source = self.widen_type_for_display(source_type);
                self.format_type_diagnostic(widened_source)
            };
            let tgt_str = self
                .property_declaring_type_name(target_type, filtered_names[0])
                .unwrap_or_else(|| {
                    if depth == 0 {
                        self.format_assignability_type_for_message(target, source)
                    } else {
                        self.format_type_diagnostic(target_type)
                    }
                });
            let message = format_message(
                diagnostic_messages::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE,
                &[&prop_name, &src_str, &tgt_str],
            );
            return Diagnostic::error(
                file_name,
                start,
                length,
                message,
                diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE,
            );
        }

        // TS2739/TS2740: Type 'A' is missing the following properties from type 'B': x, y, z
        let display_source = if self
            .missing_required_properties_from_index_signature_source(source_type, target_type)
            .is_some()
        {
            self.evaluate_type_for_assignability(source_type)
        } else {
            source_type
        };
        let src_str = if depth == 0 {
            self.format_type_for_diagnostic_role(
                source,
                DiagnosticTypeDisplayRole::AssignmentSource {
                    target,
                    anchor_idx: idx,
                },
            )
        } else {
            self.format_type_diagnostic(self.widen_type_for_display(display_source))
        };
        let tgt_str = if depth == 0 {
            self.format_assignability_type_for_message(target, source)
        } else {
            self.format_type_diagnostic(target_type)
        };
        let ordered_names =
            self.sort_missing_property_names_for_display(target_type, &filtered_names);
        // tsc lists up to 5 properties inline (TS2739), and uses "and N more"
        // truncation (TS2740) when there are 6+. For TS2740, tsc lists the
        // first 4 properties then "and N more" (where N = total - 4).
        let is_truncated = ordered_names.len() > 5;
        let display_count = if is_truncated { 4 } else { 5 };
        let prop_list: Vec<String> = ordered_names
            .iter()
            .take(display_count)
            .map(|name| self.missing_property_name_for_display(*name, target))
            .collect();
        let props_joined = prop_list.join(", ");
        if is_truncated {
            let more_count = (ordered_names.len() - display_count).to_string();
            let message = format_message(
                diagnostic_messages::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE_AND_MORE,
                &[&src_str, &tgt_str, &props_joined, &more_count],
            );
            Diagnostic::error(
                file_name,
                start,
                length,
                message,
                diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE_AND_MORE,
            )
        } else {
            let message = format_message(
                diagnostic_messages::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE,
                &[&src_str, &tgt_str, &props_joined],
            );
            Diagnostic::error(
                file_name,
                start,
                length,
                message,
                diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE,
            )
        }
    }

    fn missing_property_name_for_display(
        &mut self,
        property_name: tsz_common::interner::Atom,
        target: TypeId,
    ) -> String {
        if let Some(display) = self.enum_mapped_property_name_for_display(property_name, target) {
            return display;
        }
        self.ctx.types.resolve_atom_ref(property_name).to_string()
    }

    fn enum_mapped_property_name_for_display(
        &mut self,
        property_name: tsz_common::interner::Atom,
        target: TypeId,
    ) -> Option<String> {
        let property_key = self.ctx.types.resolve_atom_ref(property_name).to_string();
        let (_, args) = crate::query_boundaries::common::application_info(self.ctx.types, target)?;

        args.into_iter()
            .find_map(|arg| self.enum_key_property_name_for_display(&property_key, arg))
    }

    fn enum_key_property_name_for_display(
        &mut self,
        property_key: &str,
        key_type: TypeId,
    ) -> Option<String> {
        if let Some(members) =
            crate::query_boundaries::common::union_members(self.ctx.types, key_type)
        {
            return members
                .iter()
                .find_map(|&member| self.enum_key_property_name_for_display(property_key, member));
        }

        let def_id = crate::query_boundaries::common::enum_def_id(self.ctx.types, key_type)
            .or_else(|| crate::query_boundaries::common::lazy_def_id(self.ctx.types, key_type))?;
        let def = self.ctx.definition_store.get(def_id)?;
        if def.kind == tsz_solver::def::DefKind::Enum && !def.enum_members.is_empty() {
            return self.enum_property_name_from_parent_def(property_key, &def);
        }

        self.enum_property_name_from_member_type(property_key, key_type, &def)
    }

    fn enum_property_name_from_parent_def(
        &mut self,
        property_key: &str,
        enum_def: &tsz_solver::def::DefinitionInfo,
    ) -> Option<String> {
        let enum_name = self.ctx.types.resolve_atom_ref(enum_def.name).to_string();
        let enum_symbol_id = tsz_binder::SymbolId(enum_def.symbol_id?);
        let enum_symbol = self.ctx.binder.get_symbol(enum_symbol_id)?;
        let exports = enum_symbol.exports.as_ref()?;

        for (member_atom, _) in &enum_def.enum_members {
            let member_name = self.ctx.types.resolve_atom_ref(*member_atom).to_string();
            let Some(member_symbol_id) = exports.get(&member_name) else {
                continue;
            };
            let Some(member_type) = self.ctx.symbol_types.get(&member_symbol_id).copied() else {
                continue;
            };
            if self.enum_member_type_matches_property_key(member_type, property_key) {
                return Some(format!("[{enum_name}.{member_name}]"));
            }
        }

        None
    }

    fn enum_property_name_from_member_type(
        &mut self,
        property_key: &str,
        member_type: TypeId,
        member_def: &tsz_solver::def::DefinitionInfo,
    ) -> Option<String> {
        if !self.enum_member_type_matches_property_key(member_type, property_key) {
            return None;
        }

        let member_symbol_id = tsz_binder::SymbolId(member_def.symbol_id?);
        let member_symbol = self.ctx.binder.get_symbol(member_symbol_id)?;
        if member_symbol.parent.is_none() {
            return None;
        }
        let enum_symbol = self.ctx.binder.get_symbol(member_symbol.parent)?;
        Some(format!(
            "[{}.{}]",
            enum_symbol.escaped_name, member_symbol.escaped_name
        ))
    }

    fn enum_member_type_matches_property_key(
        &self,
        member_type: TypeId,
        property_key: &str,
    ) -> bool {
        let value_type =
            crate::query_boundaries::common::enum_member_type(self.ctx.types, member_type)
                .unwrap_or(member_type);
        crate::query_boundaries::common::literal_value(self.ctx.types, value_type)
            .and_then(|literal| self.literal_property_key_text(literal))
            .is_some_and(|key| key == property_key)
    }

    fn literal_property_key_text(&self, literal: tsz_solver::LiteralValue) -> Option<String> {
        match literal {
            tsz_solver::LiteralValue::String(atom) | tsz_solver::LiteralValue::BigInt(atom) => {
                Some(self.ctx.types.resolve_atom_ref(atom).to_string())
            }
            tsz_solver::LiteralValue::Number(value) => {
                let value = value.0;
                if value == 0.0 {
                    Some("0".to_string())
                } else if value.is_finite() && value.fract() == 0.0 {
                    Some(format!("{value:.0}"))
                } else {
                    Some(value.to_string())
                }
            }
            tsz_solver::LiteralValue::Boolean(value) => Some(value.to_string()),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn render_property_type_mismatch(
        &mut self,
        reason: &tsz_solver::SubtypeFailureReason,
        source: TypeId,
        target: TypeId,
        idx: NodeIndex,
        depth: u32,
        start: u32,
        length: u32,
        file_name: String,
        property_name: tsz_common::interner::Atom,
        source_property_type: TypeId,
        target_property_type: TypeId,
        nested_reason: Option<&tsz_solver::SubtypeFailureReason>,
    ) -> Diagnostic {
        let target_property_type = if self.should_strip_nullish_for_property_display(target) {
            self.strip_nullish_for_assignability_display(target_property_type, source_property_type)
                .unwrap_or(target_property_type)
        } else {
            target_property_type
        };

        if depth == 0 {
            let (source_str, target_str) =
                self.format_top_level_assignability_message_types_at(source, target, idx);
            if let Some(tsz_solver::SubtypeFailureReason::LiteralTypeMismatch { .. }) =
                nested_reason
            {
                let is_typed_array_display = |display: &str| {
                    display.starts_with("Int8Array<")
                        || display.starts_with("Uint8Array<")
                        || display.starts_with("Uint8ClampedArray<")
                        || display.starts_with("Int16Array<")
                        || display.starts_with("Uint16Array<")
                        || display.starts_with("Int32Array<")
                        || display.starts_with("Uint32Array<")
                        || display.starts_with("Float32Array<")
                        || display.starts_with("Float64Array<")
                        || display.starts_with("BigInt64Array<")
                        || display.starts_with("BigUint64Array<")
                };
                if !(is_typed_array_display(&source_str) && is_typed_array_display(&target_str)) {
                    return self.render_failure_reason(
                        nested_reason.expect("checked above"),
                        source_property_type,
                        target_property_type,
                        idx,
                        depth,
                    );
                }
            }
            let base = format_message(
                diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                &[&source_str, &target_str],
            );
            let prop_name = self.ctx.types.resolve_atom_ref(property_name);
            let detail = format_message(
                diagnostic_messages::TYPES_OF_PROPERTY_ARE_INCOMPATIBLE,
                &[&prop_name],
            );
            let mut diag = Diagnostic::error(
                file_name.clone(),
                start,
                length,
                base,
                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            );
            diag.related_information.push(DiagnosticRelatedInformation {
                file: file_name,
                start,
                length,
                message_text: detail,
                category: DiagnosticCategory::Message,
                code: reason.diagnostic_code(),
            });
            return diag;
        }

        let prop_name = self.ctx.types.resolve_atom_ref(property_name);
        let message = format_message(
            diagnostic_messages::TYPES_OF_PROPERTY_ARE_INCOMPATIBLE,
            &[&prop_name],
        );
        let mut diag =
            Diagnostic::error(file_name, start, length, message, reason.diagnostic_code());

        if let Some(nested) = nested_reason
            && depth < 5
        {
            let nested_diag = self.render_failure_reason(
                nested,
                source_property_type,
                target_property_type,
                idx,
                depth + 1,
            );
            diag.related_information.push(DiagnosticRelatedInformation {
                file: nested_diag.file,
                start: nested_diag.start,
                length: nested_diag.length,
                message_text: nested_diag.message_text,
                category: DiagnosticCategory::Message,
                code: nested_diag.code,
            });
        }
        diag
    }

    #[allow(clippy::too_many_arguments)]
    fn render_optional_property_required(
        &mut self,
        _reason: &tsz_solver::SubtypeFailureReason,
        source: TypeId,
        target: TypeId,
        idx: NodeIndex,
        depth: u32,
        start: u32,
        length: u32,
        file_name: String,
        property_name: tsz_common::interner::Atom,
    ) -> Diagnostic {
        if depth == 0 {
            let (source_str, target_str) =
                self.format_top_level_assignability_message_types_at(source, target, idx);
            let base = format_message(
                diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                &[&source_str, &target_str],
            );
            let prop_name = self.ctx.types.resolve_atom_ref(property_name);
            let source_str = self.format_type_diagnostic(source);
            let target_str = self.format_type_diagnostic(target);
            let detail = format_message(
                diagnostic_messages::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE,
                &[&prop_name, &source_str, &target_str],
            );
            let mut diag = Diagnostic::error(
                file_name.clone(),
                start,
                length,
                base,
                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            );
            diag.related_information.push(DiagnosticRelatedInformation {
                file: file_name,
                start,
                length,
                message_text: detail,
                category: DiagnosticCategory::Message,
                code: diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE,
            });
            diag
        } else {
            let prop_name = self.ctx.types.resolve_atom_ref(property_name);
            let source_str = self.format_type_diagnostic(source);
            let target_str = self.format_type_diagnostic(target);
            let message = format_message(
                diagnostic_messages::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE,
                &[&prop_name, &source_str, &target_str],
            );
            Diagnostic::error(
                file_name,
                start,
                length,
                message,
                diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE,
            )
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn render_property_nominal_mismatch(
        &mut self,
        reason: &tsz_solver::SubtypeFailureReason,
        source: TypeId,
        target: TypeId,
        idx: NodeIndex,
        _depth: u32,
        start: u32,
        length: u32,
        file_name: String,
        property_name: tsz_common::interner::Atom,
    ) -> Diagnostic {
        if let Some((prop_name, owner_name, visibility)) =
            self.private_or_protected_member_missing_display(source, target, Some(property_name))
        {
            let widened_source = self.widen_type_for_display(source);
            let src_str = if source == TypeId::OBJECT {
                "{}".to_string()
            } else {
                self.format_type_diagnostic(widened_source)
            };
            let tgt_str = self.format_type_diagnostic(target);
            let message = self.private_or_protected_assignability_message(
                &src_str,
                &tgt_str,
                &prop_name,
                &owner_name,
                visibility,
                None,
            );
            return Diagnostic::error(
                file_name,
                start,
                length,
                message,
                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            );
        }

        let (source_str, target_str) =
            self.format_top_level_assignability_message_types_at(source, target, idx);
        let base = format_message(
            diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            &[&source_str, &target_str],
        );
        let mut diag = Diagnostic::error(
            file_name.clone(),
            start,
            length,
            base,
            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
        );
        if let Some(detail) = self.nominal_mismatch_detail(source, target, property_name) {
            diag.related_information.push(DiagnosticRelatedInformation {
                file: file_name,
                start,
                length,
                message_text: detail,
                category: DiagnosticCategory::Message,
                code: reason.diagnostic_code(),
            });
        }
        diag
    }

    #[allow(clippy::too_many_arguments)]
    fn render_return_type_mismatch(
        &mut self,
        reason: &tsz_solver::SubtypeFailureReason,
        source: TypeId,
        target: TypeId,
        idx: NodeIndex,
        depth: u32,
        start: u32,
        length: u32,
        file_name: String,
        source_return: TypeId,
        target_return: TypeId,
        nested_reason: Option<&tsz_solver::SubtypeFailureReason>,
    ) -> Diagnostic {
        if depth == 0 {
            let (source_str, target_str) =
                self.format_top_level_assignability_message_types_at(source, target, idx);
            let base = format_message(
                diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                &[&source_str, &target_str],
            );
            let mut diag = Diagnostic::error(
                file_name.clone(),
                start,
                length,
                base,
                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            );

            let ret_source_str = self.format_type_diagnostic(source_return);
            let ret_target_str = self.format_type_diagnostic(target_return);
            let ret_msg =
                format!("Return type '{ret_source_str}' is not assignable to '{ret_target_str}'.");
            diag.related_information.push(DiagnosticRelatedInformation {
                file: file_name,
                start,
                length,
                message_text: ret_msg,
                category: DiagnosticCategory::Message,
                code: reason.diagnostic_code(),
            });

            if let Some(nested) = nested_reason
                && depth < 5
            {
                let nested_diag = self.render_failure_reason(
                    nested,
                    source_return,
                    target_return,
                    idx,
                    depth + 1,
                );
                diag.related_information.push(DiagnosticRelatedInformation {
                    file: nested_diag.file,
                    start: nested_diag.start,
                    length: nested_diag.length,
                    message_text: nested_diag.message_text,
                    category: DiagnosticCategory::Message,
                    code: nested_diag.code,
                });
            }

            diag
        } else {
            let source_str = self.format_type_diagnostic(source_return);
            let target_str = self.format_type_diagnostic(target_return);
            let message =
                format!("Return type '{source_str}' is not assignable to '{target_str}'.");
            let mut diag =
                Diagnostic::error(file_name, start, length, message, reason.diagnostic_code());

            if let Some(nested) = nested_reason
                && depth < 5
            {
                let nested_diag = self.render_failure_reason(
                    nested,
                    source_return,
                    target_return,
                    idx,
                    depth + 1,
                );
                diag.related_information.push(DiagnosticRelatedInformation {
                    file: nested_diag.file,
                    start: nested_diag.start,
                    length: nested_diag.length,
                    message_text: nested_diag.message_text,
                    category: DiagnosticCategory::Message,
                    code: nested_diag.code,
                });
            }
            diag
        }
    }

    /// Locate the span of an excess property name within a source expression.
    ///
    /// Walks any surrounding parenthesized expression, `||`/`??`/`,` combinator,
    /// or conditional `? :` to reach the object literal that declares the
    /// property and returns the span of that property's name token. tsc
    /// underlines the property (e.g. `b` in `{ a: '', b: 123 } || ...`) rather
    /// than the containing literal's `{`; preserving that anchor is required
    /// for TS2353 fingerprint parity.
    pub(crate) fn find_excess_property_anchor(
        &self,
        idx: NodeIndex,
        property_name: tsz_common::interner::Atom,
    ) -> Option<(u32, u32)> {
        use tsz_parser::parser::syntax_kind_ext;
        const MAX_DEPTH: u32 = 8;
        // Stack holds (node, depth). Popping left-before-right requires pushing
        // right first (LIFO) so the leftmost operand is inspected first — matches
        // tsc's left-to-right property enumeration for `||` / `??` / `,`.
        let mut stack: Vec<(NodeIndex, u32)> = vec![(idx, 0)];
        while let Some((current, depth)) = stack.pop() {
            if depth > MAX_DEPTH {
                continue;
            }
            let Some(node) = self.ctx.arena.get(current) else {
                continue;
            };
            if node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                if let Some(span) =
                    self.excess_property_name_span_in_literal(current, property_name)
                {
                    return Some(span);
                }
                continue;
            }
            if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
                && let Some(paren) = self.ctx.arena.get_parenthesized(node)
            {
                stack.push((paren.expression, depth + 1));
                continue;
            }
            if node.kind == syntax_kind_ext::BINARY_EXPRESSION
                && let Some(bin) = self.ctx.arena.get_binary_expr(node)
            {
                stack.push((bin.right, depth + 1));
                stack.push((bin.left, depth + 1));
                continue;
            }
            if node.kind == syntax_kind_ext::CONDITIONAL_EXPRESSION
                && let Some(cond) = self.ctx.arena.get_conditional_expr(node)
            {
                stack.push((cond.when_false, depth + 1));
                stack.push((cond.when_true, depth + 1));
                continue;
            }
        }
        None
    }

    fn excess_property_name_span_in_literal(
        &self,
        literal_idx: NodeIndex,
        property_name: tsz_common::interner::Atom,
    ) -> Option<(u32, u32)> {
        use tsz_parser::parser::syntax_kind_ext;
        let node = self.ctx.arena.get(literal_idx)?;
        let literal = self.ctx.arena.get_literal_expr(node)?;
        for &elem in &literal.elements.nodes {
            let elem_node = self.ctx.arena.get(elem)?;
            if elem_node.kind == syntax_kind_ext::PROPERTY_ASSIGNMENT
                && let Some(prop) = self.ctx.arena.get_property_assignment(elem_node)
                && self.property_name_matches_atom(prop.name, property_name)
            {
                return self.property_name_span(prop.name);
            }
            if elem_node.kind == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT
                && let Some(prop) = self.ctx.arena.get_shorthand_property(elem_node)
                && self.property_name_matches_atom(prop.name, property_name)
            {
                return self.property_name_span(prop.name);
            }
            if elem_node.kind == syntax_kind_ext::METHOD_DECLARATION
                && let Some(method) = self.ctx.arena.get_method_decl(elem_node)
                && self.property_name_matches_atom(method.name, property_name)
            {
                return self.property_name_span(method.name);
            }
        }
        None
    }

    fn property_name_matches_atom(
        &self,
        name_idx: NodeIndex,
        target: tsz_common::interner::Atom,
    ) -> bool {
        let Some(name_node) = self.ctx.arena.get(name_idx) else {
            return false;
        };
        let resolved = self.ctx.types.resolve_atom_ref(target);
        let target_str: &str = &resolved;
        if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
            return ident.escaped_text.as_str() == target_str;
        }
        if let Some(literal) = self.ctx.arena.get_literal(name_node) {
            return literal.text.as_str() == target_str;
        }
        false
    }

    fn property_name_span(&self, name_idx: NodeIndex) -> Option<(u32, u32)> {
        let node = self.ctx.arena.get(name_idx)?;
        Some((node.pos, node.end.saturating_sub(node.pos)))
    }
}
