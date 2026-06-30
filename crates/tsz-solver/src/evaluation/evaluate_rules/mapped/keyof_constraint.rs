//! `keyof` constraint reduction for mapped type iteration.

use crate::construction::TypeDatabase;
use crate::evaluation::evaluate::TypeEvaluator;
use crate::evaluation::result::TerminationKind;
use crate::recursion::RecursionResult;
use crate::relations::subtype::TypeResolver;
use crate::types::{LiteralValue, TypeData, TypeId};

/// Named step state for mapped `keyof`/constraint reduction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KeyofConstraintStepState {
    /// The current constraint entered the recursion guard and can be reduced.
    Entered,
    /// The current constraint is already active in this reduction chain.
    AlreadyVisited,
    /// The local recursion-depth guard fired.
    DepthExceeded,
    /// The local iteration budget fired.
    IterationExceeded,
    /// The shared solver stack-frame budget fired.
    SolverFrameExhausted,
}

impl KeyofConstraintStepState {
    const fn from_guard_entry(result: RecursionResult) -> Self {
        match result {
            RecursionResult::Entered => Self::Entered,
            RecursionResult::Cycle => Self::AlreadyVisited,
            RecursionResult::DepthExceeded => Self::DepthExceeded,
            RecursionResult::IterationExceeded => Self::IterationExceeded,
        }
    }

    const fn fallback_type(self, current: TypeId) -> Option<TypeId> {
        match self {
            Self::Entered => None,
            Self::AlreadyVisited
            | Self::DepthExceeded
            | Self::IterationExceeded
            | Self::SolverFrameExhausted => Some(current),
        }
    }

    const fn termination_kind(self) -> Option<TerminationKind> {
        match self {
            Self::Entered | Self::AlreadyVisited => None,
            Self::DepthExceeded => Some(TerminationKind::DepthExceeded),
            Self::IterationExceeded => Some(TerminationKind::IterationExceeded),
            Self::SolverFrameExhausted => Some(TerminationKind::SolverStackFrames),
        }
    }
}

/// Named continuation state after one mapped constraint reduction step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KeyofConstraintReductionState {
    /// The step exposed another reducible constraint form.
    Continue(TypeId),
    /// The step reached a terminal or stable type for this chain.
    Done(TypeId),
}

impl KeyofConstraintReductionState {
    fn from_evaluated_step(types: &dyn TypeDatabase, current: TypeId, step: TypeId) -> Self {
        if step != current
            && matches!(
                types.lookup(step),
                Some(
                    TypeData::Union(_)
                        | TypeData::Intersection(_)
                        | TypeData::KeyOf(_)
                        | TypeData::Conditional(_)
                        | TypeData::Lazy(_)
                        | TypeData::Application(_)
                )
            )
        {
            Self::Continue(step)
        } else {
            Self::Done(step)
        }
    }
}

impl<'a, R: TypeResolver> TypeEvaluator<'a, R> {
    /// Evaluate a keyof or constraint type for mapped type iteration.
    ///
    /// Wrapped with `stacker::maybe_grow()` to handle deeply nested union/intersection
    /// constraint chains without overflowing the default thread stack.
    ///
    /// All intermediate types in the evaluation chain remain entered in the
    /// `keyof_constraint_guard` until the chain terminates. This ensures that
    /// a cycle like `Lazy(A) → Lazy(B) → Lazy(A)` is detected when `A` is
    /// re-entered while it is still in the guard's visited set. The depth cap
    /// (`TypeEvaluation` profile: depth 100) also limits the chain length.
    pub(super) fn evaluate_keyof_or_constraint(&mut self, constraint: TypeId) -> TypeId {
        let mut current = constraint;
        let mut entered: Vec<TypeId> = Vec::new();

        let result = loop {
            match KeyofConstraintStepState::from_guard_entry(
                self.keyof_constraint_guard.enter(current),
            ) {
                KeyofConstraintStepState::Entered => entered.push(current),
                state => {
                    if let Some(kind) = state.termination_kind() {
                        self.record_request_limit_event(kind);
                    }
                    break state.fallback_type(current).unwrap_or(current);
                }
            }

            // Shared cross-operation stack-frame breaker (issue #7574): bound
            // the combined recursion even when this constraint chain re-enters
            // fresh evaluators. On exhaustion leave the current type opaque.
            let Some(step) = crate::recursion::with_solver_frame(|| {
                self.evaluate_keyof_or_constraint_inner(current)
            }) else {
                let state = KeyofConstraintStepState::SolverFrameExhausted;
                if let Some(kind) = state.termination_kind() {
                    self.record_request_limit_event(kind);
                }
                break state.fallback_type(current).unwrap_or(current);
            };

            match KeyofConstraintReductionState::from_evaluated_step(self.interner(), current, step)
            {
                KeyofConstraintReductionState::Continue(next) => {
                    current = next;
                }
                KeyofConstraintReductionState::Done(done) => break done,
            }
        };

        for &id in entered.iter().rev() {
            self.keyof_constraint_guard.leave(id);
        }
        result
    }

