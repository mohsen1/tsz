//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/evaluation/cross_eval_guard.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 3ec65bf7f3410cdd275f5e29072a46a49c0e3c447edc0c5f3d4a7b7bc6920225 168 memo_keys_on_index_access_options
    #[test]
    fn memo_keys_on_index_access_options() {
        let session = EvaluationSession::new();
        reset_query_memo(&session);
        let t = TypeId(7);
        let default_key = EvaluationCacheKey::new(t, false, false);
        let no_unchecked_key = EvaluationCacheKey::new(t, true, false);
        let exact_optional_key = EvaluationCacheKey::new(t, false, true);
        let both_key = EvaluationCacheKey::new(t, true, true);
        let resolver_key = default_key.with_resolver_generation(7);
        query_memo_put(
            &session,
            default_key,
            EvaluationResult::complete(TypeId(70)),
        );
        query_memo_put(
            &session,
            no_unchecked_key,
            EvaluationResult::complete(TypeId(71)),
        );
        query_memo_put(
            &session,
            exact_optional_key,
            EvaluationResult::complete(TypeId(72)),
        );
        query_memo_put(
            &session,
            resolver_key,
            EvaluationResult::complete(TypeId(73)),
        );
        assert_eq!(
            query_memo_get(&session, default_key),
            Some(EvaluationResult::complete(TypeId(70)))
        );
        assert_eq!(
            query_memo_get(&session, no_unchecked_key),
            Some(EvaluationResult::complete(TypeId(71)))
        );
        assert_eq!(
            query_memo_get(&session, exact_optional_key),
            Some(EvaluationResult::complete(TypeId(72)))
        );
        assert_eq!(
            query_memo_get(&session, resolver_key),
            Some(EvaluationResult::complete(TypeId(73)))
        );
        assert_eq!(query_memo_get(&session, both_key), None);
        assert_eq!(
            query_memo_get(&session, default_key.with_resolver_generation(8)),
            None
        );
        reset_query_memo(&session);
        assert_eq!(query_memo_get(&session, default_key), None);
        assert_eq!(query_memo_get(&session, resolver_key), None);
    }
// TSZ_INLINE_TEST_END 3ec65bf7f3410cdd275f5e29072a46a49c0e3c447edc0c5f3d4a7b7bc6920225

// TSZ_INLINE_TEST_BEGIN c7aef97fc86a43b6218892bb53f3b72d05f1d15239f776822b878e6d9255823e 224 non_stable_fresh_result_is_returned_but_not_memoized
    #[test]
    fn non_stable_fresh_result_is_returned_but_not_memoized() {
        let session = EvaluationSession::new();
        reset_query_memo(&session);
        let t = TypeId(8);

        let request = EvaluationRequest::new(t);

        let first = memoized_eval(&session, request, || {
            EvaluationMemoResult::unstable_complete(TypeId(80))
        });

        assert_eq!(first, Some(TypeId(80)));
        assert_eq!(query_memo_get(&session, request.cache_key()), None);

        let second = memoized_eval(&session, request, || {
            EvaluationMemoResult::cached(TypeId(81))
        });

        assert_eq!(second, Some(TypeId(81)));
        assert_eq!(
            query_memo_get(&session, request.cache_key()),
            Some(EvaluationResult::complete(TypeId(81)))
        );
        reset_query_memo(&session);
    }
// TSZ_INLINE_TEST_END c7aef97fc86a43b6218892bb53f3b72d05f1d15239f776822b878e6d9255823e

// TSZ_INLINE_TEST_BEGIN 35137868f67d570f1129d6909ac9c90ce2e73408b5cdf47478eb01ba6e9b3a7e 251 unresolved_def_fresh_result_is_returned_and_memoized_within_query
    #[test]
    fn unresolved_def_fresh_result_is_returned_and_memoized_within_query() {
        let session = EvaluationSession::new();
        reset_query_memo(&session);
        let t = TypeId(9);
        let request = EvaluationRequest::new(t);
        let mut calls = 0;

        let first = memoized_eval(&session, request, || {
            calls += 1;
            EvaluationMemoResult::for_depth_agnostic_memo(
                EvaluationResult::complete(TypeId(90)),
                EvaluationRequestStability::UnresolvedDef,
            )
        });

        assert_eq!(first, Some(TypeId(90)));
        assert_eq!(
            query_memo_get(&session, request.cache_key()),
            Some(EvaluationResult::complete(TypeId(90)))
        );

        let second = memoized_eval(&session, request, || {
            calls += 1;
            EvaluationMemoResult::cached(TypeId(91))
        });

        assert_eq!(second, Some(TypeId(90)));
        assert_eq!(calls, 1);
        assert_eq!(
            query_memo_get(&session, request.cache_key()),
            Some(EvaluationResult::complete(TypeId(90)))
        );
        reset_query_memo(&session);
    }
// TSZ_INLINE_TEST_END 35137868f67d570f1129d6909ac9c90ce2e73408b5cdf47478eb01ba6e9b3a7e

