//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/instantiation/result.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 1166cdd057168d09017fd8169c0daf48fd68402ec7b2e503d89a7def1047233e 230 ok_result_passes_through_type_id
    #[test]
    fn ok_result_passes_through_type_id() {
        let r = InstantiationResult::ok(TypeId::NUMBER);
        assert_eq!(r.type_id(), TypeId::NUMBER);
        assert!(!r.depth_exceeded());
        assert_eq!(r.termination(), InstantiationTermination::Complete);
        assert_eq!(r.into_type_id(), TypeId::NUMBER);
    }
// TSZ_INLINE_TEST_END 1166cdd057168d09017fd8169c0daf48fd68402ec7b2e503d89a7def1047233e

// TSZ_INLINE_TEST_BEGIN 510a1849c9294cf798081b8fd7cfd87b391ad9358e3e8448171841cd00a21304 239 overflow_result_reports_sentinel_when_no_partial
    #[test]
    fn overflow_result_reports_sentinel_when_no_partial() {
        let r = InstantiationResult::overflow();
        assert!(r.depth_exceeded());
        assert_eq!(r.termination(), InstantiationTermination::DepthExceeded);
        // `overflow()` (no partial) still surfaces the `ERROR` sentinel.
        assert_eq!(r.into_type_id(), TypeId::ERROR);
    }
// TSZ_INLINE_TEST_END 510a1849c9294cf798081b8fd7cfd87b391ad9358e3e8448171841cd00a21304

// TSZ_INLINE_TEST_BEGIN 52b11239594508d6cbb18473e4396f31e0d8505456c22efb64d722192efa6041 248 overflow_with_keeps_partial_type
    #[test]
    fn overflow_with_keeps_partial_type() {
        // A depth/frame bail carries its relation-preserving partial type
        // (never a substitution-bound free param) instead of collapsing to
        // `ERROR`, so consumers do not fall back to an un-instantiated
        // original and resurface a free `T` (#13652).
        let r = InstantiationResult::overflow_with(TypeId::STRING);
        assert!(r.depth_exceeded());
        assert_eq!(r.termination(), InstantiationTermination::DepthExceeded);
        assert_eq!(r.into_type_id(), TypeId::STRING);
    }
// TSZ_INLINE_TEST_END 52b11239594508d6cbb18473e4396f31e0d8505456c22efb64d722192efa6041

// TSZ_INLINE_TEST_BEGIN f17cbc8669c7bf6bea9a630a6a2b2009980f3f47afa54ac0dbc987e59f250eb6 260 from_walk_routes_depth_flag
    #[test]
    fn from_walk_routes_depth_flag() {
        let ok = InstantiationResult::from_walk(TypeId::STRING, InstantiationTermination::Complete);
        assert_eq!(ok.into_type_id(), TypeId::STRING);
        assert_eq!(ok.termination(), InstantiationTermination::Complete);

        // A depth-exceeded walk keeps the partial type the instantiator
        // produced (the relation-preserving bail value) while still flagging
        // the overflow so the cross-call cache refuses to memoize it.
        let bad =
            InstantiationResult::from_walk(TypeId::STRING, InstantiationTermination::DepthExceeded);
        assert!(bad.depth_exceeded());
        assert_eq!(bad.termination(), InstantiationTermination::DepthExceeded);
        assert_eq!(bad.into_type_id(), TypeId::STRING);
    }
// TSZ_INLINE_TEST_END f17cbc8669c7bf6bea9a630a6a2b2009980f3f47afa54ac0dbc987e59f250eb6

// TSZ_INLINE_TEST_BEGIN dd85987258cbcee7785b74fb9f679ac511ff1eac61f4f842078b6017e3f9d74f 276 termination_names_depth_guard_bit
    #[test]
    fn termination_names_depth_guard_bit() {
        assert_eq!(
            InstantiationTermination::from_depth_exceeded(false),
            InstantiationTermination::Complete
        );
        assert_eq!(
            InstantiationTermination::from_depth_exceeded(true),
            InstantiationTermination::DepthExceeded
        );
    }
// TSZ_INLINE_TEST_END dd85987258cbcee7785b74fb9f679ac511ff1eac61f4f842078b6017e3f9d74f

// TSZ_INLINE_TEST_BEGIN 972d299cab99f613b29fb145f34878d65cce4c636025dda61c8e08bd86c241e0 288 memo_result_requires_clean_instantiation_and_request_state
    #[test]
    fn memo_result_requires_clean_instantiation_and_request_state() {
        let stable = InstantiationMemoResult::for_project_cache(
            InstantiationResult::ok(TypeId::STRING),
            true,
        );
        assert_eq!(stable.cache_stability, InstantiationMemoStability::Stable);
        assert!(stable.is_stable_for_project_cache());
        assert_eq!(stable.into_result().type_id(), TypeId::STRING);

        let request_state_tainted = InstantiationMemoResult::for_project_cache(
            InstantiationResult::ok(TypeId::NUMBER),
            false,
        );
        assert_eq!(
            request_state_tainted.cache_stability,
            InstantiationMemoStability::Unstable
        );
        assert!(!request_state_tainted.is_stable_for_project_cache());
        assert_eq!(
            request_state_tainted.into_result().type_id(),
            TypeId::NUMBER
        );

        // A plain (local-only) depth-exceeded result is a pure function of
        // the request — the walk-local depth cap always starts fresh at 0
        // (see `TypeInstantiator::ambient_frame_exhausted`'s doc) — so with
        // clean surrounding request state it stays eligible for the
        // project-wide cache instead of being treated as unstable.
        let locally_overflowed = InstantiationMemoResult::for_project_cache(
            InstantiationResult::overflow_with(TypeId::BOOLEAN),
            true,
        );
        assert_eq!(
            locally_overflowed.cache_stability,
            InstantiationMemoStability::Stable
        );
        assert!(locally_overflowed.is_stable_for_project_cache());
        assert_eq!(locally_overflowed.into_result().type_id(), TypeId::BOOLEAN);

        // A depth-exceeded result that bailed through the SHARED
        // cross-operation solver-frame budget is ambient state, not a pure
        // function of the request, so it must stay unstable even with clean
        // request state.
        let ambient_overflowed = InstantiationMemoResult::for_project_cache(
            InstantiationResult::from_walk_with_ambient_limit(
                TypeId::BOOLEAN,
                InstantiationTermination::DepthExceeded,
                true,
            ),
            true,
        );
        assert_eq!(
            ambient_overflowed.cache_stability,
            InstantiationMemoStability::Unstable
        );
        assert!(!ambient_overflowed.is_stable_for_project_cache());
        assert_eq!(ambient_overflowed.into_result().type_id(), TypeId::BOOLEAN);
    }
// TSZ_INLINE_TEST_END 972d299cab99f613b29fb145f34878d65cce4c636025dda61c8e08bd86c241e0
