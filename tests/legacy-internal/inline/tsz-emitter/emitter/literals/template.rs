//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-emitter/src/emitter/literals/template.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN bd980abfbc4acffe410d31ee0b60fcdb022f2852bb9376a85d7ae0292c8b1742 435 unterminated_template_no_content
    /// Unterminated template literal: just an opening backtick with no closing.
    /// tsc preserves the unterminated form verbatim — no closing backtick added.
    /// The emitter adds `;` as an expression statement terminator.
    #[test]
    fn unterminated_template_no_content() {
        let output = emit("`");
        assert_eq!(
            output.trim(),
            "`;",
            "should emit opening backtick without closing, plus statement semicolon"
        );
    }
// TSZ_INLINE_TEST_END bd980abfbc4acffe410d31ee0b60fcdb022f2852bb9376a85d7ae0292c8b1742

// TSZ_INLINE_TEST_BEGIN 35488ef477b0c5a0c5caa5b2ef4ba86466fd569819190099cf911a73be77350d 447 unterminated_template_escaped_backtick
    /// Unterminated template with an escaped backtick (backslash + backtick).
    /// The backslash-backtick is content, not a closing delimiter.
    #[test]
    fn unterminated_template_escaped_backtick() {
        let output = emit("`\\`");
        assert_eq!(
            output.trim(),
            "`\\`;",
            "escaped backtick should not close the template"
        );
    }
// TSZ_INLINE_TEST_END 35488ef477b0c5a0c5caa5b2ef4ba86466fd569819190099cf911a73be77350d

// TSZ_INLINE_TEST_BEGIN d23a47c6667330a67f5481477a1ce9510b9bad3e176bcd67ebfc6aa56b77e78b 459 unterminated_template_double_backslash
    /// Unterminated template with double backslash (`\\`).
    /// Two backslashes are self-escaping; no closing backtick present.
    #[test]
    fn unterminated_template_double_backslash() {
        let output = emit("`\\\\");
        assert_eq!(
            output.trim(),
            "`\\\\;",
            "double backslash without closing backtick"
        );
    }
// TSZ_INLINE_TEST_END d23a47c6667330a67f5481477a1ce9510b9bad3e176bcd67ebfc6aa56b77e78b

// TSZ_INLINE_TEST_BEGIN c9992b98fda359f545a02113f0e84c768d92ab5952af84ad7a5d3dc52352d4be 470 terminated_template_preserved
    /// Terminated template literal should still get a closing backtick.
    #[test]
    fn terminated_template_preserved() {
        let output = emit("`hello`");
        assert_eq!(
            output.trim(),
            "`hello`;",
            "terminated template should have closing backtick"
        );
    }
// TSZ_INLINE_TEST_END c9992b98fda359f545a02113f0e84c768d92ab5952af84ad7a5d3dc52352d4be

// TSZ_INLINE_TEST_BEGIN 40e46a8d1edf91fed3c49c3e8324a2452225457ab3e921520ef6c8d6f3cdc0ee 480 template_span_comments_stay_inside_substitution
    #[test]
    fn template_span_comments_stay_inside_substitution() {
        let output = emit(
            "`head${ // single line comment\n10\n}\nmiddle${\n/* Multi-\n * line\n */\n 20\n // closing comment\n}\ntail`;",
        );

        assert!(
            output.contains("`head${ // single line comment\n10}\n"),
            "Line comment after template substitution open should stay on the `${{` line.\nGot: {output}"
        );
        assert!(
            output.contains("20\n// closing comment\n}\ntail`;"),
            "Trailing comments before template substitution close should stay before `}}`.\nGot: {output}"
        );
    }
// TSZ_INLINE_TEST_END 40e46a8d1edf91fed3c49c3e8324a2452225457ab3e921520ef6c8d6f3cdc0ee

// TSZ_INLINE_TEST_BEGIN daa39b4d8794953852cc56490c2be020290cffe1aba45cb66a94f32d84491645 496 invalid_no_substitution_template_statement_does_not_duplicate_semicolon
    #[test]
    fn invalid_no_substitution_template_statement_does_not_duplicate_semicolon() {
        let output = emit(
            r"`\u`;
`\x0`;
",
        );
        assert_eq!(
            output, "`\\u`;\n`\\x0`;\n",
            "Invalid no-substitution template statements should use the source statement semicolon once.\nGot: {output}"
        );
    }
