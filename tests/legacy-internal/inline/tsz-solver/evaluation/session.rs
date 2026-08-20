//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/evaluation/session.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 033da5d7be647af8c791a196d4df2e3d01e71182c79421e1cca0e5cdcba38ffe 887 test_session_new_has_zero_counters
    #[test]
    fn test_session_new_has_zero_counters() {
        let session = EvaluationSession::new();
        assert_eq!(session.global_instantiation_depth(), 0);
        assert_eq!(session.global_instantiation_fuel(), 0);
        assert_eq!(
            session.instantiation_limit_state(),
            EvaluationSessionLimitState::WithinLimits
        );
        assert!(!session.instantiation_limits_exceeded());
    }
// TSZ_INLINE_TEST_END 033da5d7be647af8c791a196d4df2e3d01e71182c79421e1cca0e5cdcba38ffe

// TSZ_INLINE_TEST_BEGIN 71cbf8286140a87cf9ccdeebf26b8f6e12dabcb84fd0ddbbfdf732cf439592c9 899 test_enter_leave_instantiation
    #[test]
    fn test_enter_leave_instantiation() {
        let session = EvaluationSession::new();
        let prev = session.enter_instantiation();
        assert_eq!(prev, 0);
        assert_eq!(session.global_instantiation_depth(), 1);
        assert_eq!(session.global_instantiation_fuel(), 1);

        session.leave_instantiation();
        assert_eq!(session.global_instantiation_depth(), 0);
        // Fuel does not decrement
        assert_eq!(session.global_instantiation_fuel(), 1);
    }
// TSZ_INLINE_TEST_END 71cbf8286140a87cf9ccdeebf26b8f6e12dabcb84fd0ddbbfdf732cf439592c9

// TSZ_INLINE_TEST_BEGIN 2406dcd4bcaf5b103ad74b2a412e23eff744205a91b3e992a24d3bc33e558d72 913 test_depth_limit_exceeded
    #[test]
    fn test_depth_limit_exceeded() {
        let session = EvaluationSession::new();
        for _ in 0..MAX_GLOBAL_INSTANTIATION_DEPTH {
            session.enter_instantiation();
        }
        assert_eq!(
            session.instantiation_limit_state(),
            EvaluationSessionLimitState::DepthExceeded
        );
        assert!(session.instantiation_limits_exceeded());
    }
// TSZ_INLINE_TEST_END 2406dcd4bcaf5b103ad74b2a412e23eff744205a91b3e992a24d3bc33e558d72

// TSZ_INLINE_TEST_BEGIN dc4679fac41b8e88d22514e1156c7d995c7e24cba272c80760f9d858c5bb28eb 926 test_fuel_limit_exceeded
    #[test]
    fn test_fuel_limit_exceeded() {
        let session = EvaluationSession::new();
        // Enter and leave repeatedly to exhaust fuel without hitting depth limit
        for _ in 0..MAX_GLOBAL_INSTANTIATION_FUEL {
            session.enter_instantiation();
            session.leave_instantiation();
        }
        assert_eq!(
            session.instantiation_limit_state(),
            EvaluationSessionLimitState::FuelExhausted
        );
        assert!(session.instantiation_limits_exceeded());
    }
// TSZ_INLINE_TEST_END dc4679fac41b8e88d22514e1156c7d995c7e24cba272c80760f9d858c5bb28eb

// TSZ_INLINE_TEST_BEGIN 1e57424108f624ba76dd23cd0bb0256535025feba6b11de73260d74d5bfcfe6b 941 test_reset_instantiation_fuel
    #[test]
    fn test_reset_instantiation_fuel() {
        let session = EvaluationSession::new();
        for _ in 0..10 {
            session.enter_instantiation();
            session.leave_instantiation();
        }
        assert_eq!(session.global_instantiation_fuel(), 10);
        session.reset_instantiation_fuel();
        assert_eq!(session.global_instantiation_fuel(), 0);
        assert_eq!(
            session.instantiation_limit_state(),
            EvaluationSessionLimitState::WithinLimits
        );
        assert!(!session.instantiation_limits_exceeded());
    }
