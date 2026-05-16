use crate::diagnostics::{DiagnosticCategory, DiagnosticRelatedInformation, diagnostic_codes};
use crate::error_reporter::assignability::is_callable_application_type;
use crate::error_reporter::fingerprint_policy::DiagnosticAnchorKind;
use crate::query_boundaries::relation_types::RelationFailure;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Diagnose an assignment failure using failure evidence already collected
    /// by `execute_relation_request`.
    pub(crate) fn diagnose_assignment_failure_with_relation_outcome(
        &mut self,
        source: TypeId,
        target: TypeId,
        idx: NodeIndex,
        outcome: &crate::query_boundaries::assignability::RelationOutcome,
    ) {
        let anchor_idx =
            self.resolve_diagnostic_anchor_node(idx, DiagnosticAnchorKind::RewriteAssignment);
        if let Some(reason) = outcome.failure.as_ref() {
            self.diagnose_assignment_failure_with_reason(source, target, anchor_idx, reason);
        } else {
            self.diagnose_assignment_failure_with_anchor(source, target, anchor_idx);
        }
    }

    fn diagnose_assignment_failure_with_reason(
        &mut self,
        source: TypeId,
        target: TypeId,
        anchor_idx: NodeIndex,
        failure_reason: &RelationFailure,
    ) {
        if matches!(failure_reason, RelationFailure::ExcessProperty { .. }) {
            let start_idx = if let Some(node) = self.ctx.arena.get(anchor_idx) {
                if node.kind == syntax_kind_ext::RETURN_STATEMENT {
                    self.ctx
                        .arena
                        .get_return_statement(node)
                        .map(|ret| ret.expression)
                        .unwrap_or(anchor_idx)
                } else {
                    anchor_idx
                }
            } else {
                anchor_idx
            };
            if let Some(obj_idx) = self.find_rhs_object_literal(start_idx) {
                self.check_object_literal_excess_properties(source, target, obj_idx);
            }
            return;
        }
        if let RelationFailure::MissingProperty {
            property_name,
            source_type,
            target_type,
        } = failure_reason
        {
            let pn = self.ctx.types.resolve_atom_ref(*property_name);
            if pn.starts_with("[Symbol.") || pn.starts_with("__js_ctor_brand_") {
                return;
            }
            if self.missing_property_is_satisfied_by_source(
                &[source, *source_type],
                &[target, *target_type],
                *property_name,
            ) {
                return;
            }
        }
        if is_callable_application_type(self.ctx.types, source)
            && is_callable_application_type(self.ctx.types, target)
            && self.should_suppress_outer_callback_return_assignability(target, anchor_idx)
        {
            return;
        }
        if matches!(failure_reason, RelationFailure::TypeMismatch { .. }) {
            self.diagnose_assignment_failure_with_anchor(source, target, anchor_idx);
            return;
        }
        let mut diag = self.render_relation_failure(failure_reason, source, target, anchor_idx, 0);
        if let RelationFailure::PropertyNominalMismatch { property_name } = failure_reason
            && diag.related_information.is_empty()
            && let Some(detail) = self.nominal_mismatch_detail(source, target, *property_name)
        {
            diag.related_information.push(DiagnosticRelatedInformation {
                file: diag.file.clone(),
                start: diag.start,
                length: diag.length,
                message_text: detail,
                category: DiagnosticCategory::Message,
                code: tsz_solver::SubtypeFailureReason::PropertyNominalMismatch {
                    property_name: *property_name,
                }
                .diagnostic_code(),
            });
        }
        if diag.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
            && diag.related_information.is_empty()
            && let Some(detail) = self.private_brand_mismatch_error(source, target)
        {
            diag.related_information.push(DiagnosticRelatedInformation {
                file: diag.file.clone(),
                start: diag.start,
                length: diag.length,
                message_text: detail,
                category: DiagnosticCategory::Message,
                code: diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
            });
        }
        if diag.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE {
            diag.message_text = self.rewrite_declared_generic_alias_source_in_ts2322_message(
                anchor_idx,
                diag.message_text,
            );
        }
        self.ctx.push_diagnostic(diag);
    }
}
