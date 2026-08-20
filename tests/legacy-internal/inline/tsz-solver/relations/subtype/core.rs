//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/relations/subtype/core.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 44429b6dfbd1cdd10d4eacdfdef052eec98923ee32f5fc3a1f5ba0a88413e6a3 259 ids_equivalence_matches_only_by_id
    /// An id-only equivalence carries no exact-binder discriminator, so it keeps
    /// the historical id-keyed behavior.
    #[test]
    fn ids_equivalence_matches_only_by_id() {
        let a = TypeId(100);
        let b = TypeId(200);
        let eq = TypeParamEquivalence::ids(a, b);
        assert!(eq.matches_ids(a, b));
        assert!(eq.matches_ids(b, a), "id match is order-insensitive");
        assert!(!eq.matches_ids(a, TypeId(300)));
        // No binders recorded -> binder match never fires.
        assert!(!eq.matches_binders(info(1, 2, 10), info(3, 4, 10)));
    }
// TSZ_INLINE_TEST_END 44429b6dfbd1cdd10d4eacdfdef052eec98923ee32f5fc3a1f5ba0a88413e6a3

// TSZ_INLINE_TEST_BEGIN 75a03664099a1c3ee5754cb455705af3490ec80cf402ce6d599857fd7333b6f3 275 binder_equivalence_matches_registered_pair_both_orders
    /// A registered exact-binder pair matches a reconstructed leaf pair in either
    /// order (the accepted `B ≡ A` bridge). This is the #14345 WAVE-1 positive
    /// case: two reduced-body leaves whose `(file, node)` origins equal the
    /// registered signature params relate.
    #[test]
    fn binder_equivalence_matches_registered_pair_both_orders() {
        let eq = TypeParamEquivalence {
            source: TypeId(100),
            target: TypeId(200),
            binders: Some((binder(57, 20, 10), binder(5, 20, 10))),
        };
        assert!(eq.matches_binders(info(57, 20, 10), info(5, 20, 10)));
        assert!(
            eq.matches_binders(info(5, 20, 10), info(57, 20, 10)),
            "binder match is order-insensitive"
        );
    }
// TSZ_INLINE_TEST_END 75a03664099a1c3ee5754cb455705af3490ec80cf402ce6d599857fd7333b6f3

// TSZ_INLINE_TEST_BEGIN 15e76f3e38ca174a61894f858d8f72af0533364d80cc6b3597c87de1f6bc7513 292 binder_equivalence_rejects_unregistered_pair
    /// A different-origin leaf pair (distinct `(file, node)` that was never
    /// registered) must NOT match — the sound discriminator the name+surface
    /// strip cannot express. A single differing node is enough to reject.
    #[test]
    fn binder_equivalence_rejects_unregistered_pair() {
        let eq = TypeParamEquivalence {
            source: TypeId(100),
            target: TypeId(200),
            binders: Some((binder(57, 20, 10), binder(5, 20, 10))),
        };
        // Different file on one side.
        assert!(!eq.matches_binders(info(99, 20, 10), info(5, 20, 10)));
        // Different node on one side (same file) — distinct declaration site.
        assert!(!eq.matches_binders(info(57, 21, 10), info(5, 20, 10)));
        // Same declaration sites but a different sibling name is also distinct.
        assert!(!eq.matches_binders(info(57, 20, 11), info(5, 20, 10)));
        // Both sides different.
        assert!(!eq.matches_binders(info(1, 1, 10), info(2, 2, 10)));
    }
// TSZ_INLINE_TEST_END 15e76f3e38ca174a61894f858d8f72af0533364d80cc6b3597c87de1f6bc7513

// TSZ_INLINE_TEST_BEGIN 76d18b6ebc8fd57fb208eed1a21757539bb7190324be5ad85599fdca6aa101ae 311 binder_equivalence_never_matches_user_leaves
    /// A `User` (unstamped) leaf carries no declaration site and must never
    /// match on binders, even against a registered declaration-binder pair.
    #[test]
    fn binder_equivalence_never_matches_user_leaves() {
        let eq = TypeParamEquivalence {
            source: TypeId(100),
            target: TypeId(200),
            binders: Some((binder(57, 20, 10), binder(5, 20, 10))),
        };
        let user = TypeParamInfo::simple(Atom(10));
        assert!(!eq.matches_binders(user, info(5, 20, 10)));
        assert!(!eq.matches_binders(info(57, 20, 10), user));
        assert!(!eq.matches_binders(user, user));
    }
