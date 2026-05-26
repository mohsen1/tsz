use crate::query_boundaries::checkers::generic as query;
use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(super) fn generic_boolean_literal_probe_should_remain_indeterminate(
        &mut self,
        type_arg: TypeId,
        evaluated: TypeId,
        constraint: TypeId,
    ) -> bool {
        if !matches!(constraint, TypeId::BOOLEAN_TRUE | TypeId::BOOLEAN_FALSE)
            || !matches!(evaluated, TypeId::BOOLEAN_TRUE | TypeId::BOOLEAN_FALSE)
            || !query::contains_type_parameters(self.ctx.types, type_arg)
        {
            return false;
        }

        let db = self.ctx.types.as_type_database();
        let Some((_base, args)) = query::application_base_and_args(db, type_arg) else {
            return false;
        };

        args.into_iter()
            .any(|arg| self.application_alias_body_is_deferred_conditional(arg))
    }

    fn application_alias_body_is_deferred_conditional(&mut self, type_id: TypeId) -> bool {
        let Some((base, args)) =
            query::application_base_and_args(self.ctx.types.as_type_database(), type_id)
        else {
            return false;
        };
        let Some(def_id) = query::lazy_def_id(self.ctx.types.as_type_database(), base) else {
            return false;
        };
        let body_and_params = self.direct_source_alias_body_for_def(def_id).or_else(|| {
            let body = self
                .ctx
                .type_env
                .try_borrow()
                .ok()
                .and_then(|env| env.get_def(def_id))
                .or_else(|| self.ctx.definition_store.get_body(def_id))?;
            let params = self.ctx.get_def_type_params(def_id)?;
            Some((body, params))
        });
        let Some((body, params)) = body_and_params else {
            return false;
        };
        if params.len() != args.len() {
            return false;
        }
        let instantiated = crate::query_boundaries::common::instantiate_generic(
            self.ctx.types,
            body,
            &params,
            &args,
        );
        query::full_conditional_type_components(self.ctx.types.as_type_database(), instantiated)
            .is_some_and(|(check, extends, true_type, false_type)| {
                [check, extends, true_type, false_type]
                    .into_iter()
                    .any(|ty| query::contains_type_parameters(self.ctx.types, ty))
            })
    }
}
