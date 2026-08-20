//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-common/src/file_extensions.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN cc7377a6b1028637deaa0bd2ffb0adf7cf814358cfa5d5b3b8402979f839b169 310 strip_ts_extension_drops_ts_family_only
    #[test]
    fn strip_ts_extension_drops_ts_family_only() {
        assert_eq!(strip_ts_extension("foo.ts"), "foo");
        assert_eq!(strip_ts_extension("foo.tsx"), "foo");
        assert_eq!(strip_ts_extension("foo.d.ts"), "foo");
        assert_eq!(strip_ts_extension("foo.d.mts"), "foo");
        assert_eq!(strip_ts_extension("foo.cts"), "foo");
        // JS family preserved (regression: lateBoundAssignmentDeclarationSupport2.js)
        assert_eq!(strip_ts_extension("foo.js"), "foo.js");
        assert_eq!(strip_ts_extension("foo.jsx"), "foo.jsx");
        assert_eq!(strip_ts_extension("foo.mjs"), "foo.mjs");
        assert_eq!(strip_ts_extension("foo.cjs"), "foo.cjs");
        // Unknown / no-extension preserved
        assert_eq!(strip_ts_extension("foo"), "foo");
        assert_eq!(strip_ts_extension("foo.json"), "foo.json");
    }
// TSZ_INLINE_TEST_END cc7377a6b1028637deaa0bd2ffb0adf7cf814358cfa5d5b3b8402979f839b169

// TSZ_INLINE_TEST_BEGIN 4a430a26af85b32dead86e4a83618c69b91dd5c79bf6b2485fd6d7fa9c900a27 327 strip_ts_extension_prefers_d_ts_over_ts
    #[test]
    fn strip_ts_extension_prefers_d_ts_over_ts() {
        assert_eq!(strip_ts_extension("foo.d.ts"), "foo");
        assert_eq!(strip_ts_extension("foo.d.mts"), "foo");
        assert_eq!(strip_ts_extension("foo.d.cts"), "foo");
        assert_eq!(strip_ts_extension("foo.d.tsx"), "foo.d");
    }
// TSZ_INLINE_TEST_END 4a430a26af85b32dead86e4a83618c69b91dd5c79bf6b2485fd6d7fa9c900a27

// TSZ_INLINE_TEST_BEGIN b89cfc75c085b8abe91bc26467a284549035e6cd72963976a13604c15e7a50b9 335 strip_known_extension_drops_both_families
    #[test]
    fn strip_known_extension_drops_both_families() {
        assert_eq!(strip_known_extension("foo.ts"), "foo");
        assert_eq!(strip_known_extension("foo.js"), "foo");
        assert_eq!(strip_known_extension("foo.d.ts"), "foo");
        assert_eq!(strip_known_extension("foo.d.tsx"), "foo.d");
        assert_eq!(strip_known_extension("foo"), "foo");
        assert_eq!(strip_known_extension("foo.json"), "foo.json");
    }
// TSZ_INLINE_TEST_END b89cfc75c085b8abe91bc26467a284549035e6cd72963976a13604c15e7a50b9

// TSZ_INLINE_TEST_BEGIN b185ef8007eb852a4b2b1ddf16666ae39ec5eb811a53024e6b1a79b1f3017980 345 path_predicates_classify_extension_families
    #[test]
    fn path_predicates_classify_extension_families() {
        assert!(is_ts_file(Path::new("index.ts")));
        assert!(is_ts_file(Path::new("index.d.ts")));
        assert!(is_ts_file(Path::new("index.d.mts")));
        assert!(is_ts_source_file(Path::new("index.mts")));
        assert!(is_ts_source_file(Path::new("index.d.tsx")));
        assert!(!is_ts_source_file(Path::new("index.d.mts")));
        assert!(is_ts_declaration_file(Path::new("index.d.cts")));
        assert!(is_ts_declaration_file(Path::new("style.d.css.ts")));
        assert!(is_ts_declaration_file(Path::new("INDEX.D.MTS")));
        assert!(!is_ts_declaration_file(Path::new("style.css.ts")));
        assert!(!is_ts_declaration_file_name("foo.d/bar.ts"));
        assert!(!is_ts_declaration_file(Path::new("index.d.tsx")));
        assert!(is_js_file(Path::new("index.cjs")));
        assert!(is_json_file(Path::new("package.json")));
        assert!(!is_valid_module_file(Path::new("index.js")));
        assert!(is_valid_module_or_js_file(Path::new("index.js")));
    }
