//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-emitter/src/transforms/class_es5_ast_to_ir_comments.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN d77b14ff7329b33069887a3d0269880573749d1f7da4e2cab6d31f1827ec9260 296 attached_line_comment_start_allows_statement_trailing_comments
    #[test]
    fn attached_line_comment_start_allows_statement_trailing_comments() {
        assert_eq!(
            AstToIr::attached_line_comment_start(syntax_kind_ext::VARIABLE_STATEMENT, " // ok"),
            Some(1)
        );
        assert_eq!(
            AstToIr::attached_line_comment_start(syntax_kind_ext::VARIABLE_STATEMENT, "; // ok"),
            Some(2)
        );
    }
// TSZ_INLINE_TEST_END d77b14ff7329b33069887a3d0269880573749d1f7da4e2cab6d31f1827ec9260

// TSZ_INLINE_TEST_BEGIN 301a2766cba1127c9a735aa426b7f7b37b67712de6ddcc9a2cac6274ef203e3d 308 attached_line_comment_start_rejects_comments_after_parent_delimiters
    #[test]
    fn attached_line_comment_start_rejects_comments_after_parent_delimiters() {
        assert_eq!(
            AstToIr::attached_line_comment_start(
                syntax_kind_ext::VARIABLE_STATEMENT,
                " } // not inner",
            ),
            None
        );
        assert_eq!(
            AstToIr::attached_line_comment_start(
                syntax_kind_ext::VARIABLE_STATEMENT,
                " }) // not inner",
            ),
            None
        );
    }
// TSZ_INLINE_TEST_END 301a2766cba1127c9a735aa426b7f7b37b67712de6ddcc9a2cac6274ef203e3d

// TSZ_INLINE_TEST_BEGIN 7b94af9107ecd9f33f0d888a8e57dd0651f8ec1ca9e145368adcff09ed6e4816 326 can_attach_trailing_comment_gap_rejects_parent_delimiters
    #[test]
    fn can_attach_trailing_comment_gap_rejects_parent_delimiters() {
        assert!(AstToIr::can_attach_trailing_comment_gap(" ; "));
        assert!(!AstToIr::can_attach_trailing_comment_gap(" } "));
        assert!(!AstToIr::can_attach_trailing_comment_gap(" }) "));
    }
// TSZ_INLINE_TEST_END 7b94af9107ecd9f33f0d888a8e57dd0651f8ec1ca9e145368adcff09ed6e4816
