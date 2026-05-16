use crate::query_boundaries::checkers::generic as query;
use crate::state::CheckerState;
use rustc_hash::FxHashSet;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn conditional_result_branches_satisfy_constraint(
        &mut self,
        type_arg: TypeId,
        constraint: TypeId,
    ) -> bool {
        if matches!(constraint, TypeId::ANY | TypeId::UNKNOWN)
            || query::contains_free_type_parameters(self.ctx.types, constraint)
        {
            return false;
        }

        let components =
            query::full_conditional_type_components(self.ctx.types.as_type_database(), type_arg)
                .or_else(|| self.type_alias_application_conditional_components(type_arg))
                .or_else(|| {
                    let type_arg_evaluated = self.evaluate_type_for_assignability(type_arg);
                    (type_arg_evaluated != type_arg).then(|| {
                        query::full_conditional_type_components(
                            self.ctx.types.as_type_database(),
                            type_arg_evaluated,
                        )
                    })?
                });
        let Some((_check_type, _extends_type, true_type, false_type)) = components else {
            return false;
        };
        let db = self.ctx.types.as_type_database();
        if [true_type, false_type]
            .into_iter()
            .any(|branch| branch != TypeId::NEVER && query::is_infer_type(db, branch))
        {
            return false;
        }

        [true_type, false_type].into_iter().all(|branch| {
            if branch == TypeId::NEVER {
                return true;
            }
            let branch = self.resolve_lazy_type(branch);
            let branch_evaluated = self.evaluate_type_for_assignability(branch);
            self.is_assignable_to(branch, constraint)
                || self.is_assignable_to(branch_evaluated, constraint)
        })
    }

    fn type_alias_application_conditional_components(
        &mut self,
        mut type_arg: TypeId,
    ) -> Option<(TypeId, TypeId, TypeId, TypeId)> {
        let mut seen = FxHashSet::default();
        for _ in 0..8 {
            if !seen.insert(type_arg) {
                return None;
            }
            if let Some(components) =
                query::full_conditional_type_components(self.ctx.types.as_type_database(), type_arg)
            {
                let (_check_type, _extends_type, true_type, false_type) = components;
                let branch_is_simple = |branch| {
                    branch == TypeId::NEVER
                        || (!query::contains_free_type_parameters(self.ctx.types, branch)
                            && !query::is_infer_type(self.ctx.types.as_type_database(), branch))
                };
                if branch_is_simple(true_type) && branch_is_simple(false_type) {
                    return Some(components);
                }
                return None;
            }

            let app = crate::query_boundaries::common::type_application(self.ctx.types, type_arg)?;
            let def_id = crate::query_boundaries::common::lazy_def_id(self.ctx.types, app.base)?;
            let def = self.ctx.definition_store.get(def_id)?;
            if def.kind != tsz_solver::def::DefKind::TypeAlias
                || def.type_params.len() != app.args.len()
            {
                return None;
            }
            let body = def.body?;
            let subst = crate::query_boundaries::common::TypeSubstitution::from_args(
                self.ctx.types,
                &def.type_params,
                &app.args,
            );
            let instantiated =
                crate::query_boundaries::common::instantiate_type(self.ctx.types, body, &subst);
            if instantiated == type_arg {
                return None;
            }
            type_arg = instantiated;
        }
        None
    }

    pub(crate) fn type_alias_application_infer_result_conditional_components(
        &mut self,
        mut type_arg: TypeId,
    ) -> Option<(TypeId, TypeId, TypeId, TypeId)> {
        let mut seen = FxHashSet::default();
        for _ in 0..8 {
            if !seen.insert(type_arg) {
                return None;
            }
            if let Some(components) =
                query::full_conditional_type_components(self.ctx.types.as_type_database(), type_arg)
            {
                let (_check_type, _extends_type, true_type, false_type) = components;
                return (false_type == TypeId::NEVER
                    && query::is_infer_type(self.ctx.types.as_type_database(), true_type))
                .then_some(components);
            }

            let app = crate::query_boundaries::common::type_application(self.ctx.types, type_arg)?;
            let def_id = crate::query_boundaries::common::lazy_def_id(self.ctx.types, app.base)?;
            let def = self.ctx.definition_store.get(def_id)?;
            if def.kind != tsz_solver::def::DefKind::TypeAlias
                || def.type_params.len() != app.args.len()
            {
                return None;
            }
            let body = def.body?;
            let subst = crate::query_boundaries::common::TypeSubstitution::from_args(
                self.ctx.types,
                &def.type_params,
                &app.args,
            );
            let instantiated =
                crate::query_boundaries::common::instantiate_type(self.ctx.types, body, &subst);
            if instantiated == type_arg {
                return None;
            }
            type_arg = instantiated;
        }
        None
    }

    pub(crate) fn resolve_record_alias_type_for_indexed_access_value(
        &mut self,
        object_type: TypeId,
    ) -> Option<TypeId> {
        let app = crate::query_boundaries::common::type_application(self.ctx.types, object_type)?;
        let def_id = crate::query_boundaries::common::lazy_def_id(self.ctx.types, app.base)?;
        let def = self.ctx.definition_store.get(def_id)?;
        if def.kind != tsz_solver::def::DefKind::TypeAlias {
            return None;
        }
        if self.ctx.types.resolve_atom(def.name) != "Record" {
            return None;
        }
        if def.type_params.len() != app.args.len() || def.type_params.is_empty() {
            return None;
        }
        let body = def.body?;
        let subst = crate::query_boundaries::common::TypeSubstitution::from_args(
            self.ctx.types,
            &def.type_params,
            &app.args,
        );
        let instantiated =
            crate::query_boundaries::common::instantiate_type(self.ctx.types, body, &subst);
        let evaluated = self.evaluate_type_for_assignability(instantiated);
        Some(self.resolve_lazy_type(evaluated))
    }

    pub(crate) fn type_alias_application_filters_to_constraint(
        &mut self,
        mut type_arg: TypeId,
        constraint: TypeId,
    ) -> bool {
        for _ in 0..8 {
            if let Some((check, extends_type, true_type, false_type)) =
                query::full_conditional_type_components(self.ctx.types.as_type_database(), type_arg)
            {
                if false_type != TypeId::NEVER {
                    return false;
                }
                if query::is_infer_type(self.ctx.types.as_type_database(), true_type) {
                    return false;
                }

                let true_resolved = self.resolve_lazy_type(true_type);
                let true_evaluated = self.evaluate_type_for_assignability(true_resolved);
                let constraint_evaluated = self.evaluate_type_for_assignability(constraint);
                if self.is_assignable_to(true_evaluated, constraint_evaluated)
                    || self.is_assignable_to(true_resolved, constraint)
                {
                    return true;
                }

                if true_type != check {
                    return false;
                }
                let extends_resolved = self.resolve_lazy_type(extends_type);
                let extends_evaluated = self.evaluate_type_for_assignability(extends_resolved);
                return self.is_assignable_to(extends_evaluated, constraint_evaluated)
                    || self.is_assignable_to(extends_resolved, constraint);
            }

            let Some(app) =
                crate::query_boundaries::common::type_application(self.ctx.types, type_arg)
            else {
                return false;
            };
            let Some(def_id) =
                crate::query_boundaries::common::lazy_def_id(self.ctx.types, app.base)
            else {
                return false;
            };
            let Some(def) = self.ctx.definition_store.get(def_id) else {
                return false;
            };
            if def.kind != tsz_solver::def::DefKind::TypeAlias {
                return false;
            }
            let Some(body) = def.body else {
                return false;
            };
            if def.type_params.len() != app.args.len() {
                return false;
            }
            let subst = crate::query_boundaries::common::TypeSubstitution::from_args(
                self.ctx.types,
                &def.type_params,
                &app.args,
            );
            type_arg =
                crate::query_boundaries::common::instantiate_type(self.ctx.types, body, &subst);
            type_arg = self.resolve_lazy_type(type_arg);
        }
        false
    }
}
