//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/limits/mod.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 82f2ccd9bfdc9c8cd41d990b54ff1462750b49b44d4130cf35340c8c07496382 745 reset_clears_every_limit_budget_field
    /// Regression test for #13368: batch/row reuse runs many compilations on
    /// one worker thread and relies on [`reset_subtype_thread_local_state`] to
    /// isolate them. A compilation that bailed via the depth/stack breaker or a
    /// caught-and-swallowed panic can leave any [`LimitBudgets`] counter dirty.
    /// Before the fix the reset cleared only three of the eight fields, so the
    /// per-query op budget, cross-evaluator eval depth, per-file evaluation
    /// fuel, and solver recursion-frame count leaked into the next compilation
    /// and made its depth/fuel verdicts schedule-dependent. Dirty every field
    /// the way an aborted mid-evaluation row would and assert the reset zeroes
    /// all of them.
    #[test]
    fn reset_clears_every_limit_budget_field() {
        LIMIT_BUDGETS.with(|b| {
            b.subtype_state.set(pack_depth_fuel(7, 9));
            b.lazy_resolve_failures.set(3);
            b.weak_type_sensitivity.set(5);
            b.eval_query_active.set(11);
            b.eval_query_ops.set(13);
            b.global_eval_depth.set(17);
            b.evaluation_fuel.set(19);
            b.solver_stack_frames.set(23);
        });

        reset_subtype_thread_local_state();

        LIMIT_BUDGETS.with(|b| {
            assert_eq!(b.subtype_state.get(), 0, "subtype chain state");
            assert_eq!(b.lazy_resolve_failures.get(), 0, "lazy-resolve sentinel");
            assert_eq!(b.weak_type_sensitivity.get(), 0, "weak-type sentinel");
            assert_eq!(b.eval_query_active.get(), 0, "per-query active frames");
            assert_eq!(b.eval_query_ops.get(), 0, "per-query op count");
            assert_eq!(b.global_eval_depth.get(), 0, "cross-evaluator eval depth");
            assert_eq!(b.evaluation_fuel.get(), 0, "per-file evaluation fuel");
            assert_eq!(b.solver_stack_frames.get(), 0, "solver recursion frames");
        });
    }
// TSZ_INLINE_TEST_END 82f2ccd9bfdc9c8cd41d990b54ff1462750b49b44d4130cf35340c8c07496382

// TSZ_INLINE_TEST_BEGIN 65e3283ba573715513ee6dd1c1bb57f2cc9528c9c90cce77c738b7d4c4186c14 775 subtype_frame_enter_leave_round_trip
    /// The packed subtype chain state round-trips depth and fuel through the
    /// consolidated accessors with the exact pre/post semantics the relation
    /// cache relies on.
    #[test]
    fn subtype_frame_enter_leave_round_trip() {
        // Ensure a clean slate even if another test on this thread leaked.
        reset_subtype_thread_local_state();

        let outer = enter_subtype_frame();
        assert_eq!(outer.global_depth, 0, "first frame is outermost");
        assert_eq!(outer.fuel, 0, "no fuel consumed before the first frame");

        let inner = enter_subtype_frame();
        assert_eq!(inner.global_depth, 1);
        assert_eq!(inner.fuel, 1);
        assert_eq!(
            remaining_global_subtype_fuel(),
            MAX_GLOBAL_SUBTYPE_FUEL - 2,
            "two frames consumed two fuel units"
        );

        leave_subtype_frame(false);
        assert_eq!(
            remaining_global_subtype_fuel(),
            MAX_GLOBAL_SUBTYPE_FUEL - 2,
            "fuel is monotonic until the outermost frame exits"
        );

        leave_subtype_frame(true);
        assert_eq!(
            remaining_global_subtype_fuel(),
            MAX_GLOBAL_SUBTYPE_FUEL,
            "outermost exit resets the chain fuel"
        );
    }
// TSZ_INLINE_TEST_END 65e3283ba573715513ee6dd1c1bb57f2cc9528c9c90cce77c738b7d4c4186c14

// TSZ_INLINE_TEST_BEGIN 8b21919d9a22488986db00f6efb9d960da42eca53e185c4603458d81874acf55 810 poison_sentinels_advance_and_combine
    /// Sentinel counters advance independently and are visible through both
    /// the single-counter and the combined snapshot accessors.
    #[test]
    fn poison_sentinels_advance_and_combine() {
        let (lazy0, weak0) = poison_sentinel_counts();
        note_lazy_resolve_failure();
        note_weak_type_sensitivity();
        note_weak_type_sensitivity();
        let (lazy1, weak1) = poison_sentinel_counts();
        assert_eq!(lazy1, lazy0 + 1);
        assert_eq!(weak1, weak0 + 2);
        assert_eq!(lazy_resolve_failure_count(), lazy1);
    }
// TSZ_INLINE_TEST_END 8b21919d9a22488986db00f6efb9d960da42eca53e185c4603458d81874acf55
