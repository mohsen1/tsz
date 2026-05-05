//! Infer-result conditional helpers for TS2344 constraint validation.

use crate::query_boundaries::checkers::generic as query;
use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(super) fn type_arg_evaluates_to_infer_result_conditional(
        &mut self,
        type_arg: TypeId,
    ) -> bool {
        if self.type_is_infer_result_conditional(type_arg) {
            return true;
        }

        let evaluated = self.evaluate_type_for_assignability(type_arg);
        evaluated != type_arg && self.type_is_infer_result_conditional(evaluated)
    }

    pub(super) fn infer_result_satisfies_via_check_constraint(
        &mut self,
        type_arg: TypeId,
        cond_check: TypeId,
        inst_constraint: TypeId,
    ) -> bool {
        let db = self.ctx.types.as_type_database();
        let Some(check_name) = query::type_parameter_name(db, cond_check) else {
            return false;
        };
        let check_constraint = query::get_type_parameter_constraint(db, cond_check)
            .unwrap_or_else(|| query::base_constraint_of_type(db, cond_check));
        if check_constraint == TypeId::UNKNOWN
            || check_constraint == TypeId::ANY
            || check_constraint == cond_check
        {
            return false;
        }

        let mut substitution = crate::query_boundaries::common::TypeSubstitution::new();
        substitution.insert(check_name, check_constraint);
        let restricted = crate::query_boundaries::common::instantiate_type(
            self.ctx.types,
            type_arg,
            &substitution,
        );
        let restricted = self.resolve_lazy_type(restricted);
        let restricted_evaluated = self.evaluate_type_for_assignability(restricted);
        self.is_assignable_to(restricted_evaluated, inst_constraint)
            || self.is_assignable_to(restricted, inst_constraint)
    }

    pub(super) fn infer_result_satisfies_via_application_arg_constraints(
        &mut self,
        type_arg: TypeId,
        inst_constraint: TypeId,
    ) -> bool {
        let Some((_base, args)) =
            query::application_base_and_args(self.ctx.types.as_type_database(), type_arg)
        else {
            return false;
        };

        let mut substitution = crate::query_boundaries::common::TypeSubstitution::new();
        for arg in args {
            let db = self.ctx.types.as_type_database();
            let Some(arg_name) = query::type_parameter_name(db, arg) else {
                continue;
            };
            let arg_constraint = query::get_type_parameter_constraint(db, arg)
                .unwrap_or_else(|| query::base_constraint_of_type(db, arg));
            if arg_constraint != TypeId::UNKNOWN
                && arg_constraint != TypeId::ANY
                && arg_constraint != arg
            {
                substitution.insert(arg_name, arg_constraint);
            }
        }

        if substitution.is_empty() {
            return false;
        }

        let restricted = crate::query_boundaries::common::instantiate_type(
            self.ctx.types,
            type_arg,
            &substitution,
        );
        let restricted = self.resolve_lazy_type(restricted);
        let restricted_evaluated = self.evaluate_type_for_assignability(restricted);
        self.is_assignable_to(restricted_evaluated, inst_constraint)
            || self.is_assignable_to(restricted, inst_constraint)
    }

    fn type_is_infer_result_conditional(&self, type_id: TypeId) -> bool {
        let db = self.ctx.types.as_type_database();
        query::full_conditional_type_components(db, type_id).is_some_and(
            |(_cond_check, _cond_extends, cond_true, cond_false)| {
                cond_false == TypeId::NEVER && query::is_infer_type(db, cond_true)
            },
        )
    }
}
