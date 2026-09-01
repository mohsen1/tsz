//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/evaluation/result.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 5387843b4a45d3024f69063e31f5c59a3878f34be417b044ce13affe81aa7adf 423 complete_result_wraps_evaluated_type_id
    #[test]
    fn complete_result_wraps_evaluated_type_id() {
        let result = EvaluationResult::complete(TypeId::STRING);

        assert_eq!(result.type_id(), TypeId::STRING);
        assert_eq!(result.into_type_id(), TypeId::STRING);
        assert_eq!(result.termination(), Termination::Complete);
        assert!(result.is_complete());
        assert!(!result.is_incomplete());
        assert!(result.is_identity_for(TypeId::STRING));
        assert!(!result.is_identity_for(TypeId::NUMBER));
    }
// TSZ_INLINE_TEST_END 5387843b4a45d3024f69063e31f5c59a3878f34be417b044ce13affe81aa7adf

// TSZ_INLINE_TEST_BEGIN 3d18b7833568f9e726929a7dbd3f7b5830de0ac4b3ff4f33072f6efc736bbdb2 436 incomplete_result_carries_partial_and_kind
    #[test]
    fn incomplete_result_carries_partial_and_kind() {
        // `DepthExceeded` is a real producer as of #14346 stage 3 (the
        // `guard.is_exceeded()` prologue bail); the partial/verdict contract is
        // identical for every kind.
        let result = EvaluationResult::incomplete(TypeId::NUMBER, TerminationKind::DepthExceeded);

        assert!(!result.is_complete());
        assert!(result.is_incomplete());
        // `into_type_id` returns the relation-preserving partial regardless of
        // the verdict — the same collapse every consumer performs today.
        assert_eq!(result.into_type_id(), TypeId::NUMBER);
        assert_eq!(
            result.termination(),
            Termination::Incomplete {
                kind: TerminationKind::DepthExceeded,
                partial: TypeId::NUMBER,
            }
        );
    }
// TSZ_INLINE_TEST_END 3d18b7833568f9e726929a7dbd3f7b5830de0ac4b3ff4f33072f6efc736bbdb2

// TSZ_INLINE_TEST_BEGIN 454b4344dbcdeb162dfe51532b4f53efc7f0e8be13ee9c99b753e3d9d6dfc399 457 memo_result_stability_requires_complete_result_and_clean_request_state
    #[test]
    fn memo_result_stability_requires_complete_result_and_clean_request_state() {
        let complete = EvaluationResult::complete(TypeId::STRING);
        let stable = EvaluationMemoResult::for_depth_agnostic_memo(
            complete,
            EvaluationRequestStability::Stable,
        );

        assert_eq!(stable.result(), complete);
        assert_eq!(stable.type_id(), TypeId::STRING);
        assert_eq!(stable.into_type_id(), TypeId::STRING);
        assert_eq!(stable.cache_stability, EvaluationMemoStability::Stable);
        assert!(stable.is_stable_for_depth_agnostic_cache());

        let request_state_tainted = EvaluationMemoResult::for_depth_agnostic_memo(
            complete,
            EvaluationRequestStability::RecursionLimit,
        );
        assert_eq!(
            request_state_tainted.cache_stability,
            EvaluationMemoStability::Unstable
        );
        assert!(!request_state_tainted.is_stable_for_depth_agnostic_cache());

        let unresolved_def_named = EvaluationMemoResult::for_depth_agnostic_memo(
            complete,
            EvaluationRequestStability::UnresolvedDef,
        );
        assert_eq!(
            unresolved_def_named.cache_stability,
            EvaluationMemoStability::Stable
        );
        assert!(unresolved_def_named.is_stable_for_depth_agnostic_cache());
        assert!(unresolved_def_named.is_stable_for_per_query_memo());

        let incomplete =
            EvaluationResult::incomplete(TypeId::NUMBER, TerminationKind::DepthExceeded);
        let typed_tainted = EvaluationMemoResult::for_depth_agnostic_memo(
            incomplete,
            EvaluationRequestStability::Stable,
        );
        assert_eq!(typed_tainted.into_type_id(), TypeId::NUMBER);
        assert_eq!(
            typed_tainted.cache_stability,
            EvaluationMemoStability::Unstable
        );
        assert!(!typed_tainted.is_stable_for_depth_agnostic_cache());
    }
