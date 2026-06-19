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
                && self.bare_type_param_base_inst_constraint_relation(
                    type_arg,
                    base,
                    inst_constraint,
                ))
    }

    fn bare_type_param_base_inst_constraint_relation(
        &mut self,
        type_arg: TypeId,
        base: TypeId,
        inst_constraint: TypeId,
    ) -> bool {
        let cache_key = (
            type_arg,
            base,
            inst_constraint,
            self.ctx.pack_relation_flags(),
            self.ctx.sound_mode(),
        );
        if let Some(&cached) = self
            .ctx
            .type_reference_validation_caches
            .bare_param_base_inst_constraint
            .get(&cache_key)
        {
            return cached;
        }

        let lazy_failures_at_entry = crate::query_boundaries::common::lazy_resolve_failure_count();
        let outcome = self.type_arg_constraint_relation_outcome(base, inst_constraint);
        let result = outcome.related;
        if !outcome.depth_exceeded
            && !outcome.iteration_exceeded
            && crate::query_boundaries::common::lazy_resolve_failure_count()
                == lazy_failures_at_entry
            && !self.ctx.types.is_evaluation_fuel_exhausted()
        {
            self.ctx
                .type_reference_validation_caches
                .bare_param_base_inst_constraint
                .insert(cache_key, result);
        }
        result
    }
}
