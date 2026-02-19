use tsz_solver::TypeId;
use tsz_solver::visitor::contains_type_matching;

use crate::state::CheckerState;

impl<'a> CheckerState<'a> {
    pub(crate) fn check_circular_type_arguments(
        &self,
        symbol_type: TypeId,
        type_arguments: &[TypeId],
    ) -> bool {
        for &type_argument in type_arguments {
            if type_argument == symbol_type {
                return true;
            }
        }
        false
    }
}
