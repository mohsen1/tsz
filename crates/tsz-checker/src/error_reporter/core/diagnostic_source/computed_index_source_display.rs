//! Literal target source display helpers.

use super::literal_widening_helpers::literal_display_appropriate_for_undefined_null_target;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(in crate::error_reporter) fn literal_assignment_source_display_for_target(
        &mut self,
        target: TypeId,
        anchor_idx: NodeIndex,
    ) -> Option<String> {
        if self.in_arithmetic_compound_assignment_context(anchor_idx)
            || !crate::query_boundaries::common::is_template_literal_type(self.ctx.types, target)
        {
            return None;
        }
        let expr_idx = self
            .assignment_source_expression(anchor_idx)
            .or_else(|| self.direct_diagnostic_source_expression(anchor_idx))?;
        let display = self.literal_expression_display(expr_idx)?;
        literal_display_appropriate_for_undefined_null_target(self.ctx.types, target, &display)
            .then_some(display)
    }
}