// TSZ_INLINE_TEST_END 76d18b6ebc8fd57fb208eed1a21757539bb7190324be5ad85599fdca6aa101ae

// TSZ_INLINE_TEST_BEGIN 864b81cc9261b10b904d2c517163069c1a3efbb0bc782f183cfbd426cba4448c 333 relation_evaluation_result_names_cache_stability
    #[test]
    fn relation_evaluation_result_names_cache_stability() {
        let stable = RelationEvaluationResult::stable(TypeId::STRING);
        assert_eq!(stable.type_id(), TypeId::STRING);
        assert_eq!(stable.cache_stability, RelationEvaluationStability::Stable);
        assert!(stable.is_stable_for_depth_agnostic_cache());
        assert!(!stable.is_unstable_unknown());

        let unstable_unknown = RelationEvaluationResult::unstable(TypeId::UNKNOWN);
        assert_eq!(
            unstable_unknown.cache_stability,
            RelationEvaluationStability::Unstable
        );
        assert!(!unstable_unknown.is_stable_for_depth_agnostic_cache());
        assert!(unstable_unknown.is_unstable_unknown());

        let unstable_string = RelationEvaluationResult::unstable(TypeId::STRING);
        assert!(!unstable_string.is_unstable_unknown());
    }
// TSZ_INLINE_TEST_END 864b81cc9261b10b904d2c517163069c1a3efbb0bc782f183cfbd426cba4448c

// TSZ_INLINE_TEST_BEGIN 4c79327d4213b851abc63ef1a71f225d57de5c41aa4001cff46c0aaf862af46c 353 relation_evaluation_result_imports_memo_stability
    #[test]
    fn relation_evaluation_result_imports_memo_stability() {
        let complete_memo = EvaluationMemoResult::for_depth_agnostic_memo(
            EvaluationResult::complete(TypeId::NUMBER),
            EvaluationRequestStability::Stable,
        );
        let complete = RelationEvaluationResult::from_depth_agnostic_memo(complete_memo);
        assert_eq!(complete.type_id(), TypeId::NUMBER);
        assert_eq!(
            complete.cache_stability,
            RelationEvaluationStability::Stable
        );
        assert!(complete.is_stable_for_depth_agnostic_cache());

        let incomplete_memo = EvaluationMemoResult::for_depth_agnostic_memo(
            EvaluationResult::incomplete(TypeId::UNKNOWN, TerminationKind::DepthExceeded),
            EvaluationRequestStability::Stable,
        );
        let incomplete = RelationEvaluationResult::from_depth_agnostic_memo(incomplete_memo);
        assert_eq!(incomplete.type_id(), TypeId::UNKNOWN);
        assert_eq!(
            incomplete.cache_stability,
            RelationEvaluationStability::Unstable
        );
        assert!(!incomplete.is_stable_for_depth_agnostic_cache());
        assert!(incomplete.is_unstable_unknown());
    }
// TSZ_INLINE_TEST_END 4c79327d4213b851abc63ef1a71f225d57de5c41aa4001cff46c0aaf862af46c

// TSZ_INLINE_TEST_BEGIN 6c3dd97bcaa6f11f90d6a327392d9786fc68b0472fb3acd427d44b0a2214a651 1753 restores_flags_after_closure
    #[test]
    fn restores_flags_after_closure() {
        let interner = TypeInterner::new();
        let mut checker = SubtypeChecker::new(&interner);
        checker.any_propagation = AnyPropagationMode::All;
        checker.identity_cycle_check = false;
        checker.disable_method_bivariance = false;
        checker.strict_function_types = false;

        let inside = checker.with_identity_check_mode(|sub| {
            (
                sub.any_propagation,
                sub.identity_cycle_check,
                sub.disable_method_bivariance,
                sub.strict_function_types,
            )
        });

        assert_eq!(inside.0, AnyPropagationMode::TopLevelOnly);
        assert!(inside.1);
        assert!(inside.2);
        assert!(inside.3);

        assert_eq!(checker.any_propagation, AnyPropagationMode::All);
        assert!(!checker.identity_cycle_check);
        assert!(!checker.disable_method_bivariance);
        assert!(!checker.strict_function_types);
    }
// TSZ_INLINE_TEST_END 6c3dd97bcaa6f11f90d6a327392d9786fc68b0472fb3acd427d44b0a2214a651
