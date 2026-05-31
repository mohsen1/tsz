//! Helper predicates for class and interface compatibility checks.

use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn function_type_uses_this_only_in_parameters(&self, type_id: TypeId) -> bool {
        let Some(shape) =
            crate::query_boundaries::common::function_shape_for_type(self.ctx.types, type_id)
        else {
            return false;
        };

        if self.is_direct_this_type(shape.return_type) {
            return false;
        }

        shape.params.iter().any(|param| {
            crate::query_boundaries::common::contains_this_type(self.ctx.types, param.type_id)
        })
    }
}
