//! Checker-owned type display roles for diagnostic rendering.
//!
//! This adapter keeps checker context out of the solver formatter while making
//! diagnostic display intent explicit at emission sites. Each role delegates to
//! the existing specialized helper for that surface.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(dead_code)]
pub(in crate::error_reporter) enum DiagnosticTypeDisplayRole {
    DefaultDiagnostic,
    WidenedDiagnostic,
    FlattenedDiagnostic,
    Assignability,
    AssignmentSource {
        target: TypeId,
        anchor_idx: NodeIndex,
    },
    AssignmentTarget {
        source: TypeId,
        anchor_idx: NodeIndex,
    },
    CallArgument {
        parameter: TypeId,
        argument_idx: NodeIndex,
    },
    CallParameter {
        argument: TypeId,
        argument_idx: NodeIndex,
    },
    PropertyReceiver,
}

impl<'a> CheckerState<'a> {
    pub(in crate::error_reporter) fn format_type_for_diagnostic_role(
        &mut self,
        ty: TypeId,
        role: DiagnosticTypeDisplayRole,
    ) -> String {
        match role {
            DiagnosticTypeDisplayRole::DefaultDiagnostic => self.format_type_diagnostic(ty),
            DiagnosticTypeDisplayRole::WidenedDiagnostic => self.format_type_diagnostic_widened(ty),
            DiagnosticTypeDisplayRole::FlattenedDiagnostic => {
                self.format_type_diagnostic_flattened(ty)
            }
            DiagnosticTypeDisplayRole::Assignability => {
                self.format_type_for_assignability_message(ty)
            }
            DiagnosticTypeDisplayRole::AssignmentSource { target, anchor_idx } => {
                self.format_assignment_source_type_for_diagnostic(ty, target, anchor_idx)
            }
            DiagnosticTypeDisplayRole::AssignmentTarget { source, anchor_idx } => {
                self.format_assignment_target_type_for_diagnostic(ty, source, anchor_idx)
            }
            DiagnosticTypeDisplayRole::CallArgument {
                parameter,
                argument_idx,
            } => self.format_call_argument_type_for_diagnostic(ty, parameter, argument_idx),
            DiagnosticTypeDisplayRole::CallParameter {
                argument,
                argument_idx,
            } => self.format_call_parameter_type_for_diagnostic(ty, argument, argument_idx),
            DiagnosticTypeDisplayRole::PropertyReceiver => {
                self.format_property_receiver_type_for_diagnostic(ty)
            }
        }
    }
}