    fn evaluate_keyof_or_constraint_inner(&mut self, constraint: TypeId) -> TypeId {
        // PERF: Single lookup handles all cases instead of 4 separate DashMap lookups.
        let members = match self.interner().lookup(constraint) {
            Some(TypeData::Conditional(cond_id)) => {
                let cond = self.interner().get_conditional(cond_id);
                return self.evaluate_conditional(&cond);
            }
            Some(TypeData::Literal(LiteralValue::String(_))) => {
                return constraint;
            }
            Some(TypeData::KeyOf(operand)) => {
                return self.evaluate_keyof(operand);
            }
            Some(TypeData::Union(members)) => Some(members),
            _ => None,
        };

        // Union: recursively evaluate each member. This handles the distributed form
        // where `(keyof T & keyof U)` after T is inferred becomes
        // `Union(Intersection("x", keyof U), Intersection("y", keyof U))` due to
        // the interner's intersection-over-union distribution. Each Union member
        // (which may be an Intersection) gets recursively simplified.
        if let Some(members) = members {
            let member_list = self.interner().type_list(members);
            let mut evaluated_members = Vec::with_capacity(member_list.len());
            let mut any_changed = false;
            for &member in member_list.iter() {
                let evaluated = self.evaluate_keyof_or_constraint(member);
                if evaluated != member {
                    any_changed = true;
                }
                evaluated_members.push(evaluated);
            }
            if any_changed {
                return self.interner().union(evaluated_members);
            }
            return constraint;
        }

        // Intersection: evaluate each member to get its key set, then compute
        // their intersection. Handles both pre-distribution `keyof T & keyof U`
        // and post-distribution `"x" & keyof U` forms.
        if let Some(TypeData::Intersection(members)) = self.interner().lookup(constraint) {
            let member_list = self.interner().type_list(members);
            let mut key_sets = Vec::with_capacity(member_list.len());
            for &member in member_list.iter() {
                key_sets.push(self.evaluate_keyof_or_constraint(member));
            }
            if let Some(result) = self.intersect_keyof_sets(&key_sets) {
                return result;
            }
            // If intersection computation failed, fall through to general evaluation
        }

        // Evaluate the constraint to resolve type aliases (Lazy), Applications, etc.
        // For example, `type Keys = "a" | "b"; { [P in Keys]: T }` has a Lazy(DefId)
        // constraint that must be evaluated to get the concrete union `"a" | "b"`.
        self.evaluate(constraint)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::construction::TypeInterner;
    use crate::evaluation::result::Termination;
    use crate::recursion::{RecursionGuard, RecursionResult};

    fn assert_request_verdict(
        evaluator: &TypeEvaluator<'_, crate::relations::subtype::NoopResolver>,
        partial: TypeId,
        kind: TerminationKind,
    ) {
        let result = evaluator.request_result_for_test(partial);
        assert_eq!(
            result.termination(),
            Termination::Incomplete { kind, partial }
        );
        assert_eq!(result.into_type_id(), partial);
    }

    #[test]
    fn step_state_names_every_guard_fallback() {
        let current = TypeId::STRING;

        assert_eq!(
            KeyofConstraintStepState::from_guard_entry(RecursionResult::Entered)
                .fallback_type(current),
            None
        );
        assert_eq!(
            KeyofConstraintStepState::from_guard_entry(RecursionResult::Cycle)
                .fallback_type(current),
            Some(current)
        );
        assert_eq!(
            KeyofConstraintStepState::from_guard_entry(RecursionResult::DepthExceeded)
                .fallback_type(current),
            Some(current)
        );
        assert_eq!(
            KeyofConstraintStepState::from_guard_entry(RecursionResult::IterationExceeded)
                .fallback_type(current),
            Some(current)
        );
        assert_eq!(
            KeyofConstraintStepState::SolverFrameExhausted.fallback_type(current),
            Some(current)
        );
        assert_eq!(
            KeyofConstraintStepState::from_guard_entry(RecursionResult::Cycle).termination_kind(),
            None
        );
        assert_eq!(
            KeyofConstraintStepState::from_guard_entry(RecursionResult::DepthExceeded)
                .termination_kind(),
            Some(TerminationKind::DepthExceeded)
        );
        assert_eq!(
            KeyofConstraintStepState::from_guard_entry(RecursionResult::IterationExceeded)
                .termination_kind(),
            Some(TerminationKind::IterationExceeded)
        );
        assert_eq!(
            KeyofConstraintStepState::SolverFrameExhausted.termination_kind(),
            Some(TerminationKind::SolverStackFrames)
        );
    }

    #[test]
    fn reduction_state_names_continuing_shapes() {
        let interner = crate::construction::TypeInterner::new();
        let current = TypeId::STRING;
        let nested = interner.keyof(TypeId::NUMBER);
        let literal = interner.literal_string("done");

        assert_eq!(
            KeyofConstraintReductionState::from_evaluated_step(&interner, current, nested),
            KeyofConstraintReductionState::Continue(nested)
        );
        assert_eq!(
            KeyofConstraintReductionState::from_evaluated_step(&interner, current, literal),
            KeyofConstraintReductionState::Done(literal)
        );
        assert_eq!(
            KeyofConstraintReductionState::from_evaluated_step(&interner, current, current),
            KeyofConstraintReductionState::Done(current)
        );
    }

    #[test]
    fn local_depth_bail_records_request_verdict() {
        let interner = TypeInterner::new();
        let mut evaluator = TypeEvaluator::new(&interner);
        let current = interner.keyof(TypeId::STRING);
        let mut entered = Vec::new();

        for offset in 0..evaluator.keyof_constraint_guard.max_depth() {
            let type_id = TypeId(10_000 + offset);
            assert!(evaluator.keyof_constraint_guard.enter(type_id).is_entered());
            entered.push(type_id);
        }

        assert_eq!(evaluator.evaluate_keyof_or_constraint(current), current);
        assert_request_verdict(&evaluator, current, TerminationKind::DepthExceeded);

        for type_id in entered.into_iter().rev() {
            evaluator.keyof_constraint_guard.leave(type_id);
        }
    }

    #[test]
    fn local_iteration_bail_records_request_verdict() {
        let interner = TypeInterner::new();
        let mut evaluator = TypeEvaluator::new(&interner);
        let current = interner.keyof(TypeId::STRING);
        evaluator.keyof_constraint_guard = RecursionGuard::new(100, 0);

        assert_eq!(evaluator.evaluate_keyof_or_constraint(current), current);
        assert_request_verdict(&evaluator, current, TerminationKind::IterationExceeded);
    }

    #[test]
    fn solver_frame_bail_records_request_verdict() {
        let interner = TypeInterner::new();
        let mut evaluator = TypeEvaluator::new(&interner);
        let current = interner.keyof(TypeId::STRING);
        crate::recursion::reset_solver_stack_frames();
        let mut held = Vec::with_capacity(crate::recursion::MAX_SOLVER_STACK_FRAMES as usize);

        for _ in 0..crate::recursion::MAX_SOLVER_STACK_FRAMES {
            held.push(crate::recursion::try_enter_solver_frame().expect("under frame cap"));
        }

        assert_eq!(evaluator.evaluate_keyof_or_constraint(current), current);
        drop(held);
        crate::recursion::reset_solver_stack_frames();
        assert_request_verdict(&evaluator, current, TerminationKind::SolverStackFrames);
    }

    #[test]
    fn cycle_fallback_does_not_record_request_verdict() {
        let interner = TypeInterner::new();
        let mut evaluator = TypeEvaluator::new(&interner);
        let current = interner.keyof(TypeId::STRING);

        assert!(evaluator.keyof_constraint_guard.enter(current).is_entered());
        assert_eq!(evaluator.evaluate_keyof_or_constraint(current), current);
        evaluator.keyof_constraint_guard.leave(current);

        assert_eq!(
            evaluator.request_result_for_test(current).termination(),
            Termination::Complete
        );
    }
}