// TSZ_INLINE_TEST_END 454b4344dbcdeb162dfe51532b4f53efc7f0e8be13ee9c99b753e3d9d6dfc399

// TSZ_INLINE_TEST_BEGIN fe9f4d819884e0aefdf920d435c1a39be0a8fdef24ea3e6578639c5321721ecc 506 run_wide_cache_stability_refuses_every_non_stable_request_verdict
    #[test]
    fn run_wide_cache_stability_refuses_every_non_stable_request_verdict() {
        // #16553/#16587 adjacent matrix: `is_stable_for_run_wide_cache` must
        // accept only a complete result with a `Stable` request verdict —
        // every other request-state taint, plus any incomplete termination,
        // must be refused even though some of them are tolerated by the
        // looser `is_stable_for_depth_agnostic_cache` gate above (that gate
        // stays loose deliberately, for the top-level entry write; see
        // `is_stable_for_run_wide_cache`'s doc comment for why the
        // *intermediate* drain needs the stricter gate instead).
        let complete = EvaluationResult::complete(TypeId::STRING);

        let stable = EvaluationMemoResult::for_depth_agnostic_memo(
            complete,
            EvaluationRequestStability::Stable,
        );
        assert!(stable.is_stable_for_run_wide_cache());

        for tainted_state in [
            EvaluationRequestStability::UnresolvedDef,
            EvaluationRequestStability::RecursionLimit,
            EvaluationRequestStability::IncompleteVerdict,
        ] {
            let memo = EvaluationMemoResult::for_depth_agnostic_memo(complete, tainted_state);
            assert!(
                !memo.is_stable_for_run_wide_cache(),
                "{tainted_state:?} must be refused by the run-wide cache gate"
            );
        }

        // A `Stable` request verdict paired with a guard-truncated (incomplete)
        // termination must also be refused: the collapsed `type_id` is only a
        // partial approximation, not a converged answer.
        let incomplete =
            EvaluationResult::incomplete(TypeId::NUMBER, TerminationKind::DepthExceeded);
        let incomplete_memo = EvaluationMemoResult::for_depth_agnostic_memo(
            incomplete,
            EvaluationRequestStability::Stable,
        );
        assert!(!incomplete_memo.is_stable_for_run_wide_cache());
    }
// TSZ_INLINE_TEST_END fe9f4d819884e0aefdf920d435c1a39be0a8fdef24ea3e6578639c5321721ecc

// TSZ_INLINE_TEST_BEGIN 9ea13f63d6d888dba87545254171c803c9b389af9f7e8695104d10d3d9529760 548 request_stability_names_request_state_reason
    #[test]
    fn request_stability_names_request_state_reason() {
        assert_eq!(
            EvaluationRequestStability::from_request_state(false, false, false),
            EvaluationRequestStability::Stable
        );
        assert_eq!(
            EvaluationRequestStability::from_request_state(true, false, false),
            EvaluationRequestStability::IncompleteVerdict
        );
        assert_eq!(
            EvaluationRequestStability::from_request_state(false, true, false),
            EvaluationRequestStability::RecursionLimit
        );
        assert_eq!(
            EvaluationRequestStability::from_request_state(false, false, true),
            EvaluationRequestStability::UnresolvedDef
        );
        assert_eq!(
            EvaluationRequestStability::from_request_state(true, true, true),
            EvaluationRequestStability::IncompleteVerdict,
            "typed incomplete verdict should stay the primary request-state reason"
        );
        assert_eq!(
            EvaluationRequestStability::from_request_state(false, true, true),
            EvaluationRequestStability::RecursionLimit,
            "recursion-limit taint should stay primary when both legacy taints are set"
        );
        assert!(EvaluationRequestStability::Stable.is_stable_for_depth_agnostic_cache());
        assert!(
            !EvaluationRequestStability::IncompleteVerdict.is_stable_for_depth_agnostic_cache()
        );
        assert!(!EvaluationRequestStability::RecursionLimit.is_stable_for_depth_agnostic_cache());
        assert!(!EvaluationRequestStability::UnresolvedDef.is_stable_for_depth_agnostic_cache());
        assert!(EvaluationRequestStability::UnresolvedDef.is_stable_for_per_query_memo());
    }