// TSZ_INLINE_TEST_END b185ef8007eb852a4b2b1ddf16666ae39ec5eb811a53024e6b1a79b1f3017980

// TSZ_INLINE_TEST_BEGIN fb8f04e7e7a880e8f2900a1785013a2f07cb2e1adb43327450a06a7c502cfd24 365 discovery_include_patterns_follow_extension_families
    #[test]
    fn discovery_include_patterns_follow_extension_families() {
        assert_eq!(
            default_discovery_include_patterns(false, false),
            vec![
                "*.ts", "*.tsx", "*.mts", "*.cts", "**/*.ts", "**/*.tsx", "**/*.mts", "**/*.cts"
            ]
        );
        assert!(!default_discovery_include_patterns(true, true).contains(&"**/*.json".to_string()));
        assert!(include_pattern_has_supported_extension("src/index.mjs"));
        assert!(!include_pattern_has_supported_extension("src/*.json"));
        assert!(!include_pattern_has_supported_extension("src"));
    }
// TSZ_INLINE_TEST_END fb8f04e7e7a880e8f2900a1785013a2f07cb2e1adb43327450a06a7c502cfd24

// TSZ_INLINE_TEST_BEGIN 6f9c3b63e79a691be7369dae916d0687edb6707ee6f005754485094545055d39 379 resolution_priority_lists_match_tsc_supported_extensions
    #[test]
    fn resolution_priority_lists_match_tsc_supported_extensions() {
        // `supportedTSExtensions = [[Ts, Tsx, Dts], [Cts, Dcts], [Mts, Dmts]]`
        // — universal TS group, then CJS-tagged group, then ESM-tagged group.
        assert_eq!(
            TSC_TS_RESOLUTION_EXTENSIONS,
            &[".ts", ".tsx", ".d.ts", ".cts", ".d.cts", ".mts", ".d.mts"],
        );
        // `allSupportedExtensions = [[Ts, Tsx, Dts, Js, Jsx], [Cts, Dcts, Cjs], [Mts, Dmts, Mjs]]`
        // — JS surfaces sit in the universal first group; `.cjs` ships with
        // the CJS-tagged group; `.mjs` ships with the ESM-tagged group.
        assert_eq!(
            TSC_TS_JS_RESOLUTION_EXTENSIONS,
            &[
                ".ts", ".tsx", ".d.ts", ".js", ".jsx", ".cts", ".d.cts", ".cjs", ".mts", ".d.mts",
                ".mjs",
            ],
        );
    }
// TSZ_INLINE_TEST_END 6f9c3b63e79a691be7369dae916d0687edb6707ee6f005754485094545055d39

// TSZ_INLINE_TEST_BEGIN 6b796cdc0e104f3da39855789e25da01ef657ceca66776184c5744337ab8f521 399 bare_resolution_lists_mirror_dotted_lists_without_leading_dot
    #[test]
    fn bare_resolution_lists_mirror_dotted_lists_without_leading_dot() {
        // The bare lists are the same priority order, with the leading dot
        // stripped. `tsz-core` / `tsz-cli` / `tsz-lsp` append the dot via
        // `Path::with_extension`, so they consume the bare form.
        for (dotted, bare) in [
            (
                TSC_TS_RESOLUTION_EXTENSIONS,
                TSC_TS_RESOLUTION_EXTENSIONS_BARE,
            ),
            (
                TSC_TS_JS_RESOLUTION_EXTENSIONS,
                TSC_TS_JS_RESOLUTION_EXTENSIONS_BARE,
            ),
        ] {
            assert_eq!(dotted.len(), bare.len());
            for (d, b) in dotted.iter().zip(bare) {
                assert_eq!(d.strip_prefix('.'), Some(*b), "{d} → {b}");
            }
        }
    }
