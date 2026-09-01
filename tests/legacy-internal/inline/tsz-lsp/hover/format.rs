//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-lsp/src/hover/format.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN add2d64571493d0ddfbe9c12be64467e166c4007468c8efd0baf2866e7787c7e 255 format_hover_variable_type_multiline_index_signature_object
    #[test]
    fn format_hover_variable_type_multiline_index_signature_object() {
        let input = "{ [x: string]: T; }";
        let out = format_hover_variable_type(input);
        assert_eq!(out, "{\n    [x: string]: T;\n}");
    }
// TSZ_INLINE_TEST_END add2d64571493d0ddfbe9c12be64467e166c4007468c8efd0baf2866e7787c7e
