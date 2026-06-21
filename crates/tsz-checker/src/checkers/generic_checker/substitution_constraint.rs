//! `TS2344` handling for substitution type arguments.
//!
//! A substitution type argument is produced by conditional-flow narrowing of a
//! conditional's check variable inside its true branch (see
//! `CheckerState::apply_conditional_flow_substitution`). Such an argument is
//! decided directly by the assignability relation, mirroring `tsc`'s
//! `isTypeAssignableTo(typeArgument, constraint)`: the substitution relates
//! through its intersection `base & constraint`.

use crate::query_boundaries::common::TypeSubstitution;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Decide a substitution type argument against `constraint`, returning
    /// `true` when the argument was a substitution and has been handled (the
    /// caller should skip the remaining heuristics for it).
    ///
    /// This is decisive — `T & string` satisfies `string`, while `T & number`
    /// does not — and must run before the composite/type-parameter-deferral
    /// heuristics, which only understand bare type parameters and would
    /// otherwise silently defer a substitution argument.
    pub(super) fn substitution_type_arg_constraint_handled(
        &mut self,
        type_arg: TypeId,
        constraint: TypeId,
        type_arg_subst: &TypeSubstitution,
        arg_idx: Option<NodeIndex>,
    ) -> bool {
        if tsz_solver::type_queries::substitution_components(
            self.ctx.types.as_type_database(),
            type_arg,
        )
        .is_none()
        {
            return false;
        }

        let constraint_resolved = self.resolve_lazy_type(constraint);
        let inst_constraint =
            self.instantiate_constraint_with_subst(constraint_resolved, type_arg_subst);
        if matches!(
            inst_constraint,
            TypeId::ANY | TypeId::UNKNOWN | TypeId::ERROR
        ) || self
            .type_arg_constraint_relation_outcome(type_arg, inst_constraint)
            .related
        {
            return true;
        }
        if let Some(arg_idx) = arg_idx {
            self.error_type_constraint_not_satisfied(type_arg, inst_constraint, arg_idx);
        }
        true
    }
}
