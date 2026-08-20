//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/state/type_analysis/computed_helpers_namespace_display.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 8eb83a6f0e607bbee26e24a81501852bcd6d23b4ec91f2be7adaef4a866de34b 50 virtual_fs_root_node_modules_keeps_full_path
    #[test]
    fn virtual_fs_root_node_modules_keeps_full_path() {
        // `/node_modules/pkg/index.d.ts` → `node_modules/pkg/index.d.ts`
        // (caller strips extension; we keep the full path including node_modules)
        assert_eq!(
            trim_namespace_display_path("/node_modules/mdast-util-to-string/index.d.ts"),
            "node_modules/mdast-util-to-string/index.d.ts"
        );
    }
// TSZ_INLINE_TEST_END 8eb83a6f0e607bbee26e24a81501852bcd6d23b4ec91f2be7adaef4a866de34b

// TSZ_INLINE_TEST_BEGIN 46de722828c5de34a598d40f62520ecb2bf0a3801ff12b58b699a2b8cc754635 60 virtual_fs_root_scoped_package_keeps_full_path
    #[test]
    fn virtual_fs_root_scoped_package_keeps_full_path() {
        assert_eq!(
            trim_namespace_display_path("/node_modules/@scope/pkg/index.d.ts"),
            "node_modules/@scope/pkg/index.d.ts"
        );
    }
// TSZ_INLINE_TEST_END 46de722828c5de34a598d40f62520ecb2bf0a3801ff12b58b699a2b8cc754635

// TSZ_INLINE_TEST_BEGIN 93b2917e776ee1cf260585b7b5539b230f41dbb5e60b90d25ddd3d39054adb49 68 deep_project_path_keeps_package_subpath
    #[test]
    fn deep_project_path_keeps_package_subpath() {
        // Real project: /home/user/project/node_modules/shortid/index.d.ts →
        // "node_modules/shortid/index.d.ts" (host/project prefix dropped, package
        // subpath preserved to match tsc's stable display form).
        assert_eq!(
            trim_namespace_display_path("/home/user/project/node_modules/shortid/index.d.ts"),
            "node_modules/shortid/index.d.ts"
        );
    }
// TSZ_INLINE_TEST_END 93b2917e776ee1cf260585b7b5539b230f41dbb5e60b90d25ddd3d39054adb49

// TSZ_INLINE_TEST_BEGIN a8b2e2504dd80e2cf4aed270fb7861fd466ec0c617b5f5563d922a3313be0cb9 79 deep_project_scoped_package_keeps_full_subpath
    #[test]
    fn deep_project_scoped_package_keeps_full_subpath() {
        assert_eq!(
            trim_namespace_display_path("/home/user/project/node_modules/@types/react/index.d.ts"),
            "node_modules/@types/react/index.d.ts"
        );
    }
// TSZ_INLINE_TEST_END a8b2e2504dd80e2cf4aed270fb7861fd466ec0c617b5f5563d922a3313be0cb9

// TSZ_INLINE_TEST_BEGIN 4a23c844bd58cbee7725c8ab710fb4b0c6e162f5b507f1d5227d750002b7a891 87 virtual_root_prefix_path_kept
    #[test]
    fn virtual_root_prefix_path_kept() {
        // /p123/node_modules/csv-parse/lib/index.d.ts → "p123/node_modules/csv-parse/lib/index.d.ts"
        assert_eq!(
            trim_namespace_display_path("/p123/node_modules/csv-parse/lib/index.d.ts"),
            "p123/node_modules/csv-parse/lib/index.d.ts"
        );
    }
// TSZ_INLINE_TEST_END 4a23c844bd58cbee7725c8ab710fb4b0c6e162f5b507f1d5227d750002b7a891

// TSZ_INLINE_TEST_BEGIN 2af20319178637dc4cad0bba30672437261c93ab4d88d5d61ff0b0376b9af74d 96 no_node_modules_returns_trimmed
    #[test]
    fn no_node_modules_returns_trimmed() {
        assert_eq!(trim_namespace_display_path("/src/utils.ts"), "src/utils.ts");
        assert_eq!(
            trim_namespace_display_path("./src/utils.ts"),
            "src/utils.ts"
        );
        assert_eq!(trim_namespace_display_path("server.d.ts"), "server.d.ts");
    }
// TSZ_INLINE_TEST_END 2af20319178637dc4cad0bba30672437261c93ab4d88d5d61ff0b0376b9af74d

// TSZ_INLINE_TEST_BEGIN 7e804bfdc4d102c494ee2a8fc877131583f3ceee048574b4212f92defb12b0f1 106 relative_prefix_stripped
    #[test]
    fn relative_prefix_stripped() {
        assert_eq!(trim_namespace_display_path("./mod.d.ts"), "mod.d.ts");
    }
// TSZ_INLINE_TEST_END 7e804bfdc4d102c494ee2a8fc877131583f3ceee048574b4212f92defb12b0f1
