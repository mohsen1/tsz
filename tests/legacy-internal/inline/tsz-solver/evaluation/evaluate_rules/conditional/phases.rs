//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/evaluation/evaluate_rules/conditional/phases.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN b2c6cb9e15282cc5043d1287d1df661ebb840e5c77ebb2276a87a8f0338cb45b 1043 continues_at_depth_cap
    #[test]
    fn continues_at_depth_cap() {
        assert_eq!(
            unresolvable_keyof_lazy_depth_state(MAX_UNRESOLVABLE_KEYOF_LAZY_DEPTH),
            UnresolvableKeyofLazyDepthState::Continue
        );
    }
// TSZ_INLINE_TEST_END b2c6cb9e15282cc5043d1287d1df661ebb840e5c77ebb2276a87a8f0338cb45b

// TSZ_INLINE_TEST_BEGIN af39eb9d641f07b1f7072354874bd70acb67fc66e270b32a0393aec227889ba1 1051 limits_past_depth_cap
    #[test]
    fn limits_past_depth_cap() {
        assert_eq!(
            unresolvable_keyof_lazy_depth_state(MAX_UNRESOLVABLE_KEYOF_LAZY_DEPTH + 1),
            UnresolvableKeyofLazyDepthState::LimitExceeded
        );
    }
// TSZ_INLINE_TEST_END af39eb9d641f07b1f7072354874bd70acb67fc66e270b32a0393aec227889ba1

// TSZ_INLINE_TEST_BEGIN 7fc7ffe2622e41d63300ba1f7a62230eef72887ed1906d648dc54a0ef8a4ea1e 1066 continues_before_tail_call_cap
    #[test]
    fn continues_before_tail_call_cap() {
        assert_eq!(
            tail_call_depth_state(MAX - 1, MAX),
            TailCallDepthState::Continue
        );
    }
// TSZ_INLINE_TEST_END 7fc7ffe2622e41d63300ba1f7a62230eef72887ed1906d648dc54a0ef8a4ea1e

// TSZ_INLINE_TEST_BEGIN a001b86e0fddf49fbb9dbe9989d33da843aebfc19e98c2230cf809cfb8c07e8b 1074 limits_at_tail_call_cap
    #[test]
    fn limits_at_tail_call_cap() {
        assert_eq!(
            tail_call_depth_state(MAX, MAX),
            TailCallDepthState::LimitExceeded
        );
    }
// TSZ_INLINE_TEST_END a001b86e0fddf49fbb9dbe9989d33da843aebfc19e98c2230cf809cfb8c07e8b

// TSZ_INLINE_TEST_BEGIN 03acb75030bb7d6fac323a6fd1199708e646e472c54c6d14bfd48690e363e4c1 1090 budget_bounded_probe_keeps_definitive_verdict_but_blocks_depth_cache
    #[test]
    fn budget_bounded_probe_keeps_definitive_verdict_but_blocks_depth_cache() {
        let probe = ConditionalBranchProbeResult::new(
            BranchRelation::Fails,
            ConditionalBranchCacheStability::BudgetBounded,
        );

        assert_eq!(probe.relation(), BranchRelation::Fails);
        assert_eq!(probe.definitive_verdict(), Some(false));
        assert_eq!(
            probe.depth_agnostic_cache_verdict(),
            ConditionalBranchCacheVerdict::DoNotPublish
        );
        assert_eq!(probe.depth_agnostic_cache_verdict().as_bool(), None);
    }
// TSZ_INLINE_TEST_END 03acb75030bb7d6fac323a6fd1199708e646e472c54c6d14bfd48690e363e4c1

// TSZ_INLINE_TEST_BEGIN 360d34ba50b76684bbada64b96c427d2b537417e82ea25b3485d50f305e60966 1106 undetermined_probe_has_no_cacheable_verdict
    #[test]
    fn undetermined_probe_has_no_cacheable_verdict() {
        let probe = ConditionalBranchProbeResult::new(
            BranchRelation::Undetermined,
            ConditionalBranchCacheStability::DepthAgnostic,
        );

        assert_eq!(probe.relation(), BranchRelation::Undetermined);
        assert_eq!(probe.definitive_verdict(), None);
        assert_eq!(
            probe.depth_agnostic_cache_verdict(),
            ConditionalBranchCacheVerdict::DoNotPublish
        );
        assert_eq!(probe.depth_agnostic_cache_verdict().as_bool(), None);
    }