// TSZ_INLINE_TEST_END 1e57424108f624ba76dd23cd0bb0256535025feba6b11de73260d74d5bfcfe6b

// TSZ_INLINE_TEST_BEGIN 672acf36d316eb1d47b2474a001e372ffa30b7b0905726cc200cba23786f297c 958 test_lazy_resolution_fuel_snapshot_restore_and_limit
    #[test]
    fn test_lazy_resolution_fuel_snapshot_restore_and_limit() {
        let session = EvaluationSession::new();
        assert_eq!(session.lazy_resolution_fuel_value(), 0);
        assert!(!session.lazy_resolution_fuel_exhausted());

        session.increment_lazy_resolution_fuel();
        assert_eq!(session.lazy_resolution_fuel_value(), 1);

        session.restore_lazy_resolution_fuel(MAX_CHECKER_LAZY_RESOLUTION_FUEL);
        assert!(session.lazy_resolution_fuel_exhausted());

        session.reset_lazy_resolution_fuel();
        assert_eq!(session.lazy_resolution_fuel_value(), 0);
        assert!(!session.lazy_resolution_fuel_exhausted());
    }
// TSZ_INLINE_TEST_END 672acf36d316eb1d47b2474a001e372ffa30b7b0905726cc200cba23786f297c

// TSZ_INLINE_TEST_BEGIN 3c954c67e290cc02e345da5b818896547e5d8eb7ff550dd29a1a2448c183be44 975 checker_eval_env_depth_entry_restores_on_drop_and_rejects_at_cap
    #[test]
    fn checker_eval_env_depth_entry_restores_on_drop_and_rejects_at_cap() {
        let session = EvaluationSession::new();
        let mut entries = Vec::new();
        for expected_prior in 0..MAX_CHECKER_EVAL_ENV_DEPTH {
            let entry = session
                .enter_eval_env_depth()
                .expect("pre-cap env-eval depth entry should fit");
            assert_eq!(entry.prior_depth(), expected_prior);
            entries.push(entry);
        }

        assert_eq!(session.eval_env_depth(), MAX_CHECKER_EVAL_ENV_DEPTH);
        assert!(session.enter_eval_env_depth().is_none());
        while let Some(entry) = entries.pop() {
            drop(entry);
        }
        assert_eq!(session.eval_env_depth(), 0);
    }
// TSZ_INLINE_TEST_END 3c954c67e290cc02e345da5b818896547e5d8eb7ff550dd29a1a2448c183be44

// TSZ_INLINE_TEST_BEGIN 9e86286061b884e95920705ebb427f85cb8cc8bf52258fa96da12b4160653e38 995 checker_app_symbol_resolution_depth_and_fuel_are_session_owned
    #[test]
    fn checker_app_symbol_resolution_depth_and_fuel_are_session_owned() {
        let session = EvaluationSession::new();
        {
            let entry = session.enter_app_symbol_resolution_depth();
            assert!(entry.outermost());
            assert_eq!(session.app_symbol_resolution_depth(), 1);
            let nested = session.enter_app_symbol_resolution_depth();
            assert!(!nested.outermost());
            assert_eq!(session.app_symbol_resolution_depth(), 2);
        }
        assert_eq!(session.app_symbol_resolution_depth(), 0);

        session.increment_app_symbol_resolution_fuel();
        assert_eq!(session.app_symbol_resolution_fuel(), 1);
        session.reset_app_symbol_resolution_fuel();
        assert_eq!(session.app_symbol_resolution_fuel(), 0);
        for _ in 0..MAX_CHECKER_APP_SYMBOL_RESOLUTION_FUEL {
            session.increment_app_symbol_resolution_fuel();
        }
        assert!(session.app_symbol_resolution_fuel_exhausted());
    }
// TSZ_INLINE_TEST_END 9e86286061b884e95920705ebb427f85cb8cc8bf52258fa96da12b4160653e38

