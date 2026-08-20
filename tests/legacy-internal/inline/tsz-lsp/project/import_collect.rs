//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-lsp/src/project/import_collect.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 62d6cf3d2b9d71a6ae7b401e57b38c888b1298efb8d39bbc18c8b3a78e65a46e 243 module_specifiers_cache_statistics_report_entries_and_size
    #[test]
    fn module_specifiers_cache_statistics_report_entries_and_size() {
        let mut cache = FxHashMap::default();
        assert_eq!(module_specifiers_cache_estimated_size_bytes(&cache), 0);

        cache.insert(
            "/workspace/src/source.ts".to_string(),
            vec!["./source".to_string(), "pkg/source".to_string()],
        );

        assert_eq!(cache.len(), 1);
        assert!(module_specifiers_cache_estimated_size_bytes(&cache) > 0);
    }
// TSZ_INLINE_TEST_END 62d6cf3d2b9d71a6ae7b401e57b38c888b1298efb8d39bbc18c8b3a78e65a46e
