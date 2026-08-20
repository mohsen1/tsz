//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/evaluation/evaluate_rules/mapped/keys_guard.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 72b2afdd364b374030320fac24867f7c4fb9b466bd45fdee083cbcfc1bffeedf 100 reentry_of_in_flight_type_is_rejected
    #[test]
    fn reentry_of_in_flight_type_is_rejected() {
        let t = TypeId(4242);
        let MappedKeysVisitState::Entered(outer) = MappedKeysVisitGuard::enter(t) else {
            panic!("first entry succeeds");
        };
        assert!(is_visiting(t));
        assert!(
            matches!(
                MappedKeysVisitGuard::enter(t),
                MappedKeysVisitState::AlreadyVisiting
            ),
            "re-entering an in-flight TypeId must defer"
        );
        drop(outer);
        assert!(!is_visiting(t), "drop must restore membership");
    }
// TSZ_INLINE_TEST_END 72b2afdd364b374030320fac24867f7c4fb9b466bd45fdee083cbcfc1bffeedf

// TSZ_INLINE_TEST_BEGIN 3d9c76753dfd37aeff9d619371048ec0e33a5e8f78607ef6e4ec17d40a9e148b 121 membership_is_restored_on_unwind
    /// #13368: the guard must clear membership even when the guarded work
    /// unwinds via a panic a caller catches, so a stale interner-local key can
    /// never leak into the next compilation on a reused worker thread.
    #[test]
    fn membership_is_restored_on_unwind() {
        let t = TypeId(99);
        let result = std::panic::catch_unwind(|| {
            let MappedKeysVisitState::Entered(_guard) = MappedKeysVisitGuard::enter(t) else {
                panic!("entry succeeds");
            };
            assert!(is_visiting(t));
            panic!("simulated mid-extraction panic");
        });
        assert!(result.is_err(), "the closure panicked");
        assert!(
            !is_visiting(t),
            "guard Drop must remove the key during unwind"
        );
    }
// TSZ_INLINE_TEST_END 3d9c76753dfd37aeff9d619371048ec0e33a5e8f78607ef6e4ec17d40a9e148b
