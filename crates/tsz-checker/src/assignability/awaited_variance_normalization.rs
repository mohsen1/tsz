use crate::query_boundaries::checkers::promise as query;
use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(super) fn normalize_awaited_application_args_for_variance(&mut self, ty: TypeId) -> TypeId {
        query::awaited_variance_application_with_mapped_args(self.ctx.types, ty, |arg| {
            self.evaluate_awaited_application_for_assignability(arg)
        })
        .unwrap_or(ty)
    }
}
