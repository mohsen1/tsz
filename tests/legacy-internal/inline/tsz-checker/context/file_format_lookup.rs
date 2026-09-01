//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/context/file_format_lookup.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 5b24bef9eb26691f8e33cd26ea44ef0f7b6d9cf05384992f7e6cea334ff662d3 95 empty_map_returns_none
    #[test]
    fn empty_map_returns_none() {
        let map = FxHashMap::default();
        assert_eq!(lookup_file_is_esm_in_map(&map, "/proj/foo.ts"), None);
    }
// TSZ_INLINE_TEST_END 5b24bef9eb26691f8e33cd26ea44ef0f7b6d9cf05384992f7e6cea334ff662d3

// TSZ_INLINE_TEST_BEGIN de6c51e1dbe212594055cfe70b136aded0214ad493aeb091f77865f0a0833df5 101 direct_hit_returns_value
    #[test]
    fn direct_hit_returns_value() {
        let map = map_of(&[("/proj/foo.ts", true), ("/proj/bar.ts", false)]);
        assert_eq!(lookup_file_is_esm_in_map(&map, "/proj/foo.ts"), Some(true));
        assert_eq!(lookup_file_is_esm_in_map(&map, "/proj/bar.ts"), Some(false));
    }
// TSZ_INLINE_TEST_END de6c51e1dbe212594055cfe70b136aded0214ad493aeb091f77865f0a0833df5

// TSZ_INLINE_TEST_BEGIN e1417b567f113817ce59bac5a5b4e0e6bdfc33551d3f69e2749f0ca551b024c3 110 backslash_query_hits_normalized_key
    /// Query with backslashes is normalized to forward slashes before lookup.
    /// Map keys are always forward slashes (normalized at insertion by the driver).
    #[test]
    fn backslash_query_hits_normalized_key() {
        let map = map_of(&[("/proj/foo.ts", true)]);
        assert_eq!(
            lookup_file_is_esm_in_map(&map, "\\proj\\foo.ts"),
            Some(true)
        );
    }
// TSZ_INLINE_TEST_END e1417b567f113817ce59bac5a5b4e0e6bdfc33551d3f69e2749f0ca551b024c3

// TSZ_INLINE_TEST_BEGIN 1389c63b35da86dc748b7178d63de142895fe7bb105d7cee00c566091dad990b 119 no_match_returns_none
    #[test]
    fn no_match_returns_none() {
        let map = map_of(&[("/proj/foo.ts", true)]);
        assert_eq!(lookup_file_is_esm_in_map(&map, "/proj/bar.ts"), None);
    }
// TSZ_INLINE_TEST_END 1389c63b35da86dc748b7178d63de142895fe7bb105d7cee00c566091dad990b

// TSZ_INLINE_TEST_BEGIN 066a66b021614a02a405e535e8295654a7f140988d908bb6622f6ed94c0bc32f 128 basename_keys_match_basename_queries
    /// Test helpers in `conformance_issues/modules/context.rs` key the map
    /// with bare basenames (`mod.cts`, `b.mts`); the checker queries with the
    /// same names. Direct lookup must still work.
    #[test]
    fn basename_keys_match_basename_queries() {
        let map = map_of(&[("mod.cts", false), ("b.mts", true)]);
        assert_eq!(lookup_file_is_esm_in_map(&map, "mod.cts"), Some(false));
        assert_eq!(lookup_file_is_esm_in_map(&map, "b.mts"), Some(true));
    }
// TSZ_INLINE_TEST_END 066a66b021614a02a405e535e8295654a7f140988d908bb6622f6ed94c0bc32f

// TSZ_INLINE_TEST_BEGIN 0aec274add8e997e2714f9a6c7e17e448d642648cd5a2f0322bedd56d4469560 135 normalize_path_key_converts_backslashes
    #[test]
    fn normalize_path_key_converts_backslashes() {
        assert_eq!(normalize_path_key("C:\\proj\\foo.ts"), "C:/proj/foo.ts");
        assert_eq!(normalize_path_key("/proj/foo.ts"), "/proj/foo.ts");
        assert_eq!(normalize_path_key("mod.cts"), "mod.cts");
    }
// TSZ_INLINE_TEST_END 0aec274add8e997e2714f9a6c7e17e448d642648cd5a2f0322bedd56d4469560

// TSZ_INLINE_TEST_BEGIN 57a449e5a1f1335e5da3071c069cc0bc5602b793410775b4c833b672b470e297 142 lookup_is_external_module_forward_slash_hit
    #[test]
    fn lookup_is_external_module_forward_slash_hit() {
        let map = map_of(&[("/proj/mod.ts", true), ("/proj/script.ts", false)]);
        assert_eq!(
            lookup_is_external_module_in_map(&map, "/proj/mod.ts"),
            Some(true)
        );
        assert_eq!(
            lookup_is_external_module_in_map(&map, "/proj/script.ts"),
            Some(false)
        );
    }
// TSZ_INLINE_TEST_END 57a449e5a1f1335e5da3071c069cc0bc5602b793410775b4c833b672b470e297

// TSZ_INLINE_TEST_BEGIN 93ce4082eece6fc640b57d9e2200c8bb22b2b8cc1576423a4f5f8519cf30e8c8 158 lookup_is_external_module_backslash_query_hits_normalized_key
    /// When a file name is stored with forward slashes (driver normalizes at
    /// insertion) but the query arrives with backslashes (Windows path), the
    /// helper must still find the entry.
    #[test]
    fn lookup_is_external_module_backslash_query_hits_normalized_key() {
        let map = map_of(&[("/proj/mod.ts", true)]);
        assert_eq!(
            lookup_is_external_module_in_map(&map, "\\proj\\mod.ts"),
            Some(true)
        );
    }
// TSZ_INLINE_TEST_END 93ce4082eece6fc640b57d9e2200c8bb22b2b8cc1576423a4f5f8519cf30e8c8

// TSZ_INLINE_TEST_BEGIN adeeb3fb79b3c9b386e25eab0c641df8671803f774f758af383a4f4e6786f534 167 lookup_is_external_module_missing_returns_none
    #[test]
    fn lookup_is_external_module_missing_returns_none() {
        let map = map_of(&[("/proj/mod.ts", true)]);
        assert_eq!(
            lookup_is_external_module_in_map(&map, "/proj/other.ts"),
            None
        );
    }
// TSZ_INLINE_TEST_END adeeb3fb79b3c9b386e25eab0c641df8671803f774f758af383a4f4e6786f534

// TSZ_INLINE_TEST_BEGIN de39e5c7673aacc6494c21a69c6a341155daeab1dd2c7eb7c9bade722c98645b 176 lookup_is_external_module_empty_map_returns_none
    #[test]
    fn lookup_is_external_module_empty_map_returns_none() {
        let map: FxHashMap<String, bool> = FxHashMap::default();
        assert_eq!(lookup_is_external_module_in_map(&map, "/proj/mod.ts"), None);
    }
// TSZ_INLINE_TEST_END de39e5c7673aacc6494c21a69c6a341155daeab1dd2c7eb7c9bade722c98645b
