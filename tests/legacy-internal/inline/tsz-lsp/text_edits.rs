//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-lsp/src/text_edits.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 33c6e4b0e7c53508379ffbb2df72b12361078a3ccaed8996564801f72225cfa4 122 indentation_only_delete_trims_common_prefix
    #[test]
    fn indentation_only_delete_trims_common_prefix() {
        let source_text = "    let value = 1;\n";
        let line_map = LineMap::build(source_text);
        let edit = edit_for_offsets(source_text, 0, 4, "  ");

        let narrowed = narrow_indentation_only_edit(source_text, &line_map, &edit);

        assert_eq!(
            narrowed.range,
            Range::new(Position::new(0, 2), Position::new(0, 4))
        );
        assert_eq!(narrowed.new_text, "");
    }
// TSZ_INLINE_TEST_END 33c6e4b0e7c53508379ffbb2df72b12361078a3ccaed8996564801f72225cfa4

// TSZ_INLINE_TEST_BEGIN 488e65277cd191c28e788d0691ab0c1a3543809d156032ae0aeb976837db117b 137 mixed_whitespace_insert_trims_common_prefix
    #[test]
    fn mixed_whitespace_insert_trims_common_prefix() {
        let source_text = "\t  let value = 1;\n";
        let line_map = LineMap::build(source_text);
        let edit = edit_for_offsets(source_text, 0, 3, "\t    ");

        let narrowed = narrow_indentation_only_edit(source_text, &line_map, &edit);

        assert_eq!(
            narrowed.range,
            Range::new(Position::new(0, 3), Position::new(0, 3))
        );
        assert_eq!(narrowed.new_text, "  ");
    }
// TSZ_INLINE_TEST_END 488e65277cd191c28e788d0691ab0c1a3543809d156032ae0aeb976837db117b

// TSZ_INLINE_TEST_BEGIN 54fd36732596104063d6442ccfc5cc54166eb3ce1a03e868c1a069b4b4f4ec32 152 multiline_old_text_keeps_original_edit
    #[test]
    fn multiline_old_text_keeps_original_edit() {
        let source_text = "let a = 1;\nlet b = 2;\n";
        let line_map = LineMap::build(source_text);
        let edit = edit_for_offsets(source_text, 0, 20, "let a = 1;\n  let b = 2;");

        let narrowed = narrow_indentation_only_edit(source_text, &line_map, &edit);

        assert_eq!(narrowed.range, edit.range);
        assert_eq!(narrowed.new_text, edit.new_text);
    }
// TSZ_INLINE_TEST_END 54fd36732596104063d6442ccfc5cc54166eb3ce1a03e868c1a069b4b4f4ec32

// TSZ_INLINE_TEST_BEGIN bf5f9535b17261ac114f5e3a676fe7785e8bfb551d9df39facb0354e78af5ce5 164 zero_width_edit_keeps_original_edit
    #[test]
    fn zero_width_edit_keeps_original_edit() {
        let source_text = "let value = 1;\n";
        let line_map = LineMap::build(source_text);
        let edit = edit_for_offsets(source_text, 4, 4, "  ");

        let narrowed = narrow_indentation_only_edit(source_text, &line_map, &edit);

        assert_eq!(narrowed.range, edit.range);
        assert_eq!(narrowed.new_text, edit.new_text);
    }
// TSZ_INLINE_TEST_END bf5f9535b17261ac114f5e3a676fe7785e8bfb551d9df39facb0354e78af5ce5
