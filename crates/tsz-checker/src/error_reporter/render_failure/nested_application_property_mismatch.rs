use crate::diagnostics::{
    Diagnostic, DiagnosticCategory, DiagnosticRelatedInformation, diagnostic_codes,
    diagnostic_messages, format_message,
};
use crate::error_reporter::render_failure::RenderContext;
use crate::state::CheckerState;
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

    fn is_typed_array_application_property_mismatch_display(&self, type_id: TypeId) -> bool {
        let Some(base) = self.application_base_for_property_mismatch_display(type_id) else {
            return false;
        };
        crate::query_boundaries::definition_identity::type_has_well_known_typed_array_name(
            self.ctx.types,
            &self.ctx.definition_store,
            base,
        )
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

    pub(super) fn render_property_type_mismatch(
        &mut self,
        reason: &tsz_solver::SubtypeFailureReason,
        ctx: &RenderContext,
        property_name: tsz_common::interner::Atom,
        source_property_type: TypeId,
        target_property_type: TypeId,
        nested_reason: Option<&tsz_solver::SubtypeFailureReason>,
    ) -> Diagnostic {
        let source = ctx.source;
        let target = ctx.target;
        let idx = ctx.idx;
        let depth = ctx.depth;
        let start = ctx.start;
        let length = ctx.length;
        let file_name = ctx.file_name.clone();
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
                && !(self.is_typed_array_application_property_mismatch_display(source)
                    && self.is_typed_array_application_property_mismatch_display(target))
            {
                return self.render_failure_reason(
                    nested_reason.expect("checked above"),
                    source_property_type,
                    target_property_type,
                    idx,
                    depth,
                );
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
                Self::push_nested_chain(&mut diag, nested_diag);
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
                    Self::push_nested_chain(&mut diag, nested_diag);
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
            Self::push_nested_chain(&mut diag, nested_diag);
        }
        diag
    }

    /// Render a tuple element type mismatch.
    ///
    /// tsc elaborates a failing tuple element with TS2626
    /// `Type at position <index> in source is not compatible with type at
    /// position <index> in target.` (both positions are the element index for
    /// fixed tuples), nested beneath the outer
    /// `Type 'S' is not assignable to type 'T'.` line, then the inner element
    /// failure. This mirrors the chain shape of
    /// [`Self::render_property_type_mismatch`] but keyed by position instead of
    /// a property name.
    pub(super) fn render_tuple_element_type_mismatch(
        &mut self,
        ctx: &RenderContext,
        index: usize,
        source_element: TypeId,
        target_element: TypeId,
        nested_reason: Option<&tsz_solver::SubtypeFailureReason>,
    ) -> Diagnostic {
        let source = ctx.source;
        let target = ctx.target;
        let idx = ctx.idx;
        let depth = ctx.depth;
        let start = ctx.start;
        let length = ctx.length;
        let file_name = ctx.file_name.clone();
        let index_str = index.to_string();

        // TS2626: source and target positions are both the element index for a
        // fixed tuple element mismatch.
        let detail = format_message(
            diagnostic_messages::TYPE_AT_POSITION_IN_SOURCE_IS_NOT_COMPATIBLE_WITH_TYPE_AT_POSITION_IN_TARGET,
            &[&index_str, &index_str],
        );

        let mut diag = if depth == 0 {
            let (source_str, target_str) =
                self.format_top_level_assignability_message_types_at(source, target, idx);
            let base = format_message(
                diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                &[&source_str, &target_str],
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
                code: diagnostic_codes::TYPE_AT_POSITION_IN_SOURCE_IS_NOT_COMPATIBLE_WITH_TYPE_AT_POSITION_IN_TARGET,
            });
            diag
        } else {
            Diagnostic::error(
                file_name,
                start,
                length,
                detail,
                diagnostic_codes::TYPE_AT_POSITION_IN_SOURCE_IS_NOT_COMPATIBLE_WITH_TYPE_AT_POSITION_IN_TARGET,
            )
        };

        if depth < 5 {
            self.push_tuple_element_inner_failure(
                &mut diag,
                idx,
                depth,
                source_element,
                target_element,
                nested_reason,
            );
        }

        diag
    }

    /// Append the inner element failure line beneath a tuple element mismatch.
    ///
    /// Uses the structured `nested_reason` when present so deeply nested element
    /// failures keep elaborating; otherwise falls back to a direct
    /// `Type 'S' is not assignable to type 'T'.` line for the element pair so the
    /// chain never stops at the bare `Types of property` header.
    fn push_tuple_element_inner_failure(
        &mut self,
        diag: &mut Diagnostic,
        idx: tsz_parser::parser::NodeIndex,
        depth: u32,
        source_element: TypeId,
        target_element: TypeId,
        nested_reason: Option<&tsz_solver::SubtypeFailureReason>,
    ) {
        if let Some(nested) = nested_reason {
            let (nested_source, nested_target) =
                Self::nested_failure_display_types(nested, source_element, target_element);
            let nested_diag =
                self.render_failure_reason(nested, nested_source, nested_target, idx, depth + 1);
            Self::push_nested_chain(diag, nested_diag);
        } else {
            let source_str = self.format_type_diagnostic(source_element);
            let target_str = self.format_type_diagnostic(target_element);
            let message = format_message(
                diagnostic_messages::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
                &[&source_str, &target_str],
            );
            diag.related_information.push(DiagnosticRelatedInformation {
                file: diag.file.clone(),
                start: diag.start,
                length: diag.length,
                message_text: message,
                category: DiagnosticCategory::Message,
                code: diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            });
        }
    }

    /// Flatten a fully-rendered nested failure into `diag`'s related
    /// information: the nested diagnostic's own message line followed by its
    /// related chain. This is the shared shape every elaboration step uses to
    /// append a child reason.
    fn push_nested_chain(diag: &mut Diagnostic, nested_diag: Diagnostic) {
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