// TSZ_INLINE_TEST_BEGIN faefd4525a5d656fe3b016742fa4704987528f5325d8730e904c239d2d77e907 1018 checker_refs_resolution_scope_resets_outermost_fuel_and_restores_active
    #[test]
    fn checker_refs_resolution_scope_resets_outermost_fuel_and_restores_active() {
        let session = EvaluationSession::new();
        {
            let outer = session.enter_refs_resolution_scope();
            assert!(outer.outermost());
            session.increment_refs_resolution_fuel();
            assert_eq!(session.refs_resolution_fuel(), 1);
            {
                let nested = session.enter_refs_resolution_scope();
                assert!(!nested.outermost());
                assert_eq!(session.refs_resolution_fuel(), 1);
            }
            assert_eq!(session.refs_resolution_fuel(), 1);
        }

        let new_outer = session.enter_refs_resolution_scope();
        assert!(new_outer.outermost());
        assert_eq!(
            session.refs_resolution_fuel(),
            0,
            "a new outer refs-resolution scope should reset local prewalk fuel"
        );
        for _ in 0..MAX_CHECKER_REFS_RESOLUTION_FUEL {
            session.increment_refs_resolution_fuel();
        }
        assert!(session.refs_resolution_fuel_exhausted());
    }
// TSZ_INLINE_TEST_END faefd4525a5d656fe3b016742fa4704987528f5325d8730e904c239d2d77e907

// TSZ_INLINE_TEST_BEGIN c942094bafe2971d271747e101d9e7c7d40b59a060e3abb6318c707e615d655f 1047 test_depth_limit_is_primary_when_both_limits_exceeded
    #[test]
    fn test_depth_limit_is_primary_when_both_limits_exceeded() {
        let session = EvaluationSession::new();
        for _ in 0..MAX_GLOBAL_INSTANTIATION_FUEL {
            session.enter_instantiation();
        }

        assert_eq!(
            session.instantiation_limit_state(),
            EvaluationSessionLimitState::DepthExceeded,
            "depth limit should stay the primary session limit once both limits are exceeded"
        );
    }
// TSZ_INLINE_TEST_END c942094bafe2971d271747e101d9e7c7d40b59a060e3abb6318c707e615d655f

// TSZ_INLINE_TEST_BEGIN 6ba9d9216971613cd2b7f6f616a22914c28b0ddfa7fb90df9c6ce1b0d5ed1e72 1061 test_cross_eval_active_set_is_session_owned
    #[test]
    fn test_cross_eval_active_set_is_session_owned() {
        let session = EvaluationSession::new();
        let type_id = TypeId(101);
        let key = EvaluationCacheKey::new(type_id, false, false);
        let distinct_generation = key.with_resolver_generation(1);

        assert!(session.enter_cross_eval_request(key));
        assert!(
            !session.enter_cross_eval_request(key),
            "re-entering the same request in one session should be rejected"
        );
        assert!(
            session.enter_cross_eval_request(distinct_generation),
            "same TypeId under a different request key should enter independently"
        );
        session.leave_cross_eval_request(key);
        session.leave_cross_eval_request(distinct_generation);
        assert!(session.enter_cross_eval_request(key));
    }
// TSZ_INLINE_TEST_END 6ba9d9216971613cd2b7f6f616a22914c28b0ddfa7fb90df9c6ce1b0d5ed1e72

