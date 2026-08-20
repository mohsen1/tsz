//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/instantiation/instantiate/cache_stability.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 9ba674cb5b4712ee28b5995ee39c557aeab8b7d33007473f8fec302ff9a10379 102 stable_request_state_allows_project_cache_publication
    #[test]
    fn stable_request_state_allows_project_cache_publication() {
        let stability = ProjectInstantiationCacheStability::from_ordered_taints([
            (false, ProjectInstantiationCacheTaint::UnionTooComplex),
            (false, ProjectInstantiationCacheTaint::TupleTooLarge),
            (
                false,
                ProjectInstantiationCacheTaint::SolverFrameCurtailment,
            ),
            (
                false,
                ProjectInstantiationCacheTaint::EvaluationFuelExhausted,
            ),
            (false, ProjectInstantiationCacheTaint::Poisoned),
        ]);

        assert_eq!(stability, ProjectInstantiationCacheStability::Stable);
        assert!(stability.is_stable_for_project_cache());
    }
// TSZ_INLINE_TEST_END 9ba674cb5b4712ee28b5995ee39c557aeab8b7d33007473f8fec302ff9a10379

// TSZ_INLINE_TEST_BEGIN 22e8885b6a54e557d0240845cc3fd950cdfda7d9790d82ff6fdedb1da6f21cf3 122 unstable_request_state_names_limit_reason
    #[test]
    fn unstable_request_state_names_limit_reason() {
        let stability = ProjectInstantiationCacheStability::from_ordered_taints([(
            true,
            ProjectInstantiationCacheTaint::SolverFrameCurtailment,
        )]);

        assert_eq!(
            stability,
            ProjectInstantiationCacheStability::Unstable(
                ProjectInstantiationCacheTaint::SolverFrameCurtailment
            )
        );
        assert!(!stability.is_stable_for_project_cache());
    }
// TSZ_INLINE_TEST_END 22e8885b6a54e557d0240845cc3fd950cdfda7d9790d82ff6fdedb1da6f21cf3

// TSZ_INLINE_TEST_BEGIN 495094c07dda4f9b7a669d918f90fa658dc97ecc5c07a0067d9b983bfb09d59b 138 unstable_request_state_keeps_existing_priority_order
    #[test]
    fn unstable_request_state_keeps_existing_priority_order() {
        let stability = ProjectInstantiationCacheStability::from_ordered_taints([
            (true, ProjectInstantiationCacheTaint::UnionTooComplex),
            (true, ProjectInstantiationCacheTaint::TupleTooLarge),
            (true, ProjectInstantiationCacheTaint::SolverFrameCurtailment),
            (
                true,
                ProjectInstantiationCacheTaint::EvaluationFuelExhausted,
            ),
            (true, ProjectInstantiationCacheTaint::Poisoned),
        ]);

        assert_eq!(
            stability,
            ProjectInstantiationCacheStability::Unstable(
                ProjectInstantiationCacheTaint::UnionTooComplex
            )
        );
    }
// TSZ_INLINE_TEST_END 495094c07dda4f9b7a669d918f90fa658dc97ecc5c07a0067d9b983bfb09d59b

// TSZ_INLINE_TEST_BEGIN 5e3c2d2bc404daa7a6375aa2a90c079deed9546011666624fa85ea94cf3b7384 159 second_union_event_taints_pre_existing_pending_snapshot
    #[test]
    fn second_union_event_taints_pre_existing_pending_snapshot() {
        let interner = TypeInterner::new();
        interner.set_union_too_complex();
        let snapshot = ProjectInstantiationCacheLimitSnapshot::capture(&interner);

        assert_eq!(
            snapshot.request_state_stability_after(&interner),
            ProjectInstantiationCacheStability::Stable,
        );
        interner.set_union_too_complex();
        assert_eq!(
            snapshot.request_state_stability_after(&interner),
            ProjectInstantiationCacheStability::Unstable(
                ProjectInstantiationCacheTaint::UnionTooComplex,
            ),
        );
        assert!(interner.take_union_too_complex());
    }
// TSZ_INLINE_TEST_END 5e3c2d2bc404daa7a6375aa2a90c079deed9546011666624fa85ea94cf3b7384