// TSZ_INLINE_TEST_END 9ea13f63d6d888dba87545254171c803c9b389af9f7e8695104d10d3d9529760

// TSZ_INLINE_TEST_BEGIN c3448a02f88599f5cc8d0cfeddfcd053d37679f13f19fcfbc911db08401dff7b 585 cached_memo_result_is_stable_and_complete
    #[test]
    fn cached_memo_result_is_stable_and_complete() {
        let cached = EvaluationMemoResult::cached(TypeId::BOOLEAN);

        assert_eq!(cached.type_id(), TypeId::BOOLEAN);
        assert!(cached.is_stable_for_depth_agnostic_cache());
        assert_eq!(cached.result().termination(), Termination::Complete);
    }
// TSZ_INLINE_TEST_END c3448a02f88599f5cc8d0cfeddfcd053d37679f13f19fcfbc911db08401dff7b

// TSZ_INLINE_TEST_BEGIN 8e16d0280ab2af132dd6c34e17f44384ed978d8a56f9ccc424d3303c2ce25a9d 594 unstable_complete_memo_result_collapses_without_becoming_cacheable
    #[test]
    fn unstable_complete_memo_result_collapses_without_becoming_cacheable() {
        let result = EvaluationMemoResult::unstable_complete(TypeId::STRING);

        assert_eq!(result.type_id(), TypeId::STRING);
        assert_eq!(result.into_type_id(), TypeId::STRING);
        assert!(!result.is_stable_for_depth_agnostic_cache());
        assert!(!result.is_stable_for_per_query_memo());
    }
// TSZ_INLINE_TEST_END 8e16d0280ab2af132dd6c34e17f44384ed978d8a56f9ccc424d3303c2ce25a9d

// TSZ_INLINE_TEST_BEGIN 2a6feaaaa02285e632ac9857dda7457f283cf2e1566bb54db73eb54710cfb020 604 boundary_intrinsic_names_the_iteration_bail_only
    #[test]
    fn boundary_intrinsic_names_the_iteration_bail_only() {
        // The per-evaluator total-work counter resets at every fresh-evaluator
        // boundary, so an iteration bail reproduces from the key alone.
        assert!(TerminationKind::IterationExceeded.is_boundary_intrinsic());
        // Depth is per-evaluator but reach-dependent; ambient/global budgets do
        // not reset per evaluator — all stay excluded.
        assert!(!TerminationKind::DepthExceeded.is_boundary_intrinsic());
        assert!(!TerminationKind::FuelExhausted.is_boundary_intrinsic());
        assert!(!TerminationKind::SolverStackFrames.is_boundary_intrinsic());
        assert!(!TerminationKind::CrossEvalCycle.is_boundary_intrinsic());
        assert!(!TerminationKind::QueryOpBudget.is_boundary_intrinsic());
    }
// TSZ_INLINE_TEST_END 2a6feaaaa02285e632ac9857dda7457f283cf2e1566bb54db73eb54710cfb020

