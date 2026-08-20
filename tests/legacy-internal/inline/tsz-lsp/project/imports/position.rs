//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-lsp/src/project/imports/position.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 7258edd6d4a9e59353cabb1c4db69c62bf0fdf4e47f16304feca0aaa0f2e180d 292 identifier_text_around_offset_accepts_unicode_identifier_start_and_part
    #[test]
    fn identifier_text_around_offset_accepts_unicode_identifier_start_and_part() {
        let source = "const café = 日本語;";

        let cafe_start = source.find("café").expect("café");
        assert_eq!(
            Project::identifier_text_around_offset(source, cafe_start),
            Some("café".to_string())
        );
        assert_eq!(
            Project::identifier_text_around_offset(source, cafe_start + "café".len()),
            Some("café".to_string())
        );

        let japanese_mid = source.find("本").expect("日本語");
        assert_eq!(
            Project::identifier_text_around_offset(source, japanese_mid),
            Some("日本語".to_string())
        );
    }
// TSZ_INLINE_TEST_END 7258edd6d4a9e59353cabb1c4db69c62bf0fdf4e47f16304feca0aaa0f2e180d