// TSZ_INLINE_TEST_BEGIN 67cf3bda1beaaf0b2c90f61d501a44027e43298d8fd3b2bb39c79319c3db125b 1082 test_query_memo_keys_on_index_access_options
    #[test]
    fn test_query_memo_keys_on_index_access_options() {
        let session = EvaluationSession::new();
        let type_id = TypeId(202);
        let default_key = EvaluationCacheKey::new(type_id, false, false);
        let no_unchecked_key = EvaluationCacheKey::new(type_id, true, false);
        let exact_optional_key = EvaluationCacheKey::new(type_id, false, true);
        let both_key = EvaluationCacheKey::new(type_id, true, true);
        let resolver_key = default_key.with_resolver_generation(7);

        session.query_memo_put(default_key, EvaluationResult::complete(TypeId(210)));
        session.query_memo_put(no_unchecked_key, EvaluationResult::complete(TypeId(211)));
        session.query_memo_put(exact_optional_key, EvaluationResult::complete(TypeId(212)));
        session.query_memo_put(resolver_key, EvaluationResult::complete(TypeId(213)));

        assert_eq!(
            session.query_memo_get(default_key),
            Some(EvaluationResult::complete(TypeId(210)))
        );
        assert_eq!(
            session.query_memo_get(no_unchecked_key),
            Some(EvaluationResult::complete(TypeId(211)))
        );
        assert_eq!(
            session.query_memo_get(exact_optional_key),
            Some(EvaluationResult::complete(TypeId(212)))
        );
        assert_eq!(
            session.query_memo_get(resolver_key),
            Some(EvaluationResult::complete(TypeId(213)))
        );
        assert_eq!(session.query_memo_get(both_key), None);
        assert_eq!(
            session.query_memo_get(default_key.with_resolver_generation(8)),
            None
        );

        session.reset_query_memo();
        assert_eq!(session.query_memo_get(default_key), None);
        assert_eq!(session.query_memo_get(no_unchecked_key), None);
        assert_eq!(session.query_memo_get(exact_optional_key), None);
        assert_eq!(session.query_memo_get(resolver_key), None);
    }
// TSZ_INLINE_TEST_END 67cf3bda1beaaf0b2c90f61d501a44027e43298d8fd3b2bb39c79319c3db125b

// TSZ_INLINE_TEST_BEGIN ebabcb856939229c22d20cbdfd706f5b273bb467b113a66c7f757a7b294f415c 1126 compound_subtype_probe_cache_keys_context_and_resets_with_query_memo
    #[test]
    fn compound_subtype_probe_cache_keys_context_and_resets_with_query_memo() {
        let session = EvaluationSession::new();
        let relation = RelationCacheKey::for_subtype(TypeId(1), TypeId(2), Default::default());
        let key = CompoundSubtypePairKey::new(relation, 10, 20, 30, true, 40);
        let different_arena = CompoundSubtypePairKey::new(relation, 11, 20, 30, true, 40);
        let different_resolver = CompoundSubtypePairKey::new(relation, 10, 21, 30, true, 40);
        let different_generation = CompoundSubtypePairKey::new(relation, 10, 20, 31, true, 40);
        let different_bypass = CompoundSubtypePairKey::new(relation, 10, 20, 30, false, 40);
        let different_depth = CompoundSubtypePairKey::new(relation, 10, 20, 30, true, 41);

        session.compound_subtype_probe_put(key, true);

        assert_eq!(session.compound_subtype_probe_get(key), Some(true));
        assert_eq!(session.compound_subtype_probe_get(different_arena), None);
        assert_eq!(session.compound_subtype_probe_get(different_resolver), None);
        assert_eq!(
            session.compound_subtype_probe_get(different_generation),
            None
        );
        assert_eq!(session.compound_subtype_probe_get(different_bypass), None);
        assert_eq!(session.compound_subtype_probe_get(different_depth), None);
        assert_eq!(session.compound_subtype_probe_cache_entries(), 1);
        assert!(
            session.compound_subtype_probe_cache_estimated_size_bytes() > 0,
            "the compound probe memo should report size visibility when populated",
        );

        session.query_memo_put(
            EvaluationCacheKey::new(TypeId(9), false, false),
            EvaluationResult::complete(TypeId(10)),
        );
        session.reset_query_memo();

        assert_eq!(session.compound_subtype_probe_get(key), None);
        assert_eq!(session.compound_subtype_probe_cache_entries(), 0);
        assert_eq!(
            session.query_memo_get(EvaluationCacheKey::new(TypeId(9), false, false)),
            None
        );
    }
