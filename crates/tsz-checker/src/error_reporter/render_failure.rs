//! Rendering of `SubtypeFailureReason` into diagnostics.
//!
//! This module contains the `render_failure_reason` method which converts
//! solver-produced failure reasons into user-facing diagnostic messages.
//! Split from `assignability.rs` for maintainability.

use crate::diagnostics::{
    Diagnostic, DiagnosticCategory, DiagnosticRelatedInformation, diagnostic_codes,
    diagnostic_messages, format_message,
};
use crate::error_reporter::fingerprint_policy::DiagnosticAnchorKind;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

use super::assignability::{
    is_builtin_wrapper_name, is_object_prototype_method, is_primitive_type_name,
};

impl<'a> CheckerState<'a> {
    /// Recursively render a `SubtypeFailureReason` into a Diagnostic.
    pub(super) fn render_failure_reason(
        &mut self,
        reason: &tsz_solver::SubtypeFailureReason,
        source: TypeId,
        target: TypeId,
        idx: NodeIndex,
        depth: u32,
    ) -> Diagnostic {
        use tsz_solver::SubtypeFailureReason;

        let (start, length) = self
            .resolve_diagnostic_anchor(idx, DiagnosticAnchorKind::Exact)
            .map(|anchor| (anchor.start, anchor.length))
            .unwrap_or_else(|| {
                // get_node_span returns (pos, end), convert to (start, length)
                // and apply the same span normalization as the primary path.
                let (pos, end) = self.get_node_span(idx).unwrap_or((0, 0));
                self.normalized_anchor_span(idx, pos, end.saturating_sub(pos))
            });
        let file_name = self.ctx.file_name.clone();

        // TS2696: When the source is the `Object` wrapper type and the failure is
        // about property-level issues (not call/construct signatures), tsc emits
        // "The 'Object' type is assignable to very few other types" instead of TS2322.
        // When the target is a callable/constructable type, tsc uses TS2322 instead.
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
                    self.format_top_level_assignability_message_types(source, target);
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
                Diagnostic::error(file_name, start, length, message, code)
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
                    self.format_top_level_assignability_message_types(source, target);
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
                        self.format_top_level_assignability_message_types(source, target);
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
                        self.format_top_level_assignability_message_types(source, target);
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
                        self.format_top_level_assignability_message_types(source, target);
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
                    let source_str =
                        self.format_assignment_source_type_for_diagnostic(source, target, idx);
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
                    let source_str =
                        self.format_assignment_source_type_for_diagnostic(source, target, idx);
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
                let (source_str, target_str) = if depth == 0 {
                    let use_structural_source_display =
                        tsz_solver::type_queries::get_enum_def_id(self.ctx.types, source).is_none();
                    (
                        if use_structural_source_display {
                            self.format_assignment_source_type_for_diagnostic(source, target, idx)
                        } else {
                            self.format_type_diagnostic(*source_type)
                        },
                        if use_structural_source_display {
                            self.format_assignability_type_for_message(target, source)
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
                // TS2820: when the source is a string literal and a union member is
                // close in spelling, emit "did you mean X?" instead of plain TS2322.
                let evaluated_target_for_suggestion = self.evaluate_type_with_env(target);
                if let Some(suggestion) = self.find_string_literal_spelling_suggestion(
                    source,
                    evaluated_target_for_suggestion,
                ) {
                    let msg = format_message(
                        diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE_DID_YOU_MEAN,
                        &[&source_str, &target_str, &suggestion],
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
                let source_str =
                    self.format_assignment_source_type_for_diagnostic(source, target, idx);
                let target_str = self.format_type_for_assignability_message(target);
                let message = format_message(
                    diagnostic_messages::TYPE_HAS_NO_PROPERTIES_IN_COMMON_WITH_TYPE,
                    &[&source_str, &target_str],
                );
                Diagnostic::error(
                    file_name,
                    start,
                    length,
                    message,
                    diagnostic_codes::TYPE_HAS_NO_PROPERTIES_IN_COMMON_WITH_TYPE,
                )
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

            _ => {
                let source_str =
                    self.format_assignment_source_type_for_diagnostic(source, target, idx);
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

    // =========================================================================
    // Per-variant render helpers
    // =========================================================================

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
        // TSC emits TS2322 (generic assignability error) instead of TS2741
        // when the source is a primitive type. Primitives can't have "missing properties".
        let display_src_str = if depth == 0 && source_type != tsz_solver::TypeId::OBJECT {
            self.format_assignment_source_type_for_diagnostic(source, target, idx)
        } else {
            self.format_type_diagnostic(source_type)
        };
        let is_source_primitive = (source_type != tsz_solver::TypeId::OBJECT
            && tsz_solver::is_primitive_type(self.ctx.types, source_type))
            || is_primitive_type_name(&display_src_str);
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
        if tsz_solver::type_queries::is_intersection_type(self.ctx.types, target_type)
            || tsz_solver::type_queries::is_intersection_type(self.ctx.types, target)
            || tsz_solver::type_queries::is_intersection_type(
                self.ctx.types,
                target_evaluated_for_intersection,
            )
        {
            let src_str = self.format_type_diagnostic(source_type);
            let tgt_str = if tsz_solver::type_queries::is_intersection_type(
                self.ctx.types,
                target_evaluated_for_intersection,
            ) {
                self.format_type_diagnostic(target_evaluated_for_intersection)
            } else if tsz_solver::type_queries::is_intersection_type(self.ctx.types, target) {
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

        // Private brand properties handling
        let prop_name = self.ctx.types.resolve_atom_ref(property_name);
        if prop_name.starts_with("__private_brand") {
            let src_str = if depth == 0 {
                self.format_assignment_source_type_for_diagnostic(source, target, idx)
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
        if tsz_solver::is_intersection_type(self.ctx.types, target_type)
            || tsz_solver::is_intersection_type(self.ctx.types, target)
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
            let src_str = self.format_assignment_source_type_for_diagnostic(source, target, idx);
            let tgt_str = self.format_assignability_type_for_message(target, source);
            let prop_list: Vec<String> = all_missing
                .iter()
                .take(4)
                .map(|name| self.ctx.types.resolve_atom_ref(*name).to_string())
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
        let (src_str, tgt_str_qualified) = if depth == 0 {
            let src = if source_type == TypeId::OBJECT {
                "{}".to_string()
            } else {
                self.format_assignment_source_type_for_diagnostic(source, target, idx)
            };
            (
                src,
                self.format_assignability_type_for_message(target, source),
            )
        } else if source_type == TypeId::OBJECT {
            ("{}".to_string(), tgt_str)
        } else {
            self.format_type_pair_diagnostic(widened_source, target)
        };
        let message = format_message(
            diagnostic_messages::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE,
            &[&prop_name, &src_str, &tgt_str_qualified],
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
        if tsz_solver::is_primitive_type(self.ctx.types, source_type) {
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
        if tsz_solver::is_intersection_type(self.ctx.types, target_type)
            || tsz_solver::is_intersection_type(self.ctx.types, target)
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

        // Filter out private brand properties and Object.prototype methods.
        let filtered_names: Vec<_> = property_names
            .iter()
            .filter(|name| {
                let s = self.ctx.types.resolve_atom_ref(**name);
                !s.starts_with("__private_brand") && !is_object_prototype_method(&s)
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

        // When filtering removed brand/prototype properties and only 1 remains, emit TS2741.
        if filtered_names.len() == 1 {
            let prop_name = self
                .ctx
                .types
                .resolve_atom_ref(filtered_names[0])
                .to_string();
            let src_str = if depth == 0 {
                if source_type == TypeId::OBJECT {
                    "{}".to_string()
                } else {
                    self.format_assignment_source_type_for_diagnostic(source, target, idx)
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
            self.format_assignment_source_type_for_diagnostic(source, target, idx)
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
            .map(|name| self.ctx.types.resolve_atom_ref(*name).to_string())
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
        if depth == 0 {
            let (source_str, target_str) =
                self.format_top_level_assignability_message_types(source, target);
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
        _idx: NodeIndex,
        depth: u32,
        start: u32,
        length: u32,
        file_name: String,
        property_name: tsz_common::interner::Atom,
    ) -> Diagnostic {
        if depth == 0 {
            let (source_str, target_str) =
                self.format_top_level_assignability_message_types(source, target);
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
        _idx: NodeIndex,
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
            self.format_top_level_assignability_message_types(source, target);
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
                self.format_top_level_assignability_message_types(source, target);
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

    #[allow(clippy::too_many_arguments)]
    fn render_type_mismatch(
        &mut self,
        _reason: &tsz_solver::SubtypeFailureReason,
        source: TypeId,
        target: TypeId,
        idx: NodeIndex,
        depth: u32,
        start: u32,
        length: u32,
        file_name: String,
    ) -> Diagnostic {
        let source_str = if depth == 0 {
            self.format_assignment_source_type_for_diagnostic(source, target, idx)
        } else {
            self.format_nested_assignment_source_type_for_diagnostic(source, target, idx)
        };
        let target_str = if depth == 0 {
            self.format_assignability_type_for_message(target, source)
        } else {
            self.format_type_for_assignability_message(target)
        };

        if depth == 0
            && (target_str == "Callable" || target_str == "Applicable")
            && !tsz_solver::is_primitive_type(self.ctx.types, source)
        {
            let prop_name = if target_str == "Callable" {
                "call"
            } else {
                "apply"
            };
            let message = format_message(
                diagnostic_messages::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE,
                &[prop_name, &source_str, &target_str],
            );
            return Diagnostic::error(
                file_name,
                start,
                length,
                message,
                diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE,
            );
        }

        if depth == 0
            && let Some((prop_name, owner_name, visibility)) =
                self.private_or_protected_member_missing_display(source, target, None)
        {
            let message = self.private_or_protected_assignability_message(
                &source_str,
                &target_str,
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

        // Skip single-missing-property lookup when the target is an intersection type.
        let target_is_intersection_for_mismatch = {
            let target_eval = self.evaluate_type_with_env(target);
            tsz_solver::type_queries::is_intersection_type(self.ctx.types, target)
                || tsz_solver::type_queries::is_intersection_type(self.ctx.types, target_eval)
        };
        if depth == 0
            && !target_is_intersection_for_mismatch
            && let Some(property_name) = self.missing_single_required_property(source, target)
        {
            let prop_name = self.ctx.types.resolve_atom_ref(property_name);
            if prop_name.starts_with("__private_brand") {
                let message = self
                    .private_or_protected_brand_backing_member_display(target, None)
                    .map(|(display_prop, owner_name, visibility)| {
                        self.private_or_protected_assignability_message(
                            &source_str,
                            &target_str,
                            &display_prop,
                            &owner_name,
                            visibility,
                            self.property_info_for_display(
                                source,
                                self.ctx.types.intern_string(&display_prop),
                            )
                            .map(|prop| prop.visibility),
                        )
                    })
                    .unwrap_or_else(|| {
                        format_message(
                            diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                            &[&source_str, &target_str],
                        )
                    });
                return Diagnostic::error(
                    file_name,
                    start,
                    length,
                    message,
                    diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                );
            }
            let message = format_message(
                diagnostic_messages::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE,
                &[&prop_name, &source_str, &target_str],
            );
            return Diagnostic::error(
                file_name,
                start,
                length,
                message,
                diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE,
            );
        }

        if depth == 0
            && !target_is_intersection_for_mismatch
            && let Some(missing_props) =
                self.missing_required_properties_from_index_signature_source(source, target)
            && missing_props.len() > 1
        {
            let evaluated_source = self.evaluate_type_for_assignability(source);
            let src_str = self.format_type_diagnostic(evaluated_source);
            let tgt_str = self.format_type_diagnostic(target);
            let prop_list: Vec<String> = missing_props
                .iter()
                .take(4)
                .map(|name| self.ctx.types.resolve_atom_ref(*name).to_string())
                .collect();
            let props_joined = prop_list.join(", ");
            let message = if missing_props.len() > 4 {
                let more_count = (missing_props.len() - 4).to_string();
                format_message(
                    diagnostic_messages::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE_AND_MORE,
                    &[&src_str, &tgt_str, &props_joined, &more_count],
                )
            } else {
                format_message(
                    diagnostic_messages::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE,
                    &[&src_str, &tgt_str, &props_joined],
                )
            };
            let code = if missing_props.len() > 4 {
                diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE_AND_MORE
            } else {
                diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE
            };
            return Diagnostic::error(file_name, start, length, message, code);
        }

        let base = format_message(
            diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            &[&source_str, &target_str],
        );

        if depth == 0 {
            let nonpublic = self.first_nonpublic_constructor_param_property(target);
            if tracing::enabled!(tracing::Level::TRACE) {
                tracing::trace!(
                    target = %target_str,
                    nonpublic = ?nonpublic,
                    "nonpublic constructor param property probe"
                );
            }
            if nonpublic.is_some() {
                return Diagnostic::error(
                    file_name,
                    start,
                    length,
                    base,
                    diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                );
            }
        }

        // TS2820: spelling suggestion for string literals
        if let Some(suggestion) = self.find_string_literal_spelling_suggestion(source, target) {
            let message = format_message(
                diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE_DID_YOU_MEAN,
                &[&source_str, &target_str, &suggestion],
            );
            return Diagnostic::error(
                file_name,
                start,
                length,
                message,
                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE_DID_YOU_MEAN,
            );
        }

        Diagnostic::error(
            file_name,
            start,
            length,
            base,
            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
        )
    }
}
