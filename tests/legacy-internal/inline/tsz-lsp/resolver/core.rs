//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-lsp/src/resolver/core.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 5a35ffa8a8878b3a23cd8a8bf35b52506c1d57c843302014c4d298bf666e41c5 1009 scope_cache_statistics_report_entries_and_size
    #[test]
    fn scope_cache_statistics_report_entries_and_size() {
        let mut cache = ScopeCache::default();
        assert_eq!(scope_cache_entries(&cache), 0);
        assert_eq!(scope_cache_estimated_size_bytes(&cache), 0);

        cache.insert(1, vec![SymbolTable::new(), SymbolTable::new()]);
        cache.insert(2, vec![SymbolTable::new()]);

        assert_eq!(scope_cache_entries(&cache), 2);
        assert!(scope_cache_estimated_size_bytes(&cache) >= 3 * std::mem::size_of::<SymbolTable>());
    }
// TSZ_INLINE_TEST_END 5a35ffa8a8878b3a23cd8a8bf35b52506c1d57c843302014c4d298bf666e41c5