// TSZ_INLINE_TEST_BEGIN b7f11a876b1864587cce4a94aaf305ee02c69b220a60a3bc52b2d084f8169e55 618 per_query_memo_retains_boundary_intrinsic_partials_but_never_durably
    #[test]
    fn per_query_memo_retains_boundary_intrinsic_partials_but_never_durably() {
        // A converged result keeps the pre-existing behavior: window- and
        // depth-agnostic-cacheable.
        let complete = EvaluationMemoResult::for_depth_agnostic_memo(
            EvaluationResult::complete(TypeId::STRING),
            EvaluationRequestStability::Stable,
        );
        assert!(complete.is_stable_for_per_query_memo());
        assert!(complete.is_stable_for_depth_agnostic_cache());

        // A boundary-intrinsic bail is retained in the window memo (kills the
        // re-walk storm) but stays out of every durable cache.
        let partial = EvaluationMemoResult::for_depth_agnostic_memo(
            EvaluationResult::incomplete(TypeId::NUMBER, TerminationKind::IterationExceeded),
            EvaluationRequestStability::IncompleteVerdict,
        );
        assert!(
            partial.is_stable_for_per_query_memo(),
            "iteration-exceeded partial should be window-retainable"
        );
        assert!(
            !partial.is_stable_for_depth_agnostic_cache(),
            "iteration-exceeded partial must never reach a durable cache"
        );
        assert_eq!(partial.into_type_id(), TypeId::NUMBER);

        // Reach-dependent and ambient/global-budget bails stay excluded from the
        // window memo too, so a budget-dependent partial is never reused as a
        // converged answer.
        for kind in [
            TerminationKind::DepthExceeded,
            TerminationKind::FuelExhausted,
            TerminationKind::SolverStackFrames,
            TerminationKind::CrossEvalCycle,
            TerminationKind::QueryOpBudget,
        ] {
            let partial = EvaluationMemoResult::for_depth_agnostic_memo(
                EvaluationResult::incomplete(TypeId::NUMBER, kind),
                EvaluationRequestStability::IncompleteVerdict,
            );
            assert!(
                !partial.is_stable_for_per_query_memo(),
                "{kind:?} partial must not be window-retained"
            );
            assert!(!partial.is_stable_for_depth_agnostic_cache());
        }
    }
// TSZ_INLINE_TEST_END b7f11a876b1864587cce4a94aaf305ee02c69b220a60a3bc52b2d084f8169e55

// TSZ_INLINE_TEST_BEGIN fda48fa9e4e1bda54d7eb4f296e893a2e12ca9f0560ae614bbcb59261e0c7e4f 667 from_memoized_result_preserves_the_stored_verdict
    #[test]
    fn from_memoized_result_preserves_the_stored_verdict() {
        // A converged stored result round-trips as fully stable.
        let complete =
            EvaluationMemoResult::from_memoized_result(EvaluationResult::complete(TypeId::BOOLEAN));
        assert_eq!(complete.type_id(), TypeId::BOOLEAN);
        assert!(complete.is_stable_for_depth_agnostic_cache());
        assert!(complete.is_stable_for_per_query_memo());

        // A stored boundary-intrinsic partial comes back out still incomplete:
        // window-retainable but refused by durable caches, so a hit is never
        // promoted into a depth-agnostic cache as if it were converged.
        let partial = EvaluationMemoResult::from_memoized_result(EvaluationResult::incomplete(
            TypeId::NUMBER,
            TerminationKind::IterationExceeded,
        ));
        assert_eq!(partial.into_type_id(), TypeId::NUMBER);
        assert!(!partial.is_stable_for_depth_agnostic_cache());
        assert!(partial.is_stable_for_per_query_memo());
        assert_eq!(
            partial.result().termination(),
            Termination::Incomplete {
                kind: TerminationKind::IterationExceeded,
                partial: TypeId::NUMBER,
            }
        );
    }
// TSZ_INLINE_TEST_END fda48fa9e4e1bda54d7eb4f296e893a2e12ca9f0560ae614bbcb59261e0c7e4f