// TSZ_INLINE_TEST_END ebabcb856939229c22d20cbdfd706f5b273bb467b113a66c7f757a7b294f415c

// TSZ_INLINE_TEST_BEGIN 1c11d4f24837e4353ec432b9aeb1a06828964adf134875ff19746d56268c7386 1168 application_expansion_sentinel_defers_at_limit_and_rebalances
    #[test]
    fn application_expansion_sentinel_defers_at_limit_and_rebalances() {
        let session = EvaluationSession::new();
        let node = TypeId(4321);

        let mut entered = 0;
        while session.enter_application_expansion(node) {
            entered += 1;
            assert!(
                entered <= MAX_CROSS_EVAL_APPLICATION_EXPANSION,
                "enter must deny past the in-flight expansion limit"
            );
        }
        assert_eq!(entered, MAX_CROSS_EVAL_APPLICATION_EXPANSION);
        assert!(
            !session.enter_application_expansion(node),
            "an at-limit node must keep deferring until an owner leaves"
        );

        session.leave_application_expansion(node);
        assert!(
            session.enter_application_expansion(node),
            "leaving one expansion frees one re-entry slot"
        );
        for _ in 0..entered {
            session.leave_application_expansion(node);
        }
        assert!(
            session.enter_application_expansion(node),
            "a fully-unwound node is enterable again"
        );
    }
// TSZ_INLINE_TEST_END 1c11d4f24837e4353ec432b9aeb1a06828964adf134875ff19746d56268c7386

// TSZ_INLINE_TEST_BEGIN 6659a23da26e87d9fc3ce833d51c3bc4f43b0a6f9e64e7c4825df56ebfb09a8b 1201 application_expansion_sentinel_tracks_nodes_independently
    #[test]
    fn application_expansion_sentinel_tracks_nodes_independently() {
        let session = EvaluationSession::new();
        let hot = TypeId(11);
        let other = TypeId(12);

        for _ in 0..MAX_CROSS_EVAL_APPLICATION_EXPANSION {
            assert!(session.enter_application_expansion(hot));
        }
        assert!(!session.enter_application_expansion(hot));
        assert!(
            session.enter_application_expansion(other),
            "an at-limit node must not defer expansions of a different node"
        );
    }
// TSZ_INLINE_TEST_END 6659a23da26e87d9fc3ce833d51c3bc4f43b0a6f9e64e7c4825df56ebfb09a8b

// TSZ_INLINE_TEST_BEGIN 12625ddc70a62d5c8944806d39dbc5bcb39a0ee5eea02fbb8a14e2622158b616 1217 conditional_subtype_depth_entry_restores_on_drop
    #[test]
    fn conditional_subtype_depth_entry_restores_on_drop() {
        let session = EvaluationSession::new();
        assert_eq!(session.conditional_subtype_depth(), 0);

        {
            let entry = session.enter_conditional_subtype_depth();
            assert_eq!(entry.prior_depth(), 0);
            assert_eq!(session.conditional_subtype_depth(), 1);
        }

        assert_eq!(session.conditional_subtype_depth(), 0);
    }
// TSZ_INLINE_TEST_END 12625ddc70a62d5c8944806d39dbc5bcb39a0ee5eea02fbb8a14e2622158b616

// TSZ_INLINE_TEST_BEGIN 96a696be6a644eaa3e06f1355edad4e9529ee0f9dbd0e9adc3975dcbfb1265d1 1231 type_reference_resolution_depth_entry_restores_on_drop
    #[test]
    fn type_reference_resolution_depth_entry_restores_on_drop() {
        let session = EvaluationSession::new();
        {
            let _outer = session
                .enter_type_reference_resolution_depth()
                .expect("first type-reference depth entry should fit");
            assert_eq!(session.type_reference_resolution_depth(), 1);
            {
                let _inner = session
                    .enter_type_reference_resolution_depth()
                    .expect("nested type-reference depth entry should fit");
                assert_eq!(session.type_reference_resolution_depth(), 2);
            }
            assert_eq!(session.type_reference_resolution_depth(), 1);
        }
        assert_eq!(session.type_reference_resolution_depth(), 0);
    }
