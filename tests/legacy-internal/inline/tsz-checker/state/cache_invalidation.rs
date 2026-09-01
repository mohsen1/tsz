//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/state/cache_invalidation.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 55f5b8dd8f9b9a4002644a425878f10dbf56b3001801e2d5b24fb0ca5ea3626d 820 enter_increments_and_drop_decrements
    /// A fresh thread starts at depth zero, and a single entered frame raises
    /// the active depth by exactly one, restoring it on drop. This is the base
    /// invariant the whole stack-overflow guard relies on.
    #[test]
    fn enter_increments_and_drop_decrements() {
        reset_contextual_retry_path();
        assert_eq!(contextual_retry_depth_for_test(), 0);
        {
            let _g = ContextualRetryGuard::enter().expect("first frame must enter");
            assert_eq!(contextual_retry_depth_for_test(), 1);
            {
                let _g2 = ContextualRetryGuard::enter().expect("nested frame must enter");
                assert_eq!(contextual_retry_depth_for_test(), 2);
            }
            assert_eq!(contextual_retry_depth_for_test(), 1);
        }
        assert_eq!(contextual_retry_depth_for_test(), 0);
    }
// TSZ_INLINE_TEST_END 55f5b8dd8f9b9a4002644a425878f10dbf56b3001801e2d5b24fb0ca5ea3626d

// TSZ_INLINE_TEST_BEGIN bf81e6fd9ae3b9a0a6e0fef7491782cf9ef7e007ac6620a1ae03d78ae3a49f70 839 frames_below_cap_always_enter
    /// Any walk shallower than the cap always enters successfully — the cap is
    /// chosen so legitimate (finite) expression trees never hit it, which is
    /// what keeps the guard byte-identical on non-crashing inputs.
    #[test]
    fn frames_below_cap_always_enter() {
        reset_contextual_retry_path();
        let mut guards = Vec::new();
        for expected in 1..=64u32 {
            let g = ContextualRetryGuard::enter().expect("below-cap frame must enter");
            assert_eq!(contextual_retry_depth_for_test(), expected);
            guards.push(g);
        }
        drop(guards);
        assert_eq!(contextual_retry_depth_for_test(), 0);
    }
// TSZ_INLINE_TEST_END bf81e6fd9ae3b9a0a6e0fef7491782cf9ef7e007ac6620a1ae03d78ae3a49f70

// TSZ_INLINE_TEST_BEGIN 4ac9820dad858e9695a0e5c5954fdecd2c1824c090701dd1aadd3d2318748e04 856 entering_at_cap_returns_none
    /// At the cap, `enter` returns `None` so the caller stops recursing. This is
    /// the cycle/stack-overflow cutoff: a non-terminating cyclic expression-child
    /// link is the only way to reach the cap, and reaching it terminates the
    /// walk instead of overflowing the native stack.
    #[test]
    fn entering_at_cap_returns_none() {
        reset_contextual_retry_path();
        // Hold guards up to the cap so the depth is exactly at the limit.
        let mut guards = Vec::new();
        for _ in 0..MAX_CONTEXTUAL_RETRY_DEPTH {
            guards.push(ContextualRetryGuard::enter().expect("frame below cap must enter"));
        }
        assert_eq!(
            contextual_retry_depth_for_test(),
            MAX_CONTEXTUAL_RETRY_DEPTH
        );
        // The next frame is refused, breaking the recursion.
        assert!(
            ContextualRetryGuard::enter().is_none(),
            "a frame at the cap must be refused so a cyclic walk terminates"
        );
        // A refused frame must not change the depth.
        assert_eq!(
            contextual_retry_depth_for_test(),
            MAX_CONTEXTUAL_RETRY_DEPTH
        );
        drop(guards);
        assert_eq!(contextual_retry_depth_for_test(), 0);
    }
// TSZ_INLINE_TEST_END 4ac9820dad858e9695a0e5c5954fdecd2c1824c090701dd1aadd3d2318748e04

// TSZ_INLINE_TEST_BEGIN 259334f6f0482b098ac72abf1063bac323fed7d3d4870a9821bb84134b977c98 885 reset_zeroes_leaked_depth
    /// `reset_contextual_retry_path` zeroes a leaked depth, guaranteeing row
    /// isolation even if a future non-unwinding bail-out leaves the counter
    /// non-zero.
    #[test]
    fn reset_zeroes_leaked_depth() {
        reset_contextual_retry_path();
        // Simulate a leaked frame by forgetting the guard (no drop runs).
        let g = ContextualRetryGuard::enter().expect("frame must enter");
        std::mem::forget(g);
        assert_eq!(contextual_retry_depth_for_test(), 1);
        reset_contextual_retry_path();
        assert_eq!(contextual_retry_depth_for_test(), 0);
    }
// TSZ_INLINE_TEST_END 259334f6f0482b098ac72abf1063bac323fed7d3d4870a9821bb84134b977c98
