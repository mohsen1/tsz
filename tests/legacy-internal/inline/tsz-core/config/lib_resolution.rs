//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-core/src/config/lib_resolution.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 9fd3b677452a63e5afde374b86c0b1296ba328880483799969fbc737a5540394 840 explicit_lib_aliases_map_es6_es7_only
    #[test]
    fn explicit_lib_aliases_map_es6_es7_only() {
        // `--lib es6`/`es7` map to the language-only ES2015/ES2016 libs; every
        // other name (including casing/whitespace variants) passes through.
        let input = vec![
            "es6".to_string(),
            "ES7".to_string(),
            "  es2017  ".to_string(),
            "dom".to_string(),
        ];
        assert_eq!(
            apply_explicit_lib_aliases(&input),
            vec![
                "es2015".to_string(),
                "es2016".to_string(),
                "  es2017  ".to_string(),
                "dom".to_string(),
            ]
        );
    }
// TSZ_INLINE_TEST_END 9fd3b677452a63e5afde374b86c0b1296ba328880483799969fbc737a5540394

// TSZ_INLINE_TEST_BEGIN 5f3aec63a90a171b04a5e529216996be6ab4818c2776eee312db0598a49d7eea 861 normalize_lib_name_strips_prefix_and_case
    #[test]
    fn normalize_lib_name_strips_prefix_and_case() {
        assert_eq!(normalize_lib_name("lib.ES2015.Promise"), "es2015.promise");
        assert_eq!(normalize_lib_name("  ESNext  "), "esnext");
        assert_eq!(normalize_lib_name("DOM"), "dom");
        // Only a leading `lib.` is stripped, never an embedded one.
        assert_eq!(normalize_lib_name("es2015.lib.core"), "es2015.lib.core");
    }
// TSZ_INLINE_TEST_END 5f3aec63a90a171b04a5e529216996be6ab4818c2776eee312db0598a49d7eea

// TSZ_INLINE_TEST_BEGIN 9b287429c0c981adf7f2d69435431d03776ca3ecd693341a93299f50204ff13a 870 legacy_aliases_point_at_existing_targets
    #[test]
    fn legacy_aliases_point_at_existing_targets() {
        // Each renamed-out-of-esnext alias must point at a stable target name,
        // and the table must stay free of self-referential entries.
        for (alias, target) in legacy_lib_aliases() {
            assert_ne!(alias, target, "alias {alias} must not map to itself");
            assert!(!target.is_empty());
        }
    }
// TSZ_INLINE_TEST_END 9b287429c0c981adf7f2d69435431d03776ca3ecd693341a93299f50204ff13a

// TSZ_INLINE_TEST_BEGIN f5c719ca0e85c2097541a0a0d8c1f2c62289833256e31c49a4bc10db745666a8 880 npm_platform_suffix_maps_supported_hosts
    #[test]
    fn npm_platform_suffix_maps_supported_hosts() {
        assert_eq!(
            npm_platform_suffix("macos", "aarch64").as_deref(),
            Some("darwin-arm64")
        );
        assert_eq!(
            npm_platform_suffix("darwin", "x64").as_deref(),
            Some("darwin-x64")
        );
        assert_eq!(
            npm_platform_suffix("linux", "x86_64").as_deref(),
            Some("linux-x64")
        );
        assert_eq!(
            npm_platform_suffix("windows", "arm64").as_deref(),
            Some("win32-arm64")
        );
        assert_eq!(npm_platform_suffix("plan9", "x64"), None);
    }
// TSZ_INLINE_TEST_END f5c719ca0e85c2097541a0a0d8c1f2c62289833256e31c49a4bc10db745666a8

// TSZ_INLINE_TEST_BEGIN 26a72db44ae0e02a1266d34cf9d724d52a9cc1257b8c7cefb191f77e4c66c59f 901 skips_launcher_only_wrapper_and_selects_platform_libs
    #[test]
    fn skips_launcher_only_wrapper_and_selects_platform_libs() {
        let root = temp_root("platform");
        std::fs::create_dir_all(root.join("scripts/node_modules/typescript/lib"))
            .expect("create launcher-only wrapper lib");
        let expected = root.join("scripts/node_modules/@typescript/typescript-darwin-arm64/lib");
        write_compiled_lib_markers(&expected);

        let resolved = lib_dir_from_root_for_platform(&root, "macos", "aarch64")
            .expect("platform libs should resolve");
        assert_eq!(resolved, canonicalize_or_owned(&expected));
        std::fs::remove_dir_all(root).expect("remove temp root");
    }
// TSZ_INLINE_TEST_END 26a72db44ae0e02a1266d34cf9d724d52a9cc1257b8c7cefb191f77e4c66c59f

// TSZ_INLINE_TEST_BEGIN ba14d2c81c502d6f1dba8b41958b133c30e52d444fd5e42355e9ef72ebd53e03 915 launcher_only_wrapper_is_not_a_lib_directory
    #[test]
    fn launcher_only_wrapper_is_not_a_lib_directory() {
        let root = temp_root("launcher-only");
        std::fs::create_dir_all(root.join("scripts/node_modules/typescript/lib"))
            .expect("create launcher-only wrapper lib");

        assert!(lib_dir_from_root_for_platform(&root, "linux", "x64").is_none());
        std::fs::remove_dir_all(root).expect("remove temp root");
    }
// TSZ_INLINE_TEST_END ba14d2c81c502d6f1dba8b41958b133c30e52d444fd5e42355e9ef72ebd53e03

// TSZ_INLINE_TEST_BEGIN 4d6c365462c26b1bfb0976671c73da69f20ed3c1d961f62b41378675f24ef2bd 925 is_known_lib_name_recognizes_esnext_disposable_and_float16_via_embedded
    #[test]
    fn is_known_lib_name_recognizes_esnext_disposable_and_float16_via_embedded() {
        // `esnext.disposable` and `esnext.float16` were added in TypeScript 5.8.
        // Older locally-installed TypeScript versions won't have these files on
        // disk, but tsz's embedded snapshot does. `is_known_lib_name` must
        // recognize them by falling through to the embedded catalog.
        assert!(
            is_known_lib_name("esnext.disposable"),
            "esnext.disposable must be recognized as a known lib name"
        );
        assert!(
            is_known_lib_name("esnext.float16"),
            "esnext.float16 must be recognized as a known lib name"
        );
        // Sanity: a bogus name must remain unknown.
        assert!(
            !is_known_lib_name("esnext.nonexistent"),
            "esnext.nonexistent must not be recognized"
        );
    }
// TSZ_INLINE_TEST_END 4d6c365462c26b1bfb0976671c73da69f20ed3c1d961f62b41378675f24ef2bd
