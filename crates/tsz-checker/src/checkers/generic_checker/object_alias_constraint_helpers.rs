use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(super) fn generic_alias_application_satisfies_object_constraint(
        &self,
        type_arg: TypeId,
        constraint: TypeId,
    ) -> bool {
        if constraint != TypeId::OBJECT {
            return false;
        }
        crate::query_boundaries::checkers::generic::alias_application_satisfies_object_constraint(
            self.ctx.types.as_type_database(),
            &self.ctx.definition_store,
            type_arg,
        )
    }
}
