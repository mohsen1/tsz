//! Display helpers for diagnostics involving `NoInfer<T>`.

use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn noinfer_call_parameter_mismatch_display(
        &mut self,
        param_type: TypeId,
        arg_type: TypeId,
    ) -> Option<String> {
        let stripped = crate::query_boundaries::type_rewrite::strip_noinfer_wrappers(
            self.ctx.types,
            param_type,
        );
        let alias_stripped = self
            .ctx
            .types
            .get_display_alias(param_type)
            .and_then(|alias| {
                let stripped_alias = crate::query_boundaries::type_rewrite::strip_noinfer_wrappers(
                    self.ctx.types,
                    alias,
                );
                (stripped_alias != alias).then_some(stripped_alias)
            });
        if stripped == param_type && alias_stripped.is_none() {
            return None;
        }

        let display_type = if stripped != param_type {
            stripped
        } else {
            param_type
        };
        let evaluated = self.evaluate_type_with_env(display_type);
        let display_type = if evaluated != TypeId::ERROR {
            evaluated
        } else {
            display_type
        };
        let display_type = self
            .strip_nullish_for_assignability_display(display_type, arg_type)
            .unwrap_or(display_type);
        Some(self.format_type_for_assignability_message_skip_application_alias(display_type))
    }
}
