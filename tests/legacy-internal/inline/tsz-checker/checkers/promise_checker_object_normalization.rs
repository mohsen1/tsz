//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/checkers/promise_checker_object_normalization.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 4a37920ac57c81481cd2c69b61090ea47ae98cf5da018d66329e1a845716ca68 373 visit_guard_blocks_reentry_and_restores_on_drop
    #[test]
    fn visit_guard_blocks_reentry_and_restores_on_drop() {
        reset_awaited_eval_thread_local_state();
        let t = TypeId(7);
        let outer = AwaitedEvalVisitGuard::enter(t).expect("first entry succeeds");
        assert!(
            AwaitedEvalVisitGuard::enter(t).is_none(),
            "re-entry while in flight must be blocked"
        );
        // A different type is independent.
        let other = AwaitedEvalVisitGuard::enter(TypeId(8)).expect("distinct type enters");
        drop(other);
        drop(outer);
        assert!(
            AwaitedEvalVisitGuard::enter(t).is_some(),
            "membership must be cleared on drop"
        );
        reset_awaited_eval_thread_local_state();
    }
// TSZ_INLINE_TEST_END 4a37920ac57c81481cd2c69b61090ea47ae98cf5da018d66329e1a845716ca68

// TSZ_INLINE_TEST_BEGIN fc057b1cd263be8bc03e4d1ba72d5046440a3bee82c72762fa25e4a40362b075 393 clamp_epoch_bumps_and_resets
    #[test]
    fn clamp_epoch_bumps_and_resets() {
        reset_awaited_eval_thread_local_state();
        let before = awaited_eval_clamp_epoch();
        bump_awaited_eval_clamp_epoch();
        assert_ne!(
            awaited_eval_clamp_epoch(),
            before,
            "clamp epoch must advance so a subtree clamp is observable"
        );
        reset_awaited_eval_thread_local_state();
        assert_eq!(
            awaited_eval_clamp_epoch(),
            0,
            "reset zeroes the clamp epoch"
        );
    }
// TSZ_INLINE_TEST_END fc057b1cd263be8bc03e4d1ba72d5046440a3bee82c72762fa25e4a40362b075
