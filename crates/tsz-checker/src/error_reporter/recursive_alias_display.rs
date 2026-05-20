//! Small render-failure helpers split out to keep the main renderer below its
//! file-size ceiling.

use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn is_object_intrinsic_for_missing_properties(&self, type_id: TypeId) -> bool {
        crate::query_boundaries::type_checking_utilities::is_object_intrinsic_type(
            self.ctx.types,
            type_id,
        )
    }
}
