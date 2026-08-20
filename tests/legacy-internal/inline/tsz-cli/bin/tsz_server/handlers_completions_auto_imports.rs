//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-cli/src/bin/tsz_server/handlers_completions_auto_imports.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN a3c6373d87f1be72d11d065f89cf8816444fff3b53cb1ca52e1c66c57aa5d033 1379 normalize_completion_source_for_match_uses_shared_extension_rules
    #[test]
    fn normalize_completion_source_for_match_uses_shared_extension_rules() {
        assert_eq!(
            Server::normalize_completion_source_for_match("'node:fs'"),
            "fs"
        );
        assert_eq!(
            Server::normalize_completion_source_for_match("\"pkg/types.d.cts\""),
            "pkg/types"
        );
        assert_eq!(
            Server::normalize_completion_source_for_match("pkg/types.d.tsx"),
            "pkg/types.d"
        );
        assert_eq!(
            Server::normalize_completion_source_for_match("pkg/index.d.ts"),
            "pkg"
        );
    }
// TSZ_INLINE_TEST_END a3c6373d87f1be72d11d065f89cf8816444fff3b53cb1ca52e1c66c57aa5d033
