//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/evaluation/cache_stability.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN a6b329984183cc5720505d6dd07e58b52864e1ac8a09c9a20186cd95f13fb6c1 48 union_complexity_snapshot_only_blocks_new_limit_events
    #[test]
    fn union_complexity_snapshot_only_blocks_new_limit_events() {
        let interner = TypeInterner::new();

        let clean_snapshot = EvaluationCacheLimitSnapshot::capture(&interner);
        assert_eq!(
            clean_snapshot.state_after(&interner),
            EvaluationCacheLimitState::Stable
        );
        assert!(clean_snapshot.union_complexity_stayed_stable_after(&interner));

        interner.set_union_too_complex();
        assert_eq!(
            clean_snapshot.state_after(&interner),
            EvaluationCacheLimitState::UnionComplexityNewlyExceeded
        );
        assert!(!clean_snapshot.union_complexity_stayed_stable_after(&interner));

        let pre_existing_snapshot = EvaluationCacheLimitSnapshot::capture(&interner);
        assert_eq!(
            pre_existing_snapshot.state_after(&interner),
            EvaluationCacheLimitState::Stable
        );
        assert!(pre_existing_snapshot.union_complexity_stayed_stable_after(&interner));

        interner.set_union_too_complex();
        assert_eq!(
            pre_existing_snapshot.state_after(&interner),
            EvaluationCacheLimitState::UnionComplexityNewlyExceeded,
            "a second event must taint the cache even while the sticky signal was already pending"
        );
        assert!(interner.take_union_too_complex());
    }
// TSZ_INLINE_TEST_END a6b329984183cc5720505d6dd07e58b52864e1ac8a09c9a20186cd95f13fb6c1
