use crate::query_boundaries::checkers::generic as query;
use crate::state::CheckerState;
use tsz_solver::{TypeId, TypeParamInfo};

impl<'a> CheckerState<'a> {
    pub(super) fn bare_type_param_base_satisfies_instantiated_constraint(
        &mut self,
        type_arg: TypeId,
        base: TypeId,
        constraint: TypeId,
        type_params: &[TypeParamInfo],
        type_args: &[TypeId],
    ) -> bool {
        if !query::is_bare_type_parameter(self.ctx.types.as_type_database(), type_arg)
            || base == TypeId::UNKNOWN
            || query::contains_free_type_parameters(self.ctx.types, base)
        {
            return false;
        }

        let constraint_resolved = self.resolve_lazy_type(constraint);
        let inst_constraint =
            self.instantiate_constraint_with_type_args(constraint_resolved, type_params, type_args);
        inst_constraint == TypeId::UNKNOWN
            || inst_constraint == TypeId::ANY
            || (!query::contains_type_parameters(self.ctx.types, inst_constraint)
                && self
                    .type_arg_constraint_relation_outcome(base, inst_constraint)
                    .related)
    }
}
