//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/evaluation/evaluate/application.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 197bb57853d4e9dbfba67207519342fbe6a7493bcb71a6d15a1a12ab874690bd 1892 unresolved_def_body_blocks_concrete_fixpoint_application_cache_write
    #[test]
    fn unresolved_def_body_blocks_concrete_fixpoint_application_cache_write() {
        let types = TypeInterner::new();
        let query_cache = QueryCache::new(&types);
        let mut evaluator = TypeEvaluator::new(&types).with_query_db(&query_cache);
        let def_id = DefId(901_001);
        let args = [TypeId::STRING];

        evaluator.app_body_limit_epoch = evaluator.limit_epoch;
        evaluator.app_body_unresolved_def_epoch = evaluator.unresolved_def_epoch;
        evaluator.mark_unresolved_def_seen();
        evaluator.insert_application_eval_cache_if_some(def_id, &args, false, TypeId::NEVER);

        assert_eq!(
            query_cache.lookup_application_eval_cache(def_id, &args, false),
            None,
            "a concrete-looking result computed after a registration-window unresolved def \
             must not enter the application_eval_cache",
        );
    }
// TSZ_INLINE_TEST_END 197bb57853d4e9dbfba67207519342fbe6a7493bcb71a6d15a1a12ab874690bd

// TSZ_INLINE_TEST_BEGIN 672dc9891fa37417b6b9d34d3ddf3b3f5b0ca2bc74dcd90d13ffa75f325c5de6 1913 prior_unresolved_def_does_not_block_later_clean_application_cache_write
    #[test]
    fn prior_unresolved_def_does_not_block_later_clean_application_cache_write() {
        let types = TypeInterner::new();
        let query_cache = QueryCache::new(&types);
        let mut evaluator = TypeEvaluator::new(&types).with_query_db(&query_cache);
        let def_id = DefId(901_002);
        let args = [TypeId::NUMBER];

        evaluator.mark_unresolved_def_seen();
        evaluator.app_body_limit_epoch = evaluator.limit_epoch;
        evaluator.app_body_unresolved_def_epoch = evaluator.unresolved_def_epoch;
        evaluator.insert_application_eval_cache_if_some(def_id, &args, false, TypeId::STRING);

        assert_eq!(
            query_cache.lookup_application_eval_cache(def_id, &args, false),
            Some(TypeId::STRING),
            "the unresolved-def epoch is per application body, not a sticky global \
             application_eval_cache disable",
        );
    }
// TSZ_INLINE_TEST_END 672dc9891fa37417b6b9d34d3ddf3b3f5b0ca2bc74dcd90d13ffa75f325c5de6

// TSZ_INLINE_TEST_BEGIN 49ac21b8c3e3374615aaf0e72973ee9f88e785633532c97abaf9bfbe10413fd8 1934 expanded_application_display_alias_containment_uses_shared_memo
    #[test]
    fn expanded_application_display_alias_containment_uses_shared_memo() {
        let types = TypeInterner::new();
        let prop = types.intern_string("x");
        let object = types.object(vec![PropertyInfo::new(prop, TypeId::NUMBER)]);
        let key = types.literal_string("x");
        let index_access = types.index_access(object, key);
        let base = types.lazy(DefId(901_010));
        let original = types.application(base, vec![index_access]);
        let expanded_candidate = types.application(base, vec![TypeId::NUMBER]);

        let mut evaluator =
            TypeEvaluator::new(&types).with_expanded_application_display_alias_args();

        assert!(crate::visitor::contains_type_by_id(
            &types,
            expanded_candidate,
            TypeId::NUMBER
        ));
        assert_eq!(
            types.contains_type_by_id_memo(expanded_candidate, TypeId::NUMBER),
            None
        );

        evaluator.record_application_evaluation_display_aliases(
            TypeId::NUMBER,
            original,
            &[index_access],
            true,
            false,
            None,
        );

        assert_eq!(
            types.contains_type_by_id_memo(expanded_candidate, TypeId::NUMBER),
            Some(true)
        );
        assert!(
            TypeEvaluator::new(&types)
                .cached_contains_type_by_id(expanded_candidate, TypeId::NUMBER)
        );
        assert_eq!(
            types
                .type_predicate_cache_statistics()
                .contains_type_by_id_cache_entries,
            1
        );
    }
// TSZ_INLINE_TEST_END 49ac21b8c3e3374615aaf0e72973ee9f88e785633532c97abaf9bfbe10413fd8
