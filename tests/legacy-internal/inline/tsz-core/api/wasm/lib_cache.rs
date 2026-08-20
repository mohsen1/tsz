//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-core/src/api/wasm/lib_cache.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN ec46d64b4000ba415ee3af3d591f38e8dd42222c07bf195b4b757ea6f5565e04 122 lib_file_cache_statistics_track_entries_hits_and_misses
    #[test]
    fn lib_file_cache_statistics_track_entries_hits_and_misses() {
        clear_lib_file_cache_for_test();

        let first = get_or_create_lib_file(
            "lib.test.d.ts".to_string(),
            "interface Array<T> { length: number; }\n".to_string(),
        );
        let after_first = lib_file_cache_statistics();
        assert_eq!(after_first.entries, 1);
        assert_eq!(after_first.hits, 0);
        assert_eq!(after_first.misses, 1);
        assert!(after_first.estimated_size_bytes() > 0);

        let second = get_or_create_lib_file(
            "lib.test.d.ts".to_string(),
            "interface Array<T> { length: number; }\n".to_string(),
        );
        let after_second = lib_file_cache_statistics();
        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(after_second.entries, 1);
        assert_eq!(after_second.hits, 1);
        assert_eq!(after_second.misses, 1);
        let json: serde_json::Value =
            serde_json::from_str(&lib_file_cache_statistics_json()).unwrap();
        assert_eq!(json["entries"], 1);
        assert_eq!(json["hits"], 1);
        assert_eq!(json["misses"], 1);
        assert!(json["estimatedSizeBytes"].as_u64().unwrap() > 0);

        let third = get_or_create_lib_file(
            "lib.test.d.ts".to_string(),
            "interface Array<T> { readonly length: number; }\n".to_string(),
        );
        let after_third = lib_file_cache_statistics();
        assert!(!Arc::ptr_eq(&first, &third));
        assert_eq!(after_third.entries, 2);
        assert_eq!(after_third.hits, 1);
        assert_eq!(after_third.misses, 2);
    }
// TSZ_INLINE_TEST_END ec46d64b4000ba415ee3af3d591f38e8dd42222c07bf195b4b757ea6f5565e04