// TSZ_INLINE_TEST_END 360d34ba50b76684bbada64b96c427d2b537417e82ea25b3485d50f305e60966

// TSZ_INLINE_TEST_BEGIN 71f317e3023fc9cfac8f68329116e1dd632647aa4861d7ae66ff7115e87045dd 1122 depth_agnostic_probe_exports_definitive_cache_verdict
    #[test]
    fn depth_agnostic_probe_exports_definitive_cache_verdict() {
        let probe = ConditionalBranchProbeResult::new(
            BranchRelation::Holds,
            ConditionalBranchCacheStability::DepthAgnostic,
        );

        assert_eq!(probe.relation(), BranchRelation::Holds);
        assert_eq!(probe.definitive_verdict(), Some(true));
        assert_eq!(
            probe.depth_agnostic_cache_verdict(),
            ConditionalBranchCacheVerdict::PublishTrueBranch
        );
        assert_eq!(probe.depth_agnostic_cache_verdict().as_bool(), Some(true));
    }
// TSZ_INLINE_TEST_END 71f317e3023fc9cfac8f68329116e1dd632647aa4861d7ae66ff7115e87045dd

// TSZ_INLINE_TEST_BEGIN 94a16017b9a7e42a3c7c35230bb0e07a7ebdf4471d4231e7822266e8d3ea3c95 1143 enter_reports_prior_depth_and_drop_restores
    #[test]
    fn enter_reports_prior_depth_and_drop_restores() {
        let session = EvaluationSession::new();
        assert_eq!(
            session.conditional_subtype_depth(),
            0,
            "counter starts clean"
        );
        let entry0 = session.enter_conditional_subtype_depth();
        assert_eq!(entry0.prior_depth(), 0, "first entry observes depth 0");
        assert_eq!(session.conditional_subtype_depth(), 1);
        {
            let entry1 = session.enter_conditional_subtype_depth();
            assert_eq!(
                entry1.prior_depth(),
                1,
                "nested entry observes the outer depth"
            );
            assert_eq!(session.conditional_subtype_depth(), 2);
        }
        assert_eq!(
            session.conditional_subtype_depth(),
            1,
            "nested drop restores one level"
        );
        drop(entry0);
        assert_eq!(
            session.conditional_subtype_depth(),
            0,
            "outer drop restores the clean slate"
        );
    }
// TSZ_INLINE_TEST_END 94a16017b9a7e42a3c7c35230bb0e07a7ebdf4471d4231e7822266e8d3ea3c95

// TSZ_INLINE_TEST_BEGIN 31ca637f91eba3568c4a9b77add3cd8c3d713faec4f07e403fcc21577f57eb32 1181 depth_is_restored_on_unwind
    /// #13368: the guard must restore the depth even when the guarded subtype
    /// walk unwinds via a panic a caller (`try_tsz`, LSP) catches, so a stale
    /// positive depth can never leak into the next compilation on a reused
    /// batch/merge-group worker thread (which would force later
    /// conditional-subtype checks onto the conservative false branch).
    #[test]
    fn depth_is_restored_on_unwind() {
        let session = EvaluationSession::new();
        assert_eq!(
            session.conditional_subtype_depth(),
            0,
            "counter starts clean"
        );
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _entry = session.enter_conditional_subtype_depth();
            assert_eq!(session.conditional_subtype_depth(), 1);
            panic!("simulated mid-subtype-walk panic");
        }));
        assert!(result.is_err(), "the closure panicked");
        assert_eq!(
            session.conditional_subtype_depth(),
            0,
            "guard Drop must restore the depth during unwind"
        );
    }
// TSZ_INLINE_TEST_END 31ca637f91eba3568c4a9b77add3cd8c3d713faec4f07e403fcc21577f57eb32