// TSZ_INLINE_TEST_BEGIN 028ce7bf42ceebe1350adffcdbdd256d66d34fc1ab70079ed778a13fa03f7244 287 memoized_eval_partitions_by_resolver_generation
    #[test]
    fn memoized_eval_partitions_by_resolver_generation() {
        let session = EvaluationSession::new();
        reset_query_memo(&session);
        let base = EvaluationRequest::new(TypeId(10));
        let gen_one = base.with_resolver_generation(1);
        let gen_two = base.with_resolver_generation(2);
        let mut calls = 0;

        let first = memoized_eval(&session, gen_one, || {
            calls += 1;
            EvaluationMemoResult::cached(TypeId(100))
        });
        let second_same_generation = memoized_eval(&session, gen_one, || {
            calls += 1;
            EvaluationMemoResult::cached(TypeId(101))
        });
        let third_new_generation = memoized_eval(&session, gen_two, || {
            calls += 1;
            EvaluationMemoResult::cached(TypeId(200))
        });

        assert_eq!(first, Some(TypeId(100)));
        assert_eq!(second_same_generation, Some(TypeId(100)));
        assert_eq!(third_new_generation, Some(TypeId(200)));
        assert_eq!(calls, 2);
        assert_eq!(
            query_memo_get(&session, gen_one.cache_key()),
            Some(EvaluationResult::complete(TypeId(100)))
        );
        assert_eq!(
            query_memo_get(&session, gen_two.cache_key()),
            Some(EvaluationResult::complete(TypeId(200)))
        );
        reset_query_memo(&session);
    }
// TSZ_INLINE_TEST_END 028ce7bf42ceebe1350adffcdbdd256d66d34fc1ab70079ed778a13fa03f7244

// TSZ_INLINE_TEST_BEGIN 6d6bce8f7744c29fbefdb57407e2db06911070c6cee1604fc45c70c8ed5440d7 324 reentry_of_active_type_is_rejected
    #[test]
    fn reentry_of_active_type_is_rejected() {
        let session = EvaluationSession::new();
        let t = TypeId(4242);
        let key = EvaluationCacheKey::new(t, false, false);
        let CrossEvalExpansionState::Entered(outer) = CrossEvalExpansionGuard::enter(&session, key)
        else {
            panic!("first entry succeeds");
        };
        assert!(
            matches!(
                CrossEvalExpansionGuard::enter(&session, key),
                CrossEvalExpansionState::AlreadyActive
            ),
            "re-entering an in-flight TypeId must be rejected"
        );
        drop(outer);
        assert!(
            matches!(
                CrossEvalExpansionGuard::enter(&session, key),
                CrossEvalExpansionState::Entered(_)
            ),
            "once the in-flight guard drops, the TypeId is enterable again"
        );
    }
// TSZ_INLINE_TEST_END 6d6bce8f7744c29fbefdb57407e2db06911070c6cee1604fc45c70c8ed5440d7

// TSZ_INLINE_TEST_BEGIN 4e980971cd81c92912c637193aeaa45ad8ff9a8bdddf94bd855e5a3398aa5628 350 active_set_partitions_by_full_request_key
    #[test]
    fn active_set_partitions_by_full_request_key() {
        let session = EvaluationSession::new();
        let base = EvaluationCacheKey::new(TypeId(4243), false, false)
            .with_type_database_identity(1)
            .with_resolver_identity(10)
            .with_resolver_generation(1);
        let different_generation = base.with_resolver_generation(2);
        let different_resolver = base.with_resolver_identity(11);
        let different_arena = base.with_type_database_identity(2);

        let CrossEvalExpansionState::Entered(base_guard) =
            CrossEvalExpansionGuard::enter(&session, base)
        else {
            panic!("base request enters");
        };
        let CrossEvalExpansionState::Entered(generation_guard) =
            CrossEvalExpansionGuard::enter(&session, different_generation)
        else {
            panic!("same TypeId with a different generation enters independently");
        };
        let CrossEvalExpansionState::Entered(resolver_guard) =
            CrossEvalExpansionGuard::enter(&session, different_resolver)
        else {
            panic!("same TypeId with a different resolver enters independently");
        };
        let CrossEvalExpansionState::Entered(arena_guard) =
            CrossEvalExpansionGuard::enter(&session, different_arena)
        else {
            panic!("same TypeId with a different arena enters independently");
        };

        drop(base_guard);
        drop(generation_guard);
        drop(resolver_guard);
        drop(arena_guard);
    }
// TSZ_INLINE_TEST_END 4e980971cd81c92912c637193aeaa45ad8ff9a8bdddf94bd855e5a3398aa5628

// TSZ_INLINE_TEST_BEGIN 0e8b86123d63f8e80de8ac23bd42f470bafb072082f77be6d79a9deb7afdde74 388 distinct_types_are_independent
    #[test]
    fn distinct_types_are_independent() {
        let session = EvaluationSession::new();
        let CrossEvalExpansionState::Entered(a) = CrossEvalExpansionGuard::enter(
            &session,
            EvaluationCacheKey::new(TypeId(1), false, false),
        ) else {
            panic!("a enters");
        };
        let CrossEvalExpansionState::Entered(b) = CrossEvalExpansionGuard::enter(
            &session,
            EvaluationCacheKey::new(TypeId(2), false, false),
        ) else {
            panic!("b enters independently");
        };
        drop(a);
        drop(b);
    }
// TSZ_INLINE_TEST_END 0e8b86123d63f8e80de8ac23bd42f470bafb072082f77be6d79a9deb7afdde74
