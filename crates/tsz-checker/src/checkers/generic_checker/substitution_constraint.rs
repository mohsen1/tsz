//! `TS2344` handling for substitution type arguments.
//!
//! A substitution type argument is produced by conditional-flow narrowing of a
//! conditional's check variable inside its true branch (see
//! `CheckerState::apply_conditional_flow_to_type_arg`). Such an argument is
//! decided directly by the assignability relation, mirroring `tsc`'s
//! `isTypeAssignableTo(typeArgument, constraint)`: the substitution relates
//! through its intersection `base & constraint`.

use crate::query_boundaries::checkers::generic::is_substitution_type;
use crate::query_boundaries::common::TypeSubstitution;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl CheckerState<'_> {
    /// Decide a type argument under conditional true-branch flow constraints,
    /// returning `true` when the argument was flow-sensitive and fully handled.
    pub(super) fn conditional_flow_type_arg_constraint_handled(
        &mut self,
        type_arg: TypeId,
        constraint: TypeId,
        type_arg_subst: &TypeSubstitution,
        arg_idx: NodeIndex,
    ) -> bool {
        let flow_type_arg = self.apply_conditional_flow_to_type_arg(arg_idx, type_arg);
        if flow_type_arg == type_arg {
            return false;
        }

        let constraint_resolved = self.resolve_lazy_type(constraint);
        let inst_constraint =
            self.instantiate_constraint_with_subst(constraint_resolved, type_arg_subst);
        if matches!(
            inst_constraint,
            TypeId::ANY | TypeId::UNKNOWN | TypeId::ERROR
        ) || self
            .type_arg_constraint_relation_outcome(flow_type_arg, inst_constraint)
            .related
        {
            return true;
        }

        let evaluated_flow_arg = self.evaluate_type_for_assignability(flow_type_arg);
        if evaluated_flow_arg != flow_type_arg
            && self
                .type_arg_constraint_relation_outcome(evaluated_flow_arg, inst_constraint)
                .related
        {
            return true;
        }

        // The narrowed argument is an indexed access whose index the conditional
        // narrowed to a substitution, e.g. `k extends string ? Wrap<T[k]> : never`
        // narrows `T[k]` to `T[Substitution(k, k & string)]`. The evaluator does
        // not reduce a substitution-indexed access into a bare type parameter
        // (it collapses to `undefined`), so the relations above cannot see through
        // it. Reduce the UN-narrowed access `T[k]` through the object's constraint
        // — the same base-constraint reduction the non-narrowed mainline
        // (`validate_type_args_against_params`) applies — and accept when its value
        // type satisfies the constraint. Narrowing the index only restricts the
        // key, so the un-narrowed value type is a superset of the narrowed one; if
        // the superset satisfies, the narrowed subset does too (a sound accept),
        // while a genuinely failing value type such as a `string` index signature
        // still fails the relation below. This keeps parity with tsc, which
        // relates the narrowed `T[k & string]` through the object's index
        // signature exactly as it does the plain `T[k]`. Degenerate reductions
        // (`undefined`/`null`/`never`/`void`) are artifacts of incomplete
        // evaluation and are discarded, matching the mainline's filter.
        let reduced_base = self.constraint_check_base_type(type_arg);
        if reduced_base != type_arg
            && !matches!(
                reduced_base,
                TypeId::UNKNOWN | TypeId::UNDEFINED | TypeId::NULL | TypeId::NEVER | TypeId::VOID
            )
        {
            let reduced_base = self.resolve_lazy_members_in_union(reduced_base);
            let reduced_base = self.evaluate_type_for_assignability(reduced_base);
            if self
                .type_arg_constraint_relation_outcome(reduced_base, inst_constraint)
                .related
                || self.base_union_members_satisfy_constraint(reduced_base, inst_constraint)
            {
                return true;
            }
        }

        self.error_type_constraint_not_satisfied(type_arg, inst_constraint, arg_idx);
        true
    }

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
        if !is_substitution_type(self.ctx.types.as_type_database(), type_arg) {
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
