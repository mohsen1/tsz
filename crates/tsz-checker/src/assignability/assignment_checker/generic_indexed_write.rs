use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn generic_mapped_intersection_alias_write_target(
        &mut self,
        alias_object: TypeId,
        index_type: TypeId,
    ) -> bool {
        let Some((base, args)) =
            crate::query_boundaries::common::application_info(self.ctx.types, alias_object)
        else {
            return false;
        };
        if args.is_empty()
            || !self.index_refers_to_alias_type_argument(index_type, &args)
            || !crate::query_boundaries::common::contains_type_parameters(
                self.ctx.types,
                alias_object,
            )
        {
            return false;
        }

        let Some(def_id) = crate::query_boundaries::common::lazy_def_id(self.ctx.types, base)
        else {
            return false;
        };
        if !self
            .ctx
            .definition_store
            .get(def_id)
            .is_some_and(|def| def.kind == tsz_solver::def::DefKind::TypeAlias)
        {
            return false;
        }

        let evaluated = self.evaluate_type_with_env(alias_object);
        self.intersection_contains_mapped_type(evaluated)
    }

    fn index_refers_to_alias_type_argument(&mut self, index_type: TypeId, args: &[TypeId]) -> bool {
        if let Some(members) =
            crate::query_boundaries::common::intersection_members(self.ctx.types, index_type)
        {
            return members
                .iter()
                .copied()
                .any(|member| self.index_refers_to_alias_type_argument(member, args));
        }

        if let Some(keyof_inner) =
            crate::query_boundaries::common::keyof_inner_type(self.ctx.types, index_type)
        {
            return args.iter().copied().any(|arg| {
                self.same_type_param_identity(keyof_inner, arg)
                    || self.evaluate_type_with_env(keyof_inner) == self.evaluate_type_with_env(arg)
            });
        }

        if let Some(param_info) =
            crate::query_boundaries::common::type_param_info(self.ctx.types, index_type)
            && let Some(constraint) = param_info.constraint
            && let Some(keyof_inner) =
                crate::query_boundaries::common::keyof_inner_type(self.ctx.types, constraint)
        {
            return args.iter().copied().any(|arg| {
                self.same_type_param_identity(keyof_inner, arg)
                    || self.evaluate_type_with_env(keyof_inner) == self.evaluate_type_with_env(arg)
            });
        }

        false
    }

    fn intersection_contains_mapped_type(&mut self, type_id: TypeId) -> bool {
        let Some(members) =
            crate::query_boundaries::common::intersection_members(self.ctx.types, type_id)
        else {
            return false;
        };
        members
            .iter()
            .copied()
            .any(|member| self.type_is_or_resolves_to_mapped(member))
    }

    fn type_is_or_resolves_to_mapped(&mut self, type_id: TypeId) -> bool {
        if crate::query_boundaries::common::mapped_type_id(self.ctx.types, type_id).is_some() {
            return true;
        }

        let resolved = self.resolve_lazy_type(type_id);
        resolved != type_id
            && crate::query_boundaries::common::mapped_type_id(self.ctx.types, resolved).is_some()
    }
}
