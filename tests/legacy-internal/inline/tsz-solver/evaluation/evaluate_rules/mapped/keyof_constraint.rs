//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/evaluation/evaluate_rules/mapped/keyof_constraint.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 995697a2faf2765ba6edb50337ebe75e2afeab4858565612e66ed25d43e00904 223 step_state_names_every_guard_fallback
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
// TSZ_INLINE_TEST_END 995697a2faf2765ba6edb50337ebe75e2afeab4858565612e66ed25d43e00904

// TSZ_INLINE_TEST_BEGIN f4eff8e6c203259504ed9c9f227dd14415b874678065861352d855b1d4e57bae 271 reduction_state_names_continuing_shapes
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
// TSZ_INLINE_TEST_END f4eff8e6c203259504ed9c9f227dd14415b874678065861352d855b1d4e57bae

// TSZ_INLINE_TEST_BEGIN 4992977b5c0dc1b7b9259f42852d0236867da8ab6af918a4fedc618b87936e69 292 local_depth_bail_records_request_verdict
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
// TSZ_INLINE_TEST_END 4992977b5c0dc1b7b9259f42852d0236867da8ab6af918a4fedc618b87936e69

// TSZ_INLINE_TEST_BEGIN c550bcbde9c168640efa3d507f573b7624ae894695e49ee0904116795ca2b12c 313 local_iteration_bail_records_request_verdict
    #[test]
    fn local_iteration_bail_records_request_verdict() {
        let interner = TypeInterner::new();
        let mut evaluator = TypeEvaluator::new(&interner);
        let current = interner.keyof(TypeId::STRING);
        evaluator.keyof_constraint_guard = RecursionGuard::new(100, 0);

        assert_eq!(evaluator.evaluate_keyof_or_constraint(current), current);
        assert_request_verdict(&evaluator, current, TerminationKind::IterationExceeded);
    }
// TSZ_INLINE_TEST_END c550bcbde9c168640efa3d507f573b7624ae894695e49ee0904116795ca2b12c

// TSZ_INLINE_TEST_BEGIN 4620ce15aab4b68d7ac6f632d73ed93843efd6151fa098977b3a124725eca644 324 solver_frame_bail_records_request_verdict
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
// TSZ_INLINE_TEST_END 4620ce15aab4b68d7ac6f632d73ed93843efd6151fa098977b3a124725eca644

// TSZ_INLINE_TEST_BEGIN 077e3e722e3fb436d31d7b5e09fbf43440b80b7e33085f2189e5e4b623bca79a 342 cycle_fallback_does_not_record_request_verdict
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
// TSZ_INLINE_TEST_END 077e3e722e3fb436d31d7b5e09fbf43440b80b7e33085f2189e5e4b623bca79a
