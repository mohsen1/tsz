//! Conditional/indexed alias application display helpers.

use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(in crate::error_reporter) fn reduced_alias_app_display(
        &mut self,
        ty: TypeId,
    ) -> Option<String> {
        let display_alias = self.ctx.types.get_display_alias(ty);
        for candidate in [Some(ty), display_alias].into_iter().flatten() {
            if let Some(display) = self.reduced_alias_app_candidate_display(candidate) {
                return Some(display);
            }
        }
        None
    }

    fn reduced_alias_app_candidate_display(&mut self, candidate: TypeId) -> Option<String> {
        if !crate::query_boundaries::diagnostics::alias_application_body_reduces_through_conditional_or_indexed(
            self.ctx.types,
            &self.ctx.definition_store,
            candidate,
        ) {
            return None;
        }

        let evaluated = self.evaluate_type_for_assignability(candidate);
        (self.should_use_evaluated_assignability_display(candidate, evaluated)
            || crate::query_boundaries::diagnostics::evaluated_alias_application_has_concrete_display(
                self.ctx.types,
                candidate,
                evaluated,
            ))
        .then(|| self.format_type_for_assignability_message_skip_application_alias(evaluated))
    }
}