// TSZ_INLINE_TEST_END 6b796cdc0e104f3da39855789e25da01ef657ceca66776184c5744337ab8f521

// TSZ_INLINE_TEST_BEGIN 2d53ff142a0e263b907652a9176d683105b145dbdf8d88ecf9a3bb4ad1313778 421 is_default_lib_file_name_matches_lib_prefix_dts_suffix
    #[test]
    fn is_default_lib_file_name_matches_lib_prefix_dts_suffix() {
        assert!(is_default_lib_file_name("lib.d.ts"));
        assert!(is_default_lib_file_name("lib.es5.d.ts"));
        assert!(is_default_lib_file_name("lib.esnext.full.d.ts"));
        assert!(is_default_lib_file_name("lib.dom.d.ts"));
        assert!(is_default_lib_file_name("lib.decorators.d.ts"));
    }
// TSZ_INLINE_TEST_END 2d53ff142a0e263b907652a9176d683105b145dbdf8d88ecf9a3bb4ad1313778

// TSZ_INLINE_TEST_BEGIN b33d69abbf9e01f0fcba267662c7a8b8dbd810ed4032d509c71735ba4e8b9d1e 430 is_default_lib_file_name_rejects_non_lib_files
    #[test]
    fn is_default_lib_file_name_rejects_non_lib_files() {
        assert!(!is_default_lib_file_name("types.d.ts"));
        assert!(!is_default_lib_file_name("index.ts"));
        assert!(!is_default_lib_file_name("lib.custom.ts")); // not .d.ts
        assert!(!is_default_lib_file_name("mylib.d.ts")); // no "lib." prefix
    }
// TSZ_INLINE_TEST_END b33d69abbf9e01f0fcba267662c7a8b8dbd810ed4032d509c71735ba4e8b9d1e

// TSZ_INLINE_TEST_BEGIN 8c9487f7b27d9f5c1da61f94254c095c8c0a6fe6668d5c9aeef4f9686dbf2093 438 is_default_lib_file_matches_at_typescript_lib_package
    #[test]
    fn is_default_lib_file_matches_at_typescript_lib_package() {
        use std::path::Path;
        // Absolute paths (as they appear in real project compilation)
        assert!(is_default_lib_file(Path::new(
            "/project/node_modules/@typescript/lib-es5/index.d.ts"
        )));
        assert!(is_default_lib_file(Path::new("lib.es5.d.ts")));
        assert!(!is_default_lib_file(Path::new(
            "/project/node_modules/some-pkg/index.d.ts"
        )));
    }
// TSZ_INLINE_TEST_END 8c9487f7b27d9f5c1da61f94254c095c8c0a6fe6668d5c9aeef4f9686dbf2093

// TSZ_INLINE_TEST_BEGIN 3230aeefb6f805b08bc96cca763893cfa521c360e50d3195ea5f0a4a1db29e84 451 path_extension_stripping_preserves_source_vs_declaration_boundary
    #[test]
    fn path_extension_stripping_preserves_source_vs_declaration_boundary() {
        assert_eq!(
            strip_ts_source_extension_from_path(Path::new("src/index.ts")),
            Some(PathBuf::from("src/index"))
        );
        assert_eq!(
            strip_ts_source_extension_from_path(Path::new("src/index.d.ts")),
            None
        );
        assert_eq!(
            strip_ts_declaration_extension_from_path(Path::new("src/index.d.mts")),
            Some(PathBuf::from("src/index"))
        );
        assert_eq!(
            strip_ts_source_extension_from_path(Path::new("src/index.d.tsx")),
            Some(PathBuf::from("src/index.d"))
        );
        assert_eq!(
            strip_ts_declaration_extension_from_path(Path::new("src/index.d.tsx")),
            None
        );
    }
// TSZ_INLINE_TEST_END 3230aeefb6f805b08bc96cca763893cfa521c360e50d3195ea5f0a4a1db29e84
