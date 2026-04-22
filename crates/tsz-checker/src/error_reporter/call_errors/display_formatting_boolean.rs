//! Boolean literal display helpers for call error diagnostics.

use crate::query_boundaries::common as query_common;
use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(in crate::error_reporter::call_errors) fn call_target_should_widen_boolean_literal_display(
        &mut self,
        param_type: TypeId,
    ) -> bool {
        let members = query_common::union_members(self.ctx.types, param_type).or_else(|| {
            query_common::union_members(
                self.ctx.types,
                self.evaluate_type_for_assignability(param_type),
            )
        });
        let Some(members) = members else {
            return false;
        };

        !members.iter().copied().any(|member| {
            let member = self.evaluate_type_for_assignability(member);
            matches!(
                member,
                TypeId::BOOLEAN | TypeId::BOOLEAN_TRUE | TypeId::BOOLEAN_FALSE
            )
        })
    }
}
