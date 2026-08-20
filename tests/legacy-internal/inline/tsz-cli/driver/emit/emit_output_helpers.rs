//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-cli/src/driver/emit/emit_output_helpers.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN cf35b0c54d82e555456b2566460c5616a20fccb67ce8359d1dd21976013aec63 868 resolves_relative_specifiers_against_module_dir
    #[test]
    fn resolves_relative_specifiers_against_module_dir() {
        // Pinned before routing through
        // path_identity::resolve_relative_slash_specifier.
        assert_eq!(
            resolve_amd_relative_module_specifier("dir/m1", "./m2"),
            Some("dir/m2".to_string())
        );
        assert_eq!(
            resolve_amd_relative_module_specifier("dir/sub/m", "../x.js"),
            Some("dir/x".to_string())
        );
        // Known TS/JS extensions are stripped from the resolved module id.
        assert_eq!(
            resolve_amd_relative_module_specifier("m1", "./m2.ts"),
            Some("m2".to_string())
        );
    }
// TSZ_INLINE_TEST_END cf35b0c54d82e555456b2566460c5616a20fccb67ce8359d1dd21976013aec63

// TSZ_INLINE_TEST_BEGIN 26b7312cfcead748baf4fffc4047a68186f74736198899ff3c439ae3a1984eea 887 bails_on_root_escape_and_empty_result
    #[test]
    fn bails_on_root_escape_and_empty_result() {
        // `..` escaping the bundle root: the caller keeps the raw specifier.
        assert_eq!(resolve_amd_relative_module_specifier("m1", "../x"), None);
        // Collapsing to nothing is also a bail, not an empty module id.
        assert_eq!(resolve_amd_relative_module_specifier("dir/m", "./.."), None);
    }
// TSZ_INLINE_TEST_END 26b7312cfcead748baf4fffc4047a68186f74736198899ff3c439ae3a1984eea
