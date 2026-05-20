use crate::diagnostics::{
    Diagnostic, DiagnosticCategory, DiagnosticRelatedInformation, diagnostic_codes,
    diagnostic_messages, format_message,
};
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    fn application_base_for_property_mismatch_display(&self, type_id: TypeId) -> Option<TypeId> {
        crate::query_boundaries::common::application_info(self.ctx.types, type_id)
            .or_else(|| {
                let alias = self.ctx.types.get_display_alias(type_id)?;
                crate::query_boundaries::common::application_info(self.ctx.types, alias)
            })
            .map(|(base, _)| base)
    }

    fn should_render_nested_application_property_mismatch(
        &self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        let Some(source_base) = self.application_base_for_property_mismatch_display(source) else {
            return false;
        };
        let Some(target_base) = self.application_base_for_property_mismatch_display(target) else {
            return false;
        };
        source_base == target_base
    }

    fn nested_reason_reuses_enclosing_application_source(
        &self,
        nested_source: TypeId,
        enclosing_source: TypeId,
    ) -> bool {
        let Some(nested_base) = self.application_base_for_property_mismatch_display(nested_source)
        else {
            return false;
        };
        let Some(enclosing_base) =
            self.application_base_for_property_mismatch_display(enclosing_source)
        else {
            return false;
        };
        nested_base == enclosing_base
    }

    const fn nested_reason_is_plain_type_mismatch(
        reason: &tsz_solver::SubtypeFailureReason,
    ) -> bool {
        matches!(
            reason,
            tsz_solver::SubtypeFailureReason::TypeMismatch { .. }
                | tsz_solver::SubtypeFailureReason::IntrinsicTypeMismatch { .. }
                | tsz_solver::SubtypeFailureReason::LiteralTypeMismatch { .. }
                | tsz_solver::SubtypeFailureReason::ErrorType { .. }
        )
    }

    pub(super) const fn nested_failure_display_types(
        reason: &tsz_solver::SubtypeFailureReason,
        fallback_source: TypeId,
        fallback_target: TypeId,
    ) -> (TypeId, TypeId) {
        match reason {
            tsz_solver::SubtypeFailureReason::MissingProperty {
                source_type,
                target_type,
                ..
            }
            | tsz_solver::SubtypeFailureReason::MissingProperties {
                source_type,
                target_type,
                ..
            }
            | tsz_solver::SubtypeFailureReason::TypeMismatch {
                source_type,
                target_type,
            }
            | tsz_solver::SubtypeFailureReason::IntrinsicTypeMismatch {
                source_type,
                target_type,
            }
            | tsz_solver::SubtypeFailureReason::LiteralTypeMismatch {
                source_type,
                target_type,
            }
            | tsz_solver::SubtypeFailureReason::ErrorType {
                source_type,
                target_type,
            } => (*source_type, *target_type),
            tsz_solver::SubtypeFailureReason::ReturnTypeMismatch {
                source_return,
                target_return,
                ..
            } => (*source_return, *target_return),
            tsz_solver::SubtypeFailureReason::TupleElementTypeMismatch {
                source_element,
                target_element,
                ..
            }
            | tsz_solver::SubtypeFailureReason::ArrayElementMismatch {
                source_element,
                target_element,
            } => (*source_element, *target_element),
            tsz_solver::SubtypeFailureReason::IndexSignatureMismatch {
                source_value_type,
                target_value_type,
                ..
            } => (*source_value_type, *target_value_type),
            _ => (fallback_source, fallback_target),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn render_property_type_mismatch(
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
            let outer_is_structural = {
                let eval_source = self.evaluate_type_for_assignability(source);
                let eval_target = self.evaluate_type_for_assignability(target);
                crate::query_boundaries::common::object_shape_for_type(self.ctx.types, eval_source)
                    .is_some()
                    || crate::query_boundaries::common::object_shape_for_type(
                        self.ctx.types,
                        eval_target,
                    )
                    .is_some()
            };
            if !outer_is_structural
                && let Some(tsz_solver::SubtypeFailureReason::LiteralTypeMismatch { .. }) =
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
            if self.should_render_nested_application_property_mismatch(source, target)
                && let Some(nested) = nested_reason
                && !Self::nested_reason_is_plain_type_mismatch(nested)
            {
                let (nested_source, nested_target) = Self::nested_failure_display_types(
                    nested,
                    source_property_type,
                    target_property_type,
                );
                if self.nested_reason_reuses_enclosing_application_source(nested_source, source) {
                    let prop_name = self.ctx.types.resolve_atom_ref(property_name);
                    let detail = format_message(
                        diagnostic_messages::TYPES_OF_PROPERTY_ARE_INCOMPATIBLE,
                        &[&prop_name],
                    );
                    let mut diag = Diagnostic::error(
                        file_name,
                        start,
                        length,
                        base,
                        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                    );
                    diag.related_information.push(DiagnosticRelatedInformation {
                        file: diag.file.clone(),
                        start,
                        length,
                        message_text: detail,
                        category: DiagnosticCategory::Message,
                        code: reason.diagnostic_code(),
                    });
                    return diag;
                }
                let nested_diag = self.render_failure_reason(
                    nested,
                    nested_source,
                    nested_target,
                    idx,
                    depth + 1,
                );
                let mut diag = Diagnostic::error(
                    file_name,
                    start,
                    length,
                    base,
                    diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                );
                diag.related_information.push(DiagnosticRelatedInformation {
                    file: nested_diag.file,
                    start: nested_diag.start,
                    length: nested_diag.length,
                    message_text: nested_diag.message_text,
                    category: DiagnosticCategory::Message,
                    code: nested_diag.code,
                });
                diag.related_information
                    .extend(nested_diag.related_information);
                return diag;
            }
            let prop_name = self.ctx.types.resolve_atom_ref(property_name);
            let detail = format_message(
                diagnostic_messages::TYPES_OF_PROPERTY_ARE_INCOMPATIBLE,
                &[&prop_name],
            );
            let mut diag = Diagnostic::error(
                file_name,
                start,
                length,
                base,
                diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            );
            diag.related_information.push(DiagnosticRelatedInformation {
                file: diag.file.clone(),
                start,
                length,
                message_text: detail,
                category: DiagnosticCategory::Message,
                code: reason.diagnostic_code(),
            });
            if let Some(nested) = nested_reason {
                let (nested_source, nested_target) = Self::nested_failure_display_types(
                    nested,
                    source_property_type,
                    target_property_type,
                );
                let nested_plain_mismatch_from_application = self
                    .should_render_nested_application_property_mismatch(source, target)
                    && Self::nested_reason_is_plain_type_mismatch(nested);
                if !nested_plain_mismatch_from_application
                    && !self
                        .nested_reason_reuses_enclosing_application_source(nested_source, source)
                {
                    let nested_diag = self.render_failure_reason(
                        nested,
                        nested_source,
                        nested_target,
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
                    diag.related_information
                        .extend(nested_diag.related_information);
                }
            }
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
            let (nested_source, nested_target) = Self::nested_failure_display_types(
                nested,
                source_property_type,
                target_property_type,
            );
            let nested_diag =
                self.render_failure_reason(nested, nested_source, nested_target, idx, depth + 1);
            diag.related_information.push(DiagnosticRelatedInformation {
                file: nested_diag.file,
                start: nested_diag.start,
                length: nested_diag.length,
                message_text: nested_diag.message_text,
                category: DiagnosticCategory::Message,
                code: nested_diag.code,
            });
            diag.related_information
                .extend(nested_diag.related_information);
        }
        diag
    }
}
