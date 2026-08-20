//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/assignability/assignability_checker.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN fc031f371b907a44b147318578d56738458970f7b170027dc45da11e63e3b4c3 1341 reentry_of_in_flight_type_is_rejected
    #[test]
    fn reentry_of_in_flight_type_is_rejected() {
        let t = TypeId(4242);
        let outer = AssignabilityEvalVisitGuard::enter(t).expect("first entry succeeds");
        assert!(AssignabilityEvalVisitGuard::is_visiting(t));
        assert!(
            AssignabilityEvalVisitGuard::enter(t).is_none(),
            "re-entering an in-flight TypeId must short-circuit"
        );
        drop(outer);
        assert!(
            !AssignabilityEvalVisitGuard::is_visiting(t),
            "drop must restore membership"
        );
    }
// TSZ_INLINE_TEST_END fc031f371b907a44b147318578d56738458970f7b170027dc45da11e63e3b4c3

// TSZ_INLINE_TEST_BEGIN 1927c41921e0aa128e009ca976fd03adc0447d904c5c1f8f7ae3861f64e91b3c 1360 membership_is_restored_on_unwind
    /// #13368: the guard must clear membership even when evaluation unwinds via
    /// a panic a caller (`try_tsz`, LSP) catches, so a stale interner-local key
    /// can never leak into the next compilation on a reused worker thread.
    #[test]
    fn membership_is_restored_on_unwind() {
        let t = TypeId(99);
        let result = std::panic::catch_unwind(|| {
            let _guard = AssignabilityEvalVisitGuard::enter(t).expect("entry succeeds");
            assert!(AssignabilityEvalVisitGuard::is_visiting(t));
            panic!("simulated mid-evaluation panic");
        });
        assert!(result.is_err(), "the closure panicked");
        assert!(
            !AssignabilityEvalVisitGuard::is_visiting(t),
            "guard Drop must remove the key during unwind"
        );
    }
// TSZ_INLINE_TEST_END 1927c41921e0aa128e009ca976fd03adc0447d904c5c1f8f7ae3861f64e91b3c
