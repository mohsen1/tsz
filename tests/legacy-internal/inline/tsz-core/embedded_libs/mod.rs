//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-core/src/embedded_libs/mod.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 367622bea4e568cafeafa9c2f0608ccc82c0e19766e4f4fb99d78215282e159f 683 known_embedded_lib_lookup
    #[test]
    fn known_embedded_lib_lookup() {
        let content = get_lib_content("es5.d.ts").expect("es5.d.ts should be embedded");
        assert!(is_embedded_lib("es5.d.ts"));
        assert!(!content.is_empty());
    }
// TSZ_INLINE_TEST_END 367622bea4e568cafeafa9c2f0608ccc82c0e19766e4f4fb99d78215282e159f

// TSZ_INLINE_TEST_BEGIN 02dc04fab78a0f356e2ddfe87e723b0bd962325c9c13ae6f6f9567a008468f2d 690 unknown_embedded_lib_lookup
    #[test]
    fn unknown_embedded_lib_lookup() {
        assert!(get_lib_content("not-a-lib.d.ts").is_none());
        assert!(!is_embedded_lib("not-a-lib.d.ts"));
    }
// TSZ_INLINE_TEST_END 02dc04fab78a0f356e2ddfe87e723b0bd962325c9c13ae6f6f9567a008468f2d

// TSZ_INLINE_TEST_BEGIN c238b98b4947db0bfa9c92f13a11807bfa1b11057492f60548394b8bb8166fd5 696 all_lib_filenames_count_matches_constant_and_are_unique
    #[test]
    fn all_lib_filenames_count_matches_constant_and_are_unique() {
        let mut filenames: Vec<_> = all_lib_filenames().collect();
        filenames.sort_unstable();
        assert_eq!(filenames.len(), LIB_FILE_COUNT);
        let mut deduped = filenames.clone();
        deduped.dedup();
        assert_eq!(deduped, filenames);
    }
// TSZ_INLINE_TEST_END c238b98b4947db0bfa9c92f13a11807bfa1b11057492f60548394b8bb8166fd5

// TSZ_INLINE_TEST_BEGIN 23d784713d0a79e2c9541adacd9763f433882f93032a9b093f6909a58e2fe244 706 embedded_content_hashes_cover_all_entries
    #[test]
    fn embedded_content_hashes_cover_all_entries() {
        for filename in all_lib_filenames() {
            assert!(is_embedded_lib(filename), "{filename} should be recognized");
            assert!(
                get_lib_content_hash(filename).is_some(),
                "{filename} should have a hash"
            );
            assert!(
                get_lib_references(filename).is_some(),
                "{filename} should have references"
            );
        }
    }
// TSZ_INLINE_TEST_END 23d784713d0a79e2c9541adacd9763f433882f93032a9b093f6909a58e2fe244

// TSZ_INLINE_TEST_BEGIN f9b4c63511ae9ef76c16ac9822763adf3f2f7d03dd03c6b2577e733b4f0112df 721 embedded_lib_references_resolve_to_embedded_assets
    #[test]
    fn embedded_lib_references_resolve_to_embedded_assets() {
        for filename in all_lib_filenames() {
            for ref_lib in get_embedded_lib_references(filename) {
                let embedded_name = embedded_reference_filename(ref_lib);
                assert!(
                    is_embedded_lib(&embedded_name),
                    "{filename} reference {ref_lib} should resolve to embedded {embedded_name}"
                );
            }
        }
    }
// TSZ_INLINE_TEST_END f9b4c63511ae9ef76c16ac9822763adf3f2f7d03dd03c6b2577e733b4f0112df

// TSZ_INLINE_TEST_BEGIN d7a0ab730aa21563680352f37d632c00adbff00e1d140de876e956f653559c90 734 esnext_temporal_family_is_embedded
    #[test]
    fn esnext_temporal_family_is_embedded() {
        let date = get_lib_content("esnext.date.d.ts").expect("esnext.date.d.ts");
        let temporal = get_lib_content("esnext.temporal.d.ts").expect("esnext.temporal.d.ts");
        assert!(date.contains("toTemporalInstant"));
        assert!(temporal.contains("namespace Temporal"));
        assert!(temporal.contains("ZonedDateTime"));
    }
// TSZ_INLINE_TEST_END d7a0ab730aa21563680352f37d632c00adbff00e1d140de876e956f653559c90

// TSZ_INLINE_TEST_BEGIN 74c9a8d9d566ddcf35191db0435a3f7a918af0a25a731e8fef04d4d770ae3473 743 es2025_root_and_feature_libs_are_embedded
    #[test]
    fn es2025_root_and_feature_libs_are_embedded() {
        for filename in [
            "es2025.d.ts",
            "es2025.float16.d.ts",
            "es2025.full.d.ts",
            "es2025.iterator.d.ts",
            "es2025.promise.d.ts",
            "es2025.regexp.d.ts",
        ] {
            assert!(is_embedded_lib(filename), "missing {filename}");
        }
    }
// TSZ_INLINE_TEST_END 74c9a8d9d566ddcf35191db0435a3f7a918af0a25a731e8fef04d4d770ae3473
