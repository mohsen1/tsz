//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-emitter/src/declaration_emitter/helpers/type_inference_return_normalization.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 93da2cbb817e1bfe2ba25fe6ba30062c22205ddabd87036db0a4f8989b479404 1775 exact_return_member_rewrite_preserves_indentation
    #[test]
    fn exact_return_member_rewrite_preserves_indentation() {
        let source = "{\n    value: unknown;\n    other: unknown;\n}";
        let rewrites = vec![("value: unknown;".to_string(), "value: string;".to_string())];

        let rewritten = DeclarationEmitter::rewrite_exact_return_member_lines(source, &rewrites);

        assert_eq!(rewritten, "{\n    value: string;\n    other: unknown;\n}");
    }
// TSZ_INLINE_TEST_END 93da2cbb817e1bfe2ba25fe6ba30062c22205ddabd87036db0a4f8989b479404

// TSZ_INLINE_TEST_BEGIN ef6a543e688e01965b746fec5a3cdd49543527ff3559879934021640f4f22e1e 1785 exact_return_member_rewrite_does_not_touch_partial_matches
    #[test]
    fn exact_return_member_rewrite_does_not_touch_partial_matches() {
        let source = "{\n    value: unknown;\n    nested: { value: unknown; };\n}";
        let rewrites = vec![("value: unknown;".to_string(), "value: number;".to_string())];

        let rewritten = DeclarationEmitter::rewrite_exact_return_member_lines(source, &rewrites);

        assert_eq!(
            rewritten,
            "{\n    value: number;\n    nested: { value: unknown; };\n}"
        );
    }
// TSZ_INLINE_TEST_END ef6a543e688e01965b746fec5a3cdd49543527ff3559879934021640f4f22e1e
