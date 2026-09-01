//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/context/cache_statistics.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN ed013013b4ad43d163ce0699963004c05de40d23d709bffc7e0ca25979f2a122 616 type_param_node_cache_statistics_report_entries_and_size
    #[test]
    fn type_param_node_cache_statistics_report_entries_and_size() {
        let mut cache = FxHashMap::default();
        assert_eq!(type_param_node_cache_estimated_size_bytes(&cache), 0);

        cache.insert((7, TypeParamInfo::simple(Atom(1))), TypeId::STRING);

        assert_eq!(cache.len(), 1);
        assert!(type_param_node_cache_estimated_size_bytes(&cache) > 0);
    }
// TSZ_INLINE_TEST_END ed013013b4ad43d163ce0699963004c05de40d23d709bffc7e0ca25979f2a122

// TSZ_INLINE_TEST_BEGIN 1ff4293923a0032d8789cce1dbaac925e648f0fc8ef9d6e4705c37ffee9e6fe2 627 suggestion_scan_cache_statistics_report_entries_and_size
    #[test]
    fn suggestion_scan_cache_statistics_report_entries_and_size() {
        let mut cache = FxHashMap::default();
        assert_eq!(scoped_name_string_vec_cache_estimated_size_bytes(&cache), 0);

        cache.insert(
            (ScopeId(7), 1),
            FxHashMap::from_iter([("misspelled".to_string(), vec!["candidate".to_string()])]),
        );

        assert_eq!(cache.len(), 1);
        assert!(scoped_name_string_vec_cache_estimated_size_bytes(&cache) > 0);
    }
// TSZ_INLINE_TEST_END 1ff4293923a0032d8789cce1dbaac925e648f0fc8ef9d6e4705c37ffee9e6fe2

// TSZ_INLINE_TEST_BEGIN 0f46233491b2447ef862781f5313795f7e260563524f3532e5e6c62fe9a8caf5 641 spelling_candidate_cache_statistics_report_entries_and_size
    #[test]
    fn spelling_candidate_cache_statistics_report_entries_and_size() {
        let mut cache = FxHashMap::default();
        assert_eq!(scoped_string_slice_cache_estimated_size_bytes(&cache), 0);

        cache.insert(
            (ScopeId(7), 1),
            Rc::<[String]>::from(vec!["candidate".to_string()]),
        );

        assert_eq!(cache.len(), 1);
        assert!(scoped_string_slice_cache_estimated_size_bytes(&cache) > 0);
    }
// TSZ_INLINE_TEST_END 0f46233491b2447ef862781f5313795f7e260563524f3532e5e6c62fe9a8caf5

// TSZ_INLINE_TEST_BEGIN 6b4f59a49934155e9368b243c1abd7b9a2fa0899c7617044a7048ce0f4657aec 655 checker_context_cache_statistics_roll_up_diagnostic_and_switch_caches
    #[test]
    fn checker_context_cache_statistics_roll_up_diagnostic_and_switch_caches() {
        let stats = CheckerContextCacheStatistics {
            spelling_candidate_cache_entries: 2,
            spelling_candidate_cache_estimated_size_bytes: 3,
            suggestion_scan_cache_entries: 1,
            suggestion_scan_cache_estimated_size_bytes: 2,
            flow_switch_case_literal_cache_entries: 4,
            flow_switch_case_literal_cache_estimated_size_bytes: 8,
            flow_switch_all_distinct_literals_cache_entries: 16,
            flow_switch_all_distinct_literals_cache_estimated_size_bytes: 32,
            ..CheckerContextCacheStatistics::default()
        };

        assert_eq!(stats.entries(), 23);
        assert_eq!(stats.estimated_size_bytes(), 45);
    }
// TSZ_INLINE_TEST_END 6b4f59a49934155e9368b243c1abd7b9a2fa0899c7617044a7048ce0f4657aec
