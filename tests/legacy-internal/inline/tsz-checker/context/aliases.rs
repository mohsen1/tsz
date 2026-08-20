//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/context/aliases.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 087a236f90473d5da038b6f5e0cdaa93e078ebbe6ec021cdd4f8f953706a5224 348 namespace_exports_cache_statistics_report_entries_and_size
    #[test]
    fn namespace_exports_cache_statistics_report_entries_and_size() {
        let mut table = SymbolTable::new();
        table.set("Exported".to_string(), SymbolId(7));

        let mut cache = NamespaceExportsCache::default();
        assert_eq!(namespace_exports_cache_entries(&cache), 0);
        assert_eq!(namespace_exports_cache_estimated_size_bytes(&cache), 0);

        cache.insert((1, "pkg".to_string()), Some(table));
        cache.insert((2, "missing".to_string()), None);

        assert_eq!(namespace_exports_cache_entries(&cache), 2);
        assert!(
            namespace_exports_cache_estimated_size_bytes(&cache)
                >= 2 * (std::mem::size_of::<(usize, String)>()
                    + std::mem::size_of::<Option<SymbolTable>>())
        );
    }
// TSZ_INLINE_TEST_END 087a236f90473d5da038b6f5e0cdaa93e078ebbe6ec021cdd4f8f953706a5224

// TSZ_INLINE_TEST_BEGIN c0d9f47c88438021c0ba244a6aa6d67dc31683428d1be1fb44800c64564831c8 368 file_session_alias_caches_report_entries_and_size
    #[test]
    fn file_session_alias_caches_report_entries_and_size() {
        let accessor_cache = AccessorLevelsCache::default();
        assert_eq!(accessor_levels_cache_entries(&accessor_cache), 0);
        assert_eq!(
            accessor_levels_cache_estimated_size_bytes(&accessor_cache),
            0
        );
        accessor_cache
            .borrow_mut()
            .insert((NodeIndex(1), Atom(2), false), None);
        assert_eq!(accessor_levels_cache_entries(&accessor_cache), 1);
        assert!(accessor_levels_cache_estimated_size_bytes(&accessor_cache) > 0);

        let member_cache = MemberAccessInfoCache::default();
        assert_eq!(member_access_info_cache_entries(&member_cache), 0);
        assert_eq!(
            member_access_info_cache_estimated_size_bytes(&member_cache),
            0
        );
        member_cache
            .borrow_mut()
            .insert((NodeIndex(3), Atom(4), true), None);
        assert_eq!(member_access_info_cache_entries(&member_cache), 1);
        assert!(member_access_info_cache_estimated_size_bytes(&member_cache) > 0);

        let mut callback_cache = CallbackMismatchMemo::default();
        assert_eq!(callback_mismatch_memo_entries(&callback_cache), 0);
        assert_eq!(
            callback_mismatch_memo_estimated_size_bytes(&callback_cache),
            0
        );
        callback_cache.insert(
            (NodeIndex(5), TypeId::STRING),
            Some((1, TypeId::NUMBER, TypeId::BOOLEAN)),
        );
        assert_eq!(callback_mismatch_memo_entries(&callback_cache), 1);
        assert!(callback_mismatch_memo_estimated_size_bytes(&callback_cache) > 0);
    }
// TSZ_INLINE_TEST_END c0d9f47c88438021c0ba244a6aa6d67dc31683428d1be1fb44800c64564831c8

// TSZ_INLINE_TEST_BEGIN 9049f9a0e9c357f966d474501e1f659ef7b68995e04f97b342c781b00f404536 408 retained_alias_caches_report_entries_and_size
    #[test]
    fn retained_alias_caches_report_entries_and_size() {
        let mut flow_cache = FlowAnalysisCacheMap::default();
        assert_eq!(flow_analysis_cache_map_entries(&flow_cache), 0);
        assert_eq!(flow_analysis_cache_map_estimated_size_bytes(&flow_cache), 0);
        flow_cache.insert(
            (tsz_binder::FlowNodeId(1), SymbolId(2), TypeId::STRING),
            TypeId::NUMBER,
        );
        assert_eq!(flow_analysis_cache_map_entries(&flow_cache), 1);
        assert!(flow_analysis_cache_map_estimated_size_bytes(&flow_cache) > 0);

        let mut reexport_cache = ReexportResolutionCache::default();
        assert_eq!(reexport_resolution_cache_entries(&reexport_cache), 0);
        assert_eq!(
            reexport_resolution_cache_estimated_size_bytes(&reexport_cache),
            0
        );
        reexport_cache.insert((3, "value".to_string()), Some((SymbolId(4), 5)));
        assert_eq!(reexport_resolution_cache_entries(&reexport_cache), 1);
        assert!(reexport_resolution_cache_estimated_size_bytes(&reexport_cache) > 0);
    }
