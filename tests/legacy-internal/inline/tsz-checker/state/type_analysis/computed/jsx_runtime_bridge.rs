//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/state/type_analysis/computed/jsx_runtime_bridge.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN a39f68ef9a4511682b44edb4889d79a4e2ff9d26b903f2a6661922727d4fb7a0 234 matches_jsx_runtime_directory_entry
    #[test]
    fn matches_jsx_runtime_directory_entry() {
        assert!(jsx_runtime_path_matches(
            "/repo/node_modules/react/jsx-runtime/index.d.ts",
            "react",
            "jsx-runtime",
        ));
        assert!(jsx_runtime_path_matches(
            r"C:\repo\node_modules\react\jsx-dev-runtime\index.d.ts"
                .replace('\\', "/")
                .as_str(),
            "react",
            "jsx-dev-runtime",
        ));
    }
// TSZ_INLINE_TEST_END a39f68ef9a4511682b44edb4889d79a4e2ff9d26b903f2a6661922727d4fb7a0

// TSZ_INLINE_TEST_BEGIN 13af934cfb75bf25b0cd5c2308b76b3483596d0e8a3218b5133e9a8c2fa8a6df 250 matches_jsx_runtime_declaration_file_entry
    #[test]
    fn matches_jsx_runtime_declaration_file_entry() {
        assert!(jsx_runtime_path_matches(
            "/repo/node_modules/react/jsx-runtime.d.ts",
            "react",
            "jsx-runtime",
        ));
        assert!(jsx_runtime_path_matches(
            "/repo/node_modules/@types/react/jsx-runtime.d.mts",
            "@types/react",
            "jsx-runtime",
        ));
    }
// TSZ_INLINE_TEST_END 13af934cfb75bf25b0cd5c2308b76b3483596d0e8a3218b5133e9a8c2fa8a6df

// TSZ_INLINE_TEST_BEGIN 8c3c53d1ef91482a7595af176d5a4717b00724fc149951c496f906fba166126e 264 matches_scoped_package_runtime_entry
    #[test]
    fn matches_scoped_package_runtime_entry() {
        assert!(jsx_runtime_path_matches(
            "/repo/node_modules/@scope/pkg/jsx-runtime/index.d.ts",
            "@scope/pkg",
            "jsx-runtime",
        ));
        assert!(jsx_runtime_path_matches(
            "/repo/node_modules/@types/scope__pkg/jsx-runtime.d.ts",
            "@types/scope__pkg",
            "jsx-runtime",
        ));
    }
// TSZ_INLINE_TEST_END 8c3c53d1ef91482a7595af176d5a4717b00724fc149951c496f906fba166126e

// TSZ_INLINE_TEST_BEGIN 487aa12d90db01bb0bf4de92cd4d54831cf79be20f337db859a9cb9db9d49b4f 278 rejects_adjacent_substring_paths
    #[test]
    fn rejects_adjacent_substring_paths() {
        assert!(!jsx_runtime_path_matches(
            "/repo/node_modules/not-react/jsx-runtime/index.d.ts",
            "react",
            "jsx-runtime",
        ));
        assert!(!jsx_runtime_path_matches(
            "/repo/node_modules/react/jsx-runtime-extra/index.d.ts",
            "react",
            "jsx-runtime",
        ));
        assert!(!jsx_runtime_path_matches(
            "/repo/vendor/node_modules-react/jsx-runtime.d.ts",
            "react",
            "jsx-runtime",
        ));
    }
// TSZ_INLINE_TEST_END 487aa12d90db01bb0bf4de92cd4d54831cf79be20f337db859a9cb9db9d49b4f
