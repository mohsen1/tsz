use crate::query_boundaries::diagnostics as diagnostic_query;
use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(super) fn should_suppress_missing_property_for_callable_source(
        &mut self,
        source: TypeId,
        source_type: TypeId,
        target: TypeId,
    ) -> bool {
        let source_eval = self.evaluate_type_with_env(source);
        let source_type_eval = self.evaluate_type_with_env(source_type);
        let target_eval = self.evaluate_type_with_env(target);

        let source_has_call =
            diagnostic_query::has_call_signatures_or_callable_application(self.ctx.types, source)
                || diagnostic_query::has_call_signatures_or_callable_application(
                    self.ctx.types,
                    source_eval,
                )
                || diagnostic_query::has_call_signatures_or_callable_application(
                    self.ctx.types,
                    source_type,
                )
                || diagnostic_query::has_call_signatures_or_callable_application(
                    self.ctx.types,
                    source_type_eval,
                );

        let source_has_construct =
            crate::query_boundaries::common::has_construct_signatures(self.ctx.types, source)
                || crate::query_boundaries::common::has_construct_signatures(
                    self.ctx.types,
                    source_eval,
                )
                || crate::query_boundaries::common::has_construct_signatures(
                    self.ctx.types,
                    source_type,
                )
                || crate::query_boundaries::common::has_construct_signatures(
                    self.ctx.types,
                    source_type_eval,
                );

        let target_has_call =
            diagnostic_query::has_call_signatures_or_callable_application(self.ctx.types, target)
                || diagnostic_query::has_call_signatures_or_callable_application(
                    self.ctx.types,
                    target_eval,
                );

        source_has_call && !target_has_call && !source_has_construct
    }
}