// TSZ_INLINE_TEST_END 96a696be6a644eaa3e06f1355edad4e9529ee0f9dbd0e9adc3975dcbfb1265d1

// TSZ_INLINE_TEST_BEGIN aaf66aa6dc540f88e5492a828b33dae0199930e8e11c37ebc1109a660255a7d6 1250 type_reference_resolution_depth_rejects_at_cap_without_mutating_depth
    #[test]
    fn type_reference_resolution_depth_rejects_at_cap_without_mutating_depth() {
        let session = EvaluationSession::new();
        let mut entries = Vec::new();
        for _ in 0..crate::limits::MAX_TYPE_REFERENCE_RESOLUTION_DEPTH {
            entries.push(
                session
                    .enter_type_reference_resolution_depth()
                    .expect("pre-cap entry should fit"),
            );
        }

        assert_eq!(
            session.type_reference_resolution_depth(),
            crate::limits::MAX_TYPE_REFERENCE_RESOLUTION_DEPTH
        );
        assert!(session.enter_type_reference_resolution_depth().is_none());
        assert_eq!(
            session.type_reference_resolution_depth(),
            crate::limits::MAX_TYPE_REFERENCE_RESOLUTION_DEPTH
        );
        drop(entries);
        assert_eq!(session.type_reference_resolution_depth(), 0);
    }
// TSZ_INLINE_TEST_END aaf66aa6dc540f88e5492a828b33dae0199930e8e11c37ebc1109a660255a7d6

// TSZ_INLINE_TEST_BEGIN d6ae4b4fa18c1adaf9f89130b0b3cff1583ed887708ae3f9de472656ebf0acbc 1275 effective_variance_cache_partitions_options_replaces_generation_and_resets
    #[test]
    fn effective_variance_cache_partitions_options_replaces_generation_and_resets() {
        let session = EvaluationSession::new();
        let strict_key = EffectiveVarianceCacheKey::new(DefId(1), true, false, 10, 20);
        let loose_key = EffectiveVarianceCacheKey::new(DefId(1), false, false, 10, 20);
        let identity_key = EffectiveVarianceCacheKey::new(DefId(1), true, true, 10, 20);
        let covariant: Arc<[Variance]> = Arc::from([Variance::COVARIANT]);
        let contravariant: Arc<[Variance]> = Arc::from([Variance::CONTRAVARIANT]);

        session.effective_variance_put(strict_key, 1, covariant.clone(), false);
        assert_eq!(
            session.effective_variance_get(strict_key, 1),
            Some((covariant, false))
        );
        assert_eq!(session.effective_variance_get(strict_key, 2), None);

        session.effective_variance_put(strict_key, 2, contravariant.clone(), true);
        assert_eq!(
            session.effective_variance_get(strict_key, 2),
            Some((contravariant.clone(), true))
        );
        assert_eq!(
            session.effective_variance_cache_entries(),
            1,
            "a new resolver generation must replace the stale scope value"
        );

        session.effective_variance_put(loose_key, 2, contravariant, false);
        session.effective_variance_put(identity_key, 2, Arc::from([Variance::COVARIANT]), false);
        assert_eq!(session.effective_variance_cache_entries(), 3);
        assert!(session.effective_variance_cache_estimated_size_bytes() > 0);

        session.reset_query_memo();
        assert_eq!(session.effective_variance_cache_entries(), 0);
        assert_eq!(session.effective_variance_get(strict_key, 2), None);
        assert_eq!(session.effective_variance_get(loose_key, 2), None);
        assert_eq!(session.effective_variance_get(identity_key, 2), None);
    }
// TSZ_INLINE_TEST_END d6ae4b4fa18c1adaf9f89130b0b3cff1583ed887708ae3f9de472656ebf0acbc