// TSZ_INLINE_TEST_END 9049f9a0e9c357f966d474501e1f659ef7b68995e04f97b342c781b00f404536

// TSZ_INLINE_TEST_BEGIN 98d47682616760d09b5509f91e246b5fbf4c8bc54882b2347db0c7f1e13a829e 431 export_equals_named_cache_statistics_report_entries_and_size
    #[test]
    fn export_equals_named_cache_statistics_report_entries_and_size() {
        let mut cache = ExportEqualsNamedCache::default();
        assert_eq!(export_equals_named_cache_entries(&cache), 0);
        assert_eq!(export_equals_named_cache_estimated_size_bytes(&cache), 0);

        cache.insert(
            (1, "pkg".to_string(), "foo".to_string(), vec![]),
            Some(SymbolId(3)),
        );
        cache.insert(
            (1, "pkg".to_string(), "bar".to_string(), vec![SymbolId(7)]),
            None,
        );

        assert_eq!(export_equals_named_cache_entries(&cache), 2);
        assert!(
            export_equals_named_cache_estimated_size_bytes(&cache)
                >= 2 * (std::mem::size_of::<(usize, String, String, Vec<SymbolId>)>()
                    + std::mem::size_of::<Option<SymbolId>>())
        );
    }
// TSZ_INLINE_TEST_END 98d47682616760d09b5509f91e246b5fbf4c8bc54882b2347db0c7f1e13a829e

// TSZ_INLINE_TEST_BEGIN 6ad56c73551ee4c863057d79dd8593a8b540db942d0ffee3d5de7dc7bec18a7f 454 nested_namespace_candidates_cache_statistics_report_entries_and_size
    #[test]
    fn nested_namespace_candidates_cache_statistics_report_entries_and_size() {
        let mut cache = NestedNamespaceCandidatesCache::default();
        assert_eq!(nested_namespace_candidates_cache_entries(&cache), 0);
        assert_eq!(
            nested_namespace_candidates_cache_estimated_size_bytes(&cache),
            0
        );

        cache.insert("A.B".to_string(), vec![(1, SymbolId(2)), (3, SymbolId(4))]);
        cache.insert("C.D".to_string(), vec![(5, SymbolId(6))]);

        assert_eq!(nested_namespace_candidates_cache_entries(&cache), 2);
        assert!(
            nested_namespace_candidates_cache_estimated_size_bytes(&cache)
                >= 3 * std::mem::size_of::<(usize, SymbolId)>()
        );
    }
// TSZ_INLINE_TEST_END 6ad56c73551ee4c863057d79dd8593a8b540db942d0ffee3d5de7dc7bec18a7f

// TSZ_INLINE_TEST_BEGIN 51b696252fa7f2079a5ddc799e46a73320e5eacc5f6bd677b7f722dd608dd3f4 473 namespace_member_resolution_cache_statistics_report_entries_and_size
    #[test]
    fn namespace_member_resolution_cache_statistics_report_entries_and_size() {
        let mut cache = NamespaceMemberResolutionCache::default();
        assert_eq!(namespace_member_resolution_cache_entries(&cache), 0);
        assert_eq!(
            namespace_member_resolution_cache_estimated_size_bytes(&cache),
            0
        );

        let mut pkg_members = FxHashMap::default();
        pkg_members.insert("foo".to_string(), Some(SymbolId(1)));
        pkg_members.insert("missing".to_string(), None);
        let mut other_members = FxHashMap::default();
        other_members.insert("bar".to_string(), Some(SymbolId(2)));
        cache.insert("pkg".to_string(), pkg_members);
        cache.insert("other".to_string(), other_members);

        assert_eq!(namespace_member_resolution_cache_entries(&cache), 3);
        assert!(
            namespace_member_resolution_cache_estimated_size_bytes(&cache)
                >= 3 * (std::mem::size_of::<String>() + std::mem::size_of::<Option<SymbolId>>())
        );
    }
// TSZ_INLINE_TEST_END 51b696252fa7f2079a5ddc799e46a73320e5eacc5f6bd677b7f722dd608dd3f4
