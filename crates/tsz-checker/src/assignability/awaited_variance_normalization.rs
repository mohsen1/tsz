use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(super) fn normalize_awaited_application_args_for_variance(&mut self, ty: TypeId) -> TypeId {
        let Some((base, args)) =
            crate::query_boundaries::common::application_info(self.ctx.types, ty)
        else {
            return ty;
        };

        let mut changed = false;
        let normalized_args: Vec<_> = args
            .iter()
            .copied()
            .map(|arg| {
                let normalized = self.evaluate_awaited_application_for_assignability(arg);
                changed |= normalized != arg;
                normalized
            })
            .collect();

        if changed {
            self.ctx.types.factory().application(base, normalized_args)
        } else {
            ty
        }
    }
}