// TSZ_INLINE_TEST_END daa39b4d8794953852cc56490c2be020290cffe1aba45cb66a94f32d84491645

// TSZ_INLINE_TEST_BEGIN 5ce0738bc88d6d7198f363a2697a7977590d88b87f0284219ccde90574fb2e18 509 invalid_template_expression_statement_does_not_duplicate_semicolon
    #[test]
    fn invalid_template_expression_statement_does_not_duplicate_semicolon() {
        let output = emit(
            r"`\u${0}`;
`${0}\x`;
",
        );
        assert_eq!(
            output, "`\\u${0}`;\n`${0}\\x`;\n",
            "Invalid template expression statements should not synthesize an extra empty statement.\nGot: {output}"
        );
    }
// TSZ_INLINE_TEST_END 5ce0738bc88d6d7198f363a2697a7977590d88b87f0284219ccde90574fb2e18

// TSZ_INLINE_TEST_BEGIN 16476ff0ff8217c8b7a4618a4a80fcdf9d2cb0d1ef579689d1a34108ec19d6a5 523 tagged_unterminated_template
    /// Tagged template with unterminated no-substitution template.
    #[test]
    fn tagged_unterminated_template() {
        let source = "function f(x: any) {}\nf `abc";
        let output = emit(source);
        assert!(
            output.contains("f `abc;") && !output.contains("f `abc`;"),
            "tagged unterminated template should not add closing backtick\nGot: {output}"
        );
    }
// TSZ_INLINE_TEST_END 16476ff0ff8217c8b7a4618a4a80fcdf9d2cb0d1ef579689d1a34108ec19d6a5

// TSZ_INLINE_TEST_BEGIN c294dd49726f4b3bd59df39cb3cd211e2c9979cac4d353dd9f905a63181d1351 537 template_span_missing_closing_brace_at_eof
    /// Template substitution missing its closing `}` at EOF: the parser
    /// synthesizes a `TemplateTail` with `raw_text: None` whose node
    /// position does NOT begin with `}`. The fallback path must not
    /// fabricate the `}` (or the closing backtick) that was never lexed.
    #[test]
    fn template_span_missing_closing_brace_at_eof() {
        let output = emit("`head${0");
        assert!(
            !output.contains("`head${0}"),
            "synthetic recovery TemplateTail must not synthesize a `}}`\nGot: {output}"
        );
        assert!(
            !output.contains("`head${0`"),
            "synthetic recovery TemplateTail must not synthesize a closing backtick\nGot: {output}"
        );
        assert!(
            output.contains("`head${0"),
            "recovered template head/expression bytes should still be emitted\nGot: {output}"
        );
    }
// TSZ_INLINE_TEST_END c294dd49726f4b3bd59df39cb3cd211e2c9979cac4d353dd9f905a63181d1351

// TSZ_INLINE_TEST_BEGIN e8a19cde12acdc2f981e2c3fb9907d56c828682ced2c4e34b89f31df290e8be0 558 template_span_missing_closing_brace_before_non_template_token
    /// Template substitution where the expression is followed by a
    /// non-template token (instead of `}`). The parser stops the template
    /// expression with a synthetic `TemplateTail` whose position is at the
    /// non-`}` token; the emitter must not write a fake `}`.
    #[test]
    fn template_span_missing_closing_brace_before_non_template_token() {
        let output = emit("`head${0 foo;");
        assert!(
            !output.contains("`head${0}"),
            "synthetic recovery TemplateTail before a non-`}}` token must not synthesize `}}`\nGot: {output}"
        );
        assert!(
            output.contains("`head${0"),
            "recovered template head and expression bytes should still be emitted\nGot: {output}"
        );
    }
// TSZ_INLINE_TEST_END e8a19cde12acdc2f981e2c3fb9907d56c828682ced2c4e34b89f31df290e8be0

// TSZ_INLINE_TEST_BEGIN e93548eff5684cb341ca0818c4cba035a755a18d068aab29d92df7c3f608b045 571 unterminated_template_tail_ending_with_brace_keeps_recovery_newline
    #[test]
    fn unterminated_template_tail_ending_with_brace_keeps_recovery_newline() {
        let output = emit("`head${0}\n}");
        assert!(
            output.contains("}\n;"),
            "unterminated template tail ending in `}}` should keep a line break before the synthesized semicolon\nGot: {output}"
        );
    }
// TSZ_INLINE_TEST_END e93548eff5684cb341ca0818c4cba035a755a18d068aab29d92df7c3f608b045
