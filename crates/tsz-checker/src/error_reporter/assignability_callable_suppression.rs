use crate::error_reporter::assignability_type_helpers::is_function_type_display;
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
        let display_src = self.format_type_diagnostic(source_type);

        let source_has_call =
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
                    source_type_eval,
                )
                || is_function_type_display(&display_src);

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
            crate::query_boundaries::common::has_call_signatures(self.ctx.types, target)
                || crate::query_boundaries::common::has_call_signatures(
                    self.ctx.types,
                    target_eval,
                );

        source_has_call && !target_has_call && !source_has_construct
    }
}
