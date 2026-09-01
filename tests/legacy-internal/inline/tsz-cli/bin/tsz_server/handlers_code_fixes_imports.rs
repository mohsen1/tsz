//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-cli/src/bin/tsz_server/handlers_code_fixes_imports.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 4924e246b2f1f7f1c490283701d0b885f59d160ebec53bc867247c96a74b0b1b 1588 normalize_commonjs_module_specifier_uses_shared_extension_rules
    #[test]
    fn normalize_commonjs_module_specifier_uses_shared_extension_rules() {
        assert_eq!(
            Server::normalize_commonjs_module_specifier("./types.d.cts"),
            "./types"
        );
        assert_eq!(
            Server::normalize_commonjs_module_specifier("./types.d.tsx"),
            "./types.d"
        );
        assert_eq!(
            Server::normalize_commonjs_module_specifier("react/index.d.ts"),
            "react/index.d.ts"
        );
    }
// TSZ_INLINE_TEST_END 4924e246b2f1f7f1c490283701d0b885f59d160ebec53bc867247c96a74b0b1b
