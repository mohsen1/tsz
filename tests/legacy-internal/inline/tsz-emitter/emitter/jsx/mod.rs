//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-emitter/src/emitter/jsx/mod.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 29c6be4c1a6bdf82de50256d70ddd6fd3daa325884cd5bf0de0925e2b6d3119a 663 self_closing_no_attributes_has_space_before_slash
    #[test]
    fn self_closing_no_attributes_has_space_before_slash() {
        let output = emit_jsx("const x = <Tag />;");
        assert!(
            output.contains("<Tag />"),
            "Self-closing element without attributes should have space before />.\nOutput: {output}"
        );
    }
// TSZ_INLINE_TEST_END 29c6be4c1a6bdf82de50256d70ddd6fd3daa325884cd5bf0de0925e2b6d3119a

// TSZ_INLINE_TEST_BEGIN 52ce02db510bd558929c78806dd18f6ede4155d9e72ef330cd3d79cdded76bad 672 jsx_preserve_opening_attribute_comments_are_kept
    #[test]
    fn jsx_preserve_opening_attribute_comments_are_kept() {
        let source = "const x = (<div\n    /* kept */\n    attr=\"x\"><span // line\n      value=\"y\" /></div>);";
        let output = emit_jsx_preserve_es2015(source);
        assert!(
            output.contains("<div \n/* kept */\nattr=\"x\">"),
            "Multiline comments before JSX attributes should stay in the opening tag.\nOutput: {output}"
        );
        assert!(
            output.contains("<span // line\n value=\"y\"/>"),
            "Line comments before JSX attributes should keep the comment and tsc continuation spacing.\nOutput: {output}"
        );
    }
// TSZ_INLINE_TEST_END 52ce02db510bd558929c78806dd18f6ede4155d9e72ef330cd3d79cdded76bad

// TSZ_INLINE_TEST_BEGIN efa9c56940c3be4ea65e1e55c16cd503668c78033c8e5f1a30108f46e20fa898 686 recovered_jsx_conditional_missing_false_branch_preserves_tsc_layout
    #[test]
    fn recovered_jsx_conditional_missing_false_branch_preserves_tsc_layout() {
        let source = r#"// @target: es2015
// @jsx: preserve

declare var createElement: any;

class foo {}

var x: any;
x = <any> { test: <any></any> };

x = <any><any></any>;

x = <foo>hello {<foo>{}} </foo>;

x = <foo test={<foo>{}}>hello</foo>;

x = <foo test={<foo>{}}>hello{<foo>{}}</foo>;

x = <foo>x</foo>, x = <foo/>;

<foo>{<foo><foo>{/foo/.test(x) ? <foo><foo></foo> : <foo><foo></foo>}</foo>}</foo>"#;

        let output = emit_jsx_preserve_es2015(source);

        let expected_tail = concat!(
            "    <foo>{<foo><foo>{/foo/.test(x) ? <foo><foo></foo> : ",
            "<foo><foo></foo>}</foo>}</foo>\n",
            "            :\n",
            "        }\n\n",
            "    \n",
            "        </></>}</></>}/></></></>;"
        );
        assert!(
            output.trim_end_matches('\n').ends_with(expected_tail),
            "Recovered JSX conditional tail should match tsc layout.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END efa9c56940c3be4ea65e1e55c16cd503668c78033c8e5f1a30108f46e20fa898

// TSZ_INLINE_TEST_BEGIN 0a55ea1e3c86554322b23cfe1896cf90f450dcde4575a1d6f0696d7b3d712deb 726 self_closing_with_attributes_has_no_space_before_slash
    #[test]
    fn self_closing_with_attributes_has_no_space_before_slash() {
        let output = emit_jsx("const x = <Tag foo=\"bar\"/>;");
        assert!(
            output.contains("<Tag foo=\"bar\"/>"),
            "Self-closing element with attributes should NOT have extra space before />.\nOutput: {output}"
        );
    }
// TSZ_INLINE_TEST_END 0a55ea1e3c86554322b23cfe1896cf90f450dcde4575a1d6f0696d7b3d712deb

// TSZ_INLINE_TEST_BEGIN 04ccec18929d899d8889e615a9cecbca85b2c259b373bce563aa1f6bd106932d 735 self_closing_with_expression_attribute_no_extra_space
    #[test]
    fn self_closing_with_expression_attribute_no_extra_space() {
        let output = emit_jsx("const x = <Tag value={42}/>;");
        assert!(
            output.contains("<Tag value={42}/>"),
            "Self-closing element with expression attribute should NOT have extra space before />.\nOutput: {output}"
        );
    }
// TSZ_INLINE_TEST_END 04ccec18929d899d8889e615a9cecbca85b2c259b373bce563aa1f6bd106932d

// TSZ_INLINE_TEST_BEGIN 1829a61eb7f02b6c5135122bea5880a745e8f663f880e71b14866d2b9c914d00 744 conflict_marker_unclosed_jsx_emits_empty_synthesized_close
    #[test]
    fn conflict_marker_unclosed_jsx_emits_empty_synthesized_close() {
        let output = emit_jsx("const x = <div>\n<<<<<<< HEAD");
        assert!(
            output.contains("const x = <div></>;"),
            "Conflict-marker JSX recovery should emit an empty synthesized close.\nOutput: {output}"
        );
        assert!(
            !output.contains("</div>"),
            "Conflict-marker JSX recovery should not mirror the opener tag.\nOutput: {output}"
        );
    }
// TSZ_INLINE_TEST_END 1829a61eb7f02b6c5135122bea5880a745e8f663f880e71b14866d2b9c914d00

// TSZ_INLINE_TEST_BEGIN 32433f9336e8bfac1b941d98ca3c78c2e3b5c6208843ba3fb3fb66dbf5f278ed 757 recovered_jsx_child_that_consumes_parent_close_emits_empty_close
    #[test]
    fn recovered_jsx_child_that_consumes_parent_close_emits_empty_close() {
        let output = emit_jsx_preserve_es2015("var x = <root><leaf></root>;");
        assert!(
            output.contains("<root><leaf></></root>"),
            "Recovered child close should be emitted as an empty JSX close.\nOutput: {output}"
        );
        assert!(
            !output.contains("<root><leaf></root></root>"),
            "Recovered child should not duplicate the parent closing tag.\nOutput: {output}"
        );
    }
// TSZ_INLINE_TEST_END 32433f9336e8bfac1b941d98ca3c78c2e3b5c6208843ba3fb3fb66dbf5f278ed

// TSZ_INLINE_TEST_BEGIN 691d00083de768ea0ddc982610cde1b44f0aea98ff0b9f3fcf28d7d839d2bf94 770 mismatched_jsx_close_without_parent_recovery_keeps_written_close
    #[test]
    fn mismatched_jsx_close_without_parent_recovery_keeps_written_close() {
        let output = emit_jsx_preserve_es2015("var x = <alpha></beta>;");
        assert!(
            output.contains("<alpha></beta>"),
            "Plain mismatched JSX close should preserve the written close.\nOutput: {output}"
        );
        assert!(
            !output.contains("<alpha></>"),
            "Plain mismatched JSX close should not be treated as parent recovery.\nOutput: {output}"
        );
    }
// TSZ_INLINE_TEST_END 691d00083de768ea0ddc982610cde1b44f0aea98ff0b9f3fcf28d7d839d2bf94

// TSZ_INLINE_TEST_BEGIN 03e98a60262efdb61c05bd9f548495875888b50a964d7a2e3b203f0e9f22c655 783 jsx_text_multiline_content_preserves_whitespace
    #[test]
    fn jsx_text_multiline_content_preserves_whitespace() {
        // tsc preserves JSX text content including leading/trailing whitespace and newlines.
        // The scanner's re_scan_jsx_token must reset to full_start_pos (before trivia)
        // so the text node captures the complete whitespace content.
        let source = "let k1 = <Comp a={10} b=\"hi\">\n        hi hi hi!\n    </Comp>;";
        let output = emit_jsx(source);
        assert!(
            output.contains("\n        hi hi hi!\n    "),
            "JSX text should preserve leading/trailing whitespace and newlines.\nOutput: {output}"
        );
    }
// TSZ_INLINE_TEST_END 03e98a60262efdb61c05bd9f548495875888b50a964d7a2e3b203f0e9f22c655

// TSZ_INLINE_TEST_BEGIN 2ec22946fe53476b506276fa0e5827a017cb2b44b50bd60b936a39fe6927a23f 796 jsx_text_single_line_content
    #[test]
    fn jsx_text_single_line_content() {
        let output = emit_jsx("let x = <div>hello world</div>;");
        assert!(
            output.contains(">hello world</"),
            "JSX text on single line should be preserved.\nOutput: {output}"
        );
    }
// TSZ_INLINE_TEST_END 2ec22946fe53476b506276fa0e5827a017cb2b44b50bd60b936a39fe6927a23f

// TSZ_INLINE_TEST_BEGIN 079f92496cd03538a17cef7164f816ac20a14a02307effa8cfcfc52872d7cde5 805 jsx_text_with_nested_elements
    #[test]
    fn jsx_text_with_nested_elements() {
        let source = "let x = <Comp>\n        <div>inner</div>\n    </Comp>;";
        let output = emit_jsx(source);
        assert!(
            output.contains("\n        <div>inner</div>\n    "),
            "JSX text whitespace around nested elements should be preserved.\nOutput: {output}"
        );
    }
// TSZ_INLINE_TEST_END 079f92496cd03538a17cef7164f816ac20a14a02307effa8cfcfc52872d7cde5

// TSZ_INLINE_TEST_BEGIN 4e0de9709a2ad2aa0a473083fd1a1ad915222016b16601a880cb37d5b5020942 815 jsx_text_whitespace_only_between_elements
    #[test]
    fn jsx_text_whitespace_only_between_elements() {
        // Whitespace-only text nodes between JSX elements should be preserved
        let source = "let x = <div>\n    <span>a</span>\n    <span>b</span>\n</div>;";
        let output = emit_jsx(source);
        assert!(
            output.contains("<span>a</span>\n    <span>b</span>"),
            "Whitespace between JSX children should be preserved.\nOutput: {output}"
        );
    }
// TSZ_INLINE_TEST_END 4e0de9709a2ad2aa0a473083fd1a1ad915222016b16601a880cb37d5b5020942

// TSZ_INLINE_TEST_BEGIN a96cadfedda46e6c1c1d4f084a461193f02e3ad332f153bbfeb520d8829aefb0 826 process_jsx_text_normalizes_cr_only_line_break
    #[test]
    fn process_jsx_text_normalizes_cr_only_line_break() {
        // Issue #3903: JSX text with a CR-only line break (e.g. "a\rb") was
        // emitted unchanged because the multiline path only fired on '\n'.
        // tsc collapses CR-only breaks the same as LF, joining trimmed lines
        // with a single space.
        assert_eq!(process_jsx_text("a\rb"), "a b");
    }
// TSZ_INLINE_TEST_END a96cadfedda46e6c1c1d4f084a461193f02e3ad332f153bbfeb520d8829aefb0

// TSZ_INLINE_TEST_BEGIN 3edfa453ce49fef10ed28739ba6ee2c7bbdc9f516485a79dab550f57228fa877 835 process_jsx_text_normalizes_crlf_line_break
    #[test]
    fn process_jsx_text_normalizes_crlf_line_break() {
        assert_eq!(process_jsx_text("a\r\nb"), "a b");
    }
// TSZ_INLINE_TEST_END 3edfa453ce49fef10ed28739ba6ee2c7bbdc9f516485a79dab550f57228fa877

// TSZ_INLINE_TEST_BEGIN 3a9053227d479ef0c160c7dd558b7f5ed34f263deadf66404f5d321dab9981e6 840 process_jsx_text_normalizes_mixed_cr_and_lf
    #[test]
    fn process_jsx_text_normalizes_mixed_cr_and_lf() {
        assert_eq!(process_jsx_text("a\rb\nc"), "a b c");
    }
// TSZ_INLINE_TEST_END 3a9053227d479ef0c160c7dd558b7f5ed34f263deadf66404f5d321dab9981e6

// TSZ_INLINE_TEST_BEGIN 88fe0bcaf7b8ca4e0d12730de118b3aa94b9d1c9c0d78ad559d94c3bf73a88b9 845 escape_jsx_string_normalizes_crlf_to_lf
    #[test]
    fn escape_jsx_string_normalizes_crlf_to_lf() {
        // tsc rebuilds a JSX attribute value as a JS string literal with line
        // terminators normalized to LF, so a raw CRLF must become a single `\n`.
        assert_eq!(
            escape_jsx_text_for_js_with_quote("\r\nfoo: 23\r\n", '"'),
            "\\nfoo: 23\\n"
        );
    }
// TSZ_INLINE_TEST_END 88fe0bcaf7b8ca4e0d12730de118b3aa94b9d1c9c0d78ad559d94c3bf73a88b9

// TSZ_INLINE_TEST_BEGIN d4cbf4ab446f3a7398287192caffe570bf64047fc584e7e43d0f0bd3e1c6ca20 855 escape_jsx_string_normalizes_lone_cr_to_lf
    #[test]
    fn escape_jsx_string_normalizes_lone_cr_to_lf() {
        // Classic-Mac line endings (lone CR) normalize the same as CRLF/LF.
        assert_eq!(escape_jsx_text_for_js_with_quote("a\rb", '"'), "a\\nb");
    }
// TSZ_INLINE_TEST_END d4cbf4ab446f3a7398287192caffe570bf64047fc584e7e43d0f0bd3e1c6ca20

// TSZ_INLINE_TEST_BEGIN 7378b746decbb4712192550d7bc444b307c4ca46d2fcdf8cacca94a6fcdd32c9 861 escape_jsx_string_preserves_literal_backslash_n
    #[test]
    fn escape_jsx_string_preserves_literal_backslash_n() {
        // JSX attribute strings do not process escape sequences, so a source
        // backslash-n is two characters and stays escaped (`\\n`), while the
        // surrounding raw CRLF terminators normalize to `\n`.
        assert_eq!(
            escape_jsx_text_for_js_with_quote("\r\nfoo: 23\\n\r\n", '\''),
            "\\nfoo: 23\\\\n\\n"
        );
    }
// TSZ_INLINE_TEST_END 7378b746decbb4712192550d7bc444b307c4ca46d2fcdf8cacca94a6fcdd32c9

// TSZ_INLINE_TEST_BEGIN 222c9a270ff5ab4caa0d60825c764c93cc492503ac5e6dd9147eff949cee5e35 872 escape_jsx_string_without_line_breaks_is_unchanged
    #[test]
    fn escape_jsx_string_without_line_breaks_is_unchanged() {
        assert_eq!(
            escape_jsx_text_for_js_with_quote("plain value", '"'),
            "plain value"
        );
    }
// TSZ_INLINE_TEST_END 222c9a270ff5ab4caa0d60825c764c93cc492503ac5e6dd9147eff949cee5e35

// TSZ_INLINE_TEST_BEGIN 78418e9f39dc94df7e4abeec426f5da3c620c67519d5be01252a02325253cf8e 880 react_emit_normalizes_crlf_in_double_quoted_attr_value
    #[test]
    fn react_emit_normalizes_crlf_in_double_quoted_attr_value() {
        let source = "const a = <input value=\"\r\nfoo: 23\r\n\"></input>;";
        let output = emit_jsx_react(source);
        assert!(
            output.contains("{ value: \"\\nfoo: 23\\n\" }"),
            "CRLF in a JSX attribute value should normalize to \\n.\nOutput: {output}"
        );
        assert!(
            !output.contains("\\r"),
            "No raw CR should survive into emitted JSX string content.\nOutput: {output}"
        );
    }
// TSZ_INLINE_TEST_END 78418e9f39dc94df7e4abeec426f5da3c620c67519d5be01252a02325253cf8e

// TSZ_INLINE_TEST_BEGIN 4879bc15cc03523d832b1e038e560229e2930ae3eb771c81b3ad89ad0b215606 894 react_emit_normalizes_crlf_in_single_quoted_attr_value
    #[test]
    fn react_emit_normalizes_crlf_in_single_quoted_attr_value() {
        // Preserves the single-quote style while normalizing line endings, and
        // keeps a literal backslash-n unprocessed (JSX strings are not cooked).
        let source = "const c = <input value='\r\nfoo: 23\\n\r\n'></input>;";
        let output = emit_jsx_react(source);
        assert!(
            output.contains("{ value: '\\nfoo: 23\\\\n\\n' }"),
            "Single-quoted multiline JSX attribute value mismatch.\nOutput: {output}"
        );
    }
// TSZ_INLINE_TEST_END 4879bc15cc03523d832b1e038e560229e2930ae3eb771c81b3ad89ad0b215606

// TSZ_INLINE_TEST_BEGIN 77a19f2df854e2fda641e3cc0fb6dc3d30ef454b1f7ca620db84d0778b97813e 906 process_jsx_text_preserves_text_without_line_breaks
    #[test]
    fn process_jsx_text_preserves_text_without_line_breaks() {
        // Non-multiline text must round-trip verbatim, including significant
        // whitespace, so the rest of the emitter can decide how to escape it.
        assert_eq!(process_jsx_text("hello world"), "hello world");
        assert_eq!(process_jsx_text("  spaced  "), "  spaced  ");
    }
// TSZ_INLINE_TEST_END 77a19f2df854e2fda641e3cc0fb6dc3d30ef454b1f7ca620db84d0778b97813e

// TSZ_INLINE_TEST_BEGIN 2668b147eaf899999482d06d9d98f4bc23b61c646d486be10856a6282da2142f 914 jsx_expression_with_trailing_comment_in_expression_is_preserved
    #[test]
    fn jsx_expression_with_trailing_comment_in_expression_is_preserved() {
        let source = "let x = <div>{null/* preserved */}</div>;";
        let output = emit_jsx(source);
        assert!(
            output.contains("/* preserved */"),
            "Trailing comment inside JSX expression should be preserved.\nOutput: {output}"
        );
        assert!(
            !output.contains("{null}"),
            "Trailing comment should not be dropped from JSX expression.\nOutput: {output}"
        );
    }
// TSZ_INLINE_TEST_END 2668b147eaf899999482d06d9d98f4bc23b61c646d486be10856a6282da2142f

// TSZ_INLINE_TEST_BEGIN c55a7882e3afe084c4763dae4c0f218e196bbb4b9f01e915c7931e16d4618ab4 928 jsx_classic_nested_element_trailing_line_comment_is_preserved
    #[test]
    fn jsx_classic_nested_element_trailing_line_comment_is_preserved() {
        let source = "const xs = [1];\nconst x = <ul>{xs.map(x => (<li>{x}</li> // kept\n))}</ul>;";
        let output = emit_jsx_react(source);
        assert!(
            output.contains("React.createElement(\"li\", null, x) // kept"),
            "Classic JSX transform should preserve same-line comments after nested elements.\nOutput: {output}"
        );
        let after_comment = &output[output
            .find("// kept")
            .expect("nested JSX line comment should be emitted")..];
        assert!(
            after_comment.starts_with("// kept\n"),
            "Classic JSX trailing line comment must keep the source newline.\nOutput: {output}"
        );
    }
// TSZ_INLINE_TEST_END c55a7882e3afe084c4763dae4c0f218e196bbb4b9f01e915c7931e16d4618ab4

// TSZ_INLINE_TEST_BEGIN 4fc5176dcb3f8a3afb827dd7deb98e79b74542d6b8444ebab19644e20875cf8f 945 jsx_classic_unicode_escape_component_and_member_names_are_preserved
    #[test]
    fn jsx_classic_unicode_escape_component_and_member_names_are_preserved() {
        let source = r#"const x = { video: () => null };
const a = <Comp\u0061 x={12} />;
const b = <x.\u0076ideo />;"#;
        let output = emit_jsx_react(source);
        assert!(
            output.contains(r#"React.createElement(Comp\u0061, { x: 12 })"#),
            "Component tag identifier escapes should be preserved in expression emit.\nOutput: {output}"
        );
        assert!(
            output.contains(r#"React.createElement(x.\u0076ideo, null)"#),
            "JSX member tag property-name escapes should be preserved in expression emit.\nOutput: {output}"
        );
    }
// TSZ_INLINE_TEST_END 4fc5176dcb3f8a3afb827dd7deb98e79b74542d6b8444ebab19644e20875cf8f

// TSZ_INLINE_TEST_BEGIN 262b5f69e7d388d8d57cf6c9ed0c1aae8f80ee2e25dbc7e9b2fb5fb2889e5d11 961 jsx_classic_unicode_escape_attribute_identifier_names_are_preserved
    #[test]
    fn jsx_classic_unicode_escape_attribute_identifier_names_are_preserved() {
        let source = r#"const a = <video \u0073rc="" />;
const b = <video data-\u0076ideo />;"#;
        let output = emit_jsx_react(source);
        assert!(
            output.contains(r#"React.createElement("video", { \u0073rc: "" })"#),
            "Unquoted JSX attribute identifier keys should preserve source escapes.\nOutput: {output}"
        );
        assert!(
            output.contains(r#"React.createElement("video", { "data-video": true })"#),
            "Quoted JSX attribute keys should use cooked text, not source escape spelling.\nOutput: {output}"
        );
    }
// TSZ_INLINE_TEST_END 262b5f69e7d388d8d57cf6c9ed0c1aae8f80ee2e25dbc7e9b2fb5fb2889e5d11

// TSZ_INLINE_TEST_BEGIN 8ec38bc757858d6b0f9a72ab4b3f6c2a4eac86eebcce8fb252c914ac46c907d4 985 jsx_classic_unicode_escape_intrinsic_tag_name_is_cooked
    /// Intrinsic tag names emit as JS string literals; unicode escapes in the
    /// name must be cooked because `tsc` rebuilds the argument as a fresh JS
    /// string. Covers `\uXXXX` and `\u{X...}` escape forms, both at the head of
    /// the name and inside hyphenated kebab segments.
    ///
    /// Witness: `unicodeEscapesInJsxtags.tsx` (`@jsx react`) expects
    /// `React.createElement("a", null)`, `React.createElement("a-b", null)`,
    /// and `React.createElement("a-c", null)` for the source
    /// `<a/>`, `<a-b/>`, `<a-c/>` (and their `\u{...}` variants).
    #[test]
    fn jsx_classic_unicode_escape_intrinsic_tag_name_is_cooked() {
        let source = r#"const a = <a/>;
const b = <a-b/>;
const c = <a-c/>;
const d = <\u{0061}/>;
const e = <\u{0061}-b/>;
const f = <a-\u{0063}/>;"#;
        let output = emit_jsx_react(source);
        // Anchor each assertion to the originating `const` binding so a regression
        // in only the `\u{...}` path can't be masked by the plain `<a/>` path
        // emitting the same `React.createElement("a", null)` fragment.
        for (label, fragment) in [
            (r"a head", r#"const a = React.createElement("a", null)"#),
            (
                r"a head, hyphen tail",
                r#"const b = React.createElement("a-b", null)"#,
            ),
            (
                r"hyphen head, c tail",
                r#"const c = React.createElement("a-c", null)"#,
            ),
            (
                r"\u{0061} head",
                r#"const d = React.createElement("a", null)"#,
            ),
            (
                r"\u{0061} head, hyphen tail",
                r#"const e = React.createElement("a-b", null)"#,
            ),
            (
                r"hyphen head, \u{0063} tail",
                r#"const f = React.createElement("a-c", null)"#,
            ),
        ] {
            assert!(
                output.contains(fragment),
                "Intrinsic JSX tag with unicode escape ({label}) should emit cooked string `{fragment}`.\nOutput: {output}"
            );
        }
    }
// TSZ_INLINE_TEST_END 8ec38bc757858d6b0f9a72ab4b3f6c2a4eac86eebcce8fb252c914ac46c907d4

// TSZ_INLINE_TEST_BEGIN ef159e9c8c2212887c8e252a6c0e8aa4055ada5609f04f33d7ecfac6ba61905e 1036 jsx_classic_extended_unicode_escape_component_name_is_preserved
    /// Component tag references emit as JS expressions; unicode escapes in the
    /// component identifier (including the extended `\u{X...}` form) must be
    /// preserved verbatim because the identifier is a value reference, not a
    /// string. Mirrors the existing
    /// `jsx_classic_unicode_escape_component_and_member_names_are_preserved`
    /// coverage for the `\uXXXX` form.
    ///
    /// Witness: `unicodeEscapesInJsxtags.tsx` expects
    /// `React.createElement(Comp\u{0061}, { x: 12 })` for `<Comp\u{0061} x={12} />`.
    #[test]
    fn jsx_classic_extended_unicode_escape_component_name_is_preserved() {
        let source = r#"const a = <Comp\u{0061} x={12} />;"#;
        let output = emit_jsx_react(source);
        assert!(
            output.contains(r#"React.createElement(Comp\u{0061}, { x: 12 })"#),
            "Component tag identifier extended-escape spelling should be preserved in expression emit.\nOutput: {output}"
        );
    }
// TSZ_INLINE_TEST_END ef159e9c8c2212887c8e252a6c0e8aa4055ada5609f04f33d7ecfac6ba61905e

// TSZ_INLINE_TEST_BEGIN e28b89bacf8823a3a16175f9e6e8d06e9a19f3a8f7b0cf1dca06671d297c6e9d 1046 jsx_classic_intrinsic_tag_name_escapes_non_ascii_chars
    #[test]
    fn jsx_classic_intrinsic_tag_name_escapes_non_ascii_chars() {
        let source = "const a = <a-\u{00E9}/>;\nconst b = <a-\u{00F1}/>;\nconst c = <a-\u{00FC}/>;";
        let output = emit_jsx_react(source);
        for (label, fragment) in [
            (
                "U+00E9 tail",
                r#"const a = React.createElement("a-\u00E9", null)"#,
            ),
            (
                "U+00F1 tail",
                r#"const b = React.createElement("a-\u00F1", null)"#,
            ),
            (
                "U+00FC tail",
                r#"const c = React.createElement("a-\u00FC", null)"#,
            ),
        ] {
            assert!(
                output.contains(fragment),
                "Intrinsic JSX tag with non-ASCII tail ({label}) should JS-escape the non-ASCII codepoint.\nOutput: {output}"
            );
        }
        for raw in ["\"a-\u{00E9}\"", "\"a-\u{00F1}\"", "\"a-\u{00FC}\""] {
            assert!(
                !output.contains(raw),
                "Raw non-ASCII codepoint {raw:?} should not appear inside an emitted JSX intrinsic tag string.\nOutput: {output}"
            );
        }
    }
// TSZ_INLINE_TEST_END e28b89bacf8823a3a16175f9e6e8d06e9a19f3a8f7b0cf1dca06671d297c6e9d

// TSZ_INLINE_TEST_BEGIN 3ae3698d7fe216333844a872008efbf4a541c03d2a2bd48c74f806b46bb2e9af 1077 jsx_classic_non_bmp_attribute_value_emits_surrogate_pair
    #[test]
    fn jsx_classic_non_bmp_attribute_value_emits_surrogate_pair() {
        let source = "const a = <input value=\"\u{1F600}\"/>;";
        let output = emit_jsx_react(source);
        assert!(
            output.contains(r#"value: "\uD83D\uDE00""#),
            "Non-BMP codepoint in a JSX attribute string value should emit as a UTF-16 surrogate pair.\nOutput: {output}"
        );
        assert!(
            !output.contains("\u{1F600}"),
            "Raw non-BMP codepoint should not appear inside an emitted JSX attribute string.\nOutput: {output}"
        );
    }
// TSZ_INLINE_TEST_END 3ae3698d7fe216333844a872008efbf4a541c03d2a2bd48c74f806b46bb2e9af

// TSZ_INLINE_TEST_BEGIN 3f798029943860cd2c1c18f9e1bda4b25c5a97c7516bf012af135a3567e64120 1091 jsx_classic_namespaced_tag_name_escapes_non_ascii_chars
    #[test]
    fn jsx_classic_namespaced_tag_name_escapes_non_ascii_chars() {
        let source = "const a = <\u{00E9}:path/>;\nconst b = <svg:\u{00E9}/>;\nconst c = <\u{00E9}:\u{00F1}/>;";
        let output = emit_jsx_react(source);
        for (label, fragment) in [
            (
                "non-ASCII namespace",
                r#"const a = React.createElement("\u00E9:path", null)"#,
            ),
            (
                "non-ASCII local name",
                r#"const b = React.createElement("svg:\u00E9", null)"#,
            ),
            (
                "both halves non-ASCII",
                r#"const c = React.createElement("\u00E9:\u00F1", null)"#,
            ),
        ] {
            assert!(
                output.contains(fragment),
                "Namespaced JSX tag with non-ASCII parts ({label}) should JS-escape each half.\nOutput: {output}"
            );
        }
        for raw in [
            "\"\u{00E9}:path\"",
            "\"svg:\u{00E9}\"",
            "\"\u{00E9}:\u{00F1}\"",
        ] {
            assert!(
                !output.contains(raw),
                "Raw non-ASCII codepoint {raw:?} should not appear inside an emitted namespaced JSX tag string.\nOutput: {output}"
            );
        }
    }
// TSZ_INLINE_TEST_END 3f798029943860cd2c1c18f9e1bda4b25c5a97c7516bf012af135a3567e64120

// TSZ_INLINE_TEST_BEGIN 934d6fc6bc14d69b81faa0968338466fee106ef7d86803a9e138f51728e155b9 1126 jsx_classic_quoted_attribute_key_escapes_non_ascii_chars
    #[test]
    fn jsx_classic_quoted_attribute_key_escapes_non_ascii_chars() {
        let source = "const a = <div data-\u{00E9}=\"x\" data-\u{00F1}={1} ns:\u{00E9}=\"y\" />;";
        let output = emit_jsx_react(source);
        for (label, fragment) in [
            ("hyphenated, string value", r#""data-\u00E9": "x""#),
            ("hyphenated, expression value", r#""data-\u00F1": 1"#),
            ("namespaced", r#""ns:\u00E9": "y""#),
        ] {
            assert!(
                output.contains(fragment),
                "Quoted JSX attribute key with non-ASCII ({label}) should JS-escape the non-ASCII codepoint.\nOutput: {output}"
            );
        }
        for raw in ["\"data-\u{00E9}\"", "\"data-\u{00F1}\"", "\"ns:\u{00E9}\""] {
            assert!(
                !output.contains(raw),
                "Raw non-ASCII codepoint {raw:?} should not appear inside an emitted JSX attribute key string.\nOutput: {output}"
            );
        }
    }
// TSZ_INLINE_TEST_END 934d6fc6bc14d69b81faa0968338466fee106ef7d86803a9e138f51728e155b9

// TSZ_INLINE_TEST_BEGIN 76aa0ab27efb5b43814dc8b78cf1baaf8a082d2ac2a700344fc3f55d740d79c6 1148 jsx_classic_self_closing_trailing_line_comment_is_preserved
    #[test]
    fn jsx_classic_self_closing_trailing_line_comment_is_preserved() {
        let source = "const x = (<Item value={1} /> // kept\n);";
        let output = emit_jsx_react(source);
        assert!(
            output.contains("React.createElement(Item, { value: 1 }) // kept"),
            "Classic JSX transform should preserve same-line comments after self-closing elements.\nOutput: {output}"
        );
        let after_comment = &output[output
            .find("// kept")
            .expect("self-closing JSX line comment should be emitted")..];
        assert!(
            after_comment.starts_with("// kept\n"),
            "Classic JSX self-closing trailing line comment must keep the source newline.\nOutput: {output}"
        );
    }
// TSZ_INLINE_TEST_END 76aa0ab27efb5b43814dc8b78cf1baaf8a082d2ac2a700344fc3f55d740d79c6

// TSZ_INLINE_TEST_BEGIN db1879888eaf176c032cf3acba15b1f4231e176a0de884d3a0981cc0a8168914 1165 jsx_classic_variable_statement_trailing_comment_stays_after_semicolon
    #[test]
    fn jsx_classic_variable_statement_trailing_comment_stays_after_semicolon() {
        let source = "const x = <ns:Upcase />; // kept";
        let output = emit_jsx_react(source);
        assert!(
            output.contains("const x = React.createElement(\"ns:Upcase\", null); // kept"),
            "Classic JSX transform should leave statement comments after the emitted semicolon.\nOutput: {output}"
        );
        assert!(
            !output.contains("React.createElement(\"ns:Upcase\", null) // kept"),
            "Classic JSX transform must not attach a statement comment before the semicolon.\nOutput: {output}"
        );
    }
// TSZ_INLINE_TEST_END db1879888eaf176c032cf3acba15b1f4231e176a0de884d3a0981cc0a8168914

// TSZ_INLINE_TEST_BEGIN eb8c3df9f5a947dcc858d30686f3a04fc4319fbd1291ef74f5bfbcf36afe1642 1179 jsx_classic_self_closing_statement_comment_stays_after_semicolon
    #[test]
    fn jsx_classic_self_closing_statement_comment_stays_after_semicolon() {
        let source = "const x = <Item value={1} />; // kept";
        let output = emit_jsx_react(source);
        assert!(
            output.contains("const x = React.createElement(Item, { value: 1 }); // kept"),
            "Classic JSX transform should leave statement comments after the semicolon.\nOutput: {output}"
        );
        assert!(
            !output.contains("React.createElement(Item, { value: 1 }) // kept;"),
            "Classic JSX transform must not claim a semicolon-trailing statement comment as a JSX expression comment.\nOutput: {output}"
        );
    }
// TSZ_INLINE_TEST_END eb8c3df9f5a947dcc858d30686f3a04fc4319fbd1291ef74f5bfbcf36afe1642

// TSZ_INLINE_TEST_BEGIN 288d2f9b03a253f5e1b6d5ca7af56f3816aba4adf37a56fd062cb2120d4e787e 1193 jsx_classic_self_closing_asi_statement_comment_stays_after_semicolon
    #[test]
    fn jsx_classic_self_closing_asi_statement_comment_stays_after_semicolon() {
        let source = "const x = <Item value={1} /> // kept\nconst y = 1;";
        let output = emit_jsx_react(source);
        assert!(
            output.contains("const x = React.createElement(Item, { value: 1 }); // kept"),
            "Classic JSX transform should emit synthetic semicolons before ASI statement comments.\nOutput: {output}"
        );
        assert!(
            !output.contains("React.createElement(Item, { value: 1 }) // kept\n;"),
            "Classic JSX transform must not let JSX expression comments pull the semicolon onto the next line.\nOutput: {output}"
        );
    }
// TSZ_INLINE_TEST_END 288d2f9b03a253f5e1b6d5ca7af56f3816aba4adf37a56fd062cb2120d4e787e

// TSZ_INLINE_TEST_BEGIN c1cff6f5a65fda987f9b7a9938b612ae9b6b96e19b58aa8d84ad780e3bb5d314 1207 jsx_classic_expression_statement_asi_comment_stays_after_semicolon
    #[test]
    fn jsx_classic_expression_statement_asi_comment_stays_after_semicolon() {
        let source = "<Item value={1} /> // kept\nfoo();";
        let output = emit_jsx_react(source);
        assert!(
            output.contains("React.createElement(Item, { value: 1 }); // kept"),
            "Classic JSX expression statements should emit synthetic semicolons before ASI comments.\nOutput: {output}"
        );
        assert!(
            output.contains("// kept\nfoo();"),
            "Classic JSX expression statement comments must keep the source newline before the next statement.\nOutput: {output}"
        );
        assert!(
            !output.contains("React.createElement(Item, { value: 1 }) // kept\n;"),
            "Classic JSX expression comments must not pull the semicolon onto the next line.\nOutput: {output}"
        );
    }
// TSZ_INLINE_TEST_END c1cff6f5a65fda987f9b7a9938b612ae9b6b96e19b58aa8d84ad780e3bb5d314

// TSZ_INLINE_TEST_BEGIN 056759b607605e6e421c8284312ced32c739ee22435cc61c9fcdc43aa109eb48 1225 jsx_classic_self_closing_trailing_comment_ignores_attribute_string_slash_gt
    #[test]
    fn jsx_classic_self_closing_trailing_comment_ignores_attribute_string_slash_gt() {
        let source = "const x = (\n  <Item label=\"/>\"\n    value={1} /> // kept\n);";
        let output = emit_jsx_react(source);
        assert!(
            output.contains("React.createElement(Item, { label: \"/>\", value: 1 }) // kept"),
            "Classic JSX transform should use the real self-closing tag end, not `/>` inside an attribute string.\nOutput: {output}"
        );
        let after_comment = &output[output
            .find("// kept")
            .expect("self-closing JSX line comment should be emitted")..];
        assert!(
            after_comment.starts_with("// kept\n"),
            "Classic JSX self-closing trailing line comment must keep the source newline.\nOutput: {output}"
        );
    }
// TSZ_INLINE_TEST_END 056759b607605e6e421c8284312ced32c739ee22435cc61c9fcdc43aa109eb48

// TSZ_INLINE_TEST_BEGIN 394cd008cd9c03305800f298499e7269b60cd5687040aef5cd5c866008786d94 1242 jsx_classic_fragment_trailing_line_comment_is_preserved
    #[test]
    fn jsx_classic_fragment_trailing_line_comment_is_preserved() {
        let source = "const x = (<>{x}</> // kept\n);";
        let output = emit_jsx_react(source);
        assert!(
            output.contains("React.createElement(React.Fragment"),
            "Classic JSX fragment transform should emit a React.Fragment call.\nOutput: {output}"
        );
        let after_comment = &output[output
            .find("// kept")
            .expect("fragment JSX line comment should be emitted")..];
        assert!(
            after_comment.starts_with("// kept\n"),
            "Classic JSX fragment trailing line comment must keep the source newline.\nOutput: {output}"
        );
    }
// TSZ_INLINE_TEST_END 394cd008cd9c03305800f298499e7269b60cd5687040aef5cd5c866008786d94

// TSZ_INLINE_TEST_BEGIN be85cf326c0b2e73e2c786a14e04040ec2933f534e8a7348d5cbee29a4588f55 1259 jsx_classic_trailing_line_comment_honors_remove_comments
    #[test]
    fn jsx_classic_trailing_line_comment_honors_remove_comments() {
        let source = "const x = (<Item value={1} /> // kept\n);";
        let output = emit_jsx_react_remove_comments(source);
        assert!(
            !output.contains("// kept"),
            "Classic JSX trailing comments should not be emitted with remove_comments.\nOutput: {output}"
        );
    }
// TSZ_INLINE_TEST_END be85cf326c0b2e73e2c786a14e04040ec2933f534e8a7348d5cbee29a4588f55

// TSZ_INLINE_TEST_BEGIN 4154e68632d9fd8a128062e17ee2a2d82e1d76a73d0087a24c6140dfe69b7127 1269 jsx_expression_without_expression_preserves_inner_comments
    #[test]
    fn jsx_expression_without_expression_preserves_inner_comments() {
        let source = "let x = <div>{\n    // ???\n}</div>;";
        let output = emit_jsx(source);
        assert!(
            output.contains("// ???"),
            "Line comment inside a comment-only JSX expression should be preserved.\nOutput: {output}"
        );
        // The comment should appear after `{` on a new line, and the closing `}`
        // should align with the comment (both at the increased indent level).
        assert!(
            output.contains("{") && output.contains("// ???") && output.contains("}"),
            "Comment should remain inside JSX expression braces.\nOutput: {output}"
        );
        // Closing `}` should be on its own line after the comment (not on the
        // same line), matching tsc's output for JSX expression comments.
        let comment_idx = output.find("// ???").unwrap();
        let after_comment = &output[comment_idx..];
        assert!(
            after_comment.contains('\n'),
            "There should be a newline after the comment before the closing brace.\nOutput: {output}"
        );
        let closing_brace = after_comment.find('}').unwrap();
        let between = &after_comment[..closing_brace];
        assert!(
            between.contains('\n'),
            "Closing brace should be on a separate line from the comment.\nOutput: {output}"
        );
    }
// TSZ_INLINE_TEST_END 4154e68632d9fd8a128062e17ee2a2d82e1d76a73d0087a24c6140dfe69b7127

// TSZ_INLINE_TEST_BEGIN 54b06772be9b53d59c5b563569de61f87266ca58a0bb5a753665f96517edd23e 1299 jsx_unterminated_empty_expression_preserves_recovery_braces
    #[test]
    fn jsx_unterminated_empty_expression_preserves_recovery_braces() {
        let source = "function foo() {\n    var x = <div>  { </div>\n}";
        let output = emit_jsx(source);
        assert!(
            output.contains("var x = <div>  {} </div>;"),
            "Malformed JSX child expression should preserve tsc's recovered empty braces.\nOutput: {output}"
        );
    }
// TSZ_INLINE_TEST_END 54b06772be9b53d59c5b563569de61f87266ca58a0bb5a753665f96517edd23e

// TSZ_INLINE_TEST_BEGIN 9668354a9b98c8ce1a750bf6031da253eef09afba5ebc564b7b47d850f144dfd 1309 jsx_invalid_attribute_starters_preserve_recovered_tail
    #[test]
    fn jsx_invalid_attribute_starters_preserve_recovered_tail() {
        let source = "<test1 32data={32} />;\n<test2 -data={32} />;";
        let output = emit_jsx(source);
        assert!(
            output.contains("<test1 />;\n32;"),
            "Numeric JSX attribute recovery should leave the numeric prefix as a statement.\nOutput: {output}"
        );
        assert!(
            output.contains("<test2 /> - data;"),
            "Signed JSX attribute recovery should preserve the recovered binary tail.\nOutput: {output}"
        );
    }
// TSZ_INLINE_TEST_END 9668354a9b98c8ce1a750bf6031da253eef09afba5ebc564b7b47d850f144dfd

// TSZ_INLINE_TEST_BEGIN 91c679afc8a023a6c62bc7c6bb4f5b351ee8cc5dcf8bd42fdada23713b719052 1323 jsx_expression_without_expression_normalizes_multiline_leading_comment_indentation
    #[test]
    fn jsx_expression_without_expression_normalizes_multiline_leading_comment_indentation() {
        let source = "let x = <div>{\n    // ??? 1\n            // ??? 2\n}</div>;";
        let output = emit_jsx(source);
        // Both comments should appear in the output and the closing `}` should
        // follow on its own line.
        assert!(
            output.contains("// ??? 1") && output.contains("// ??? 2"),
            "Both comment lines should be preserved.\nOutput: {output}"
        );
        // The two comments should be on separate lines with uniform indentation
        let idx1 = output.find("// ??? 1").unwrap();
        let idx2 = output.find("// ??? 2").unwrap();
        assert!(
            output[idx1..idx2].contains('\n'),
            "Comment-only JSX expression lines should be on separate lines.\nOutput: {output}"
        );
    }
// TSZ_INLINE_TEST_END 91c679afc8a023a6c62bc7c6bb4f5b351ee8cc5dcf8bd42fdada23713b719052

// TSZ_INLINE_TEST_BEGIN 5706b9f77e61fcf778bf142e9e253fe06ea75088afca10a4fce4635a7a99aa6e 1342 jsx_expression_inline_block_comment_keeps_spacing
    #[test]
    fn jsx_expression_inline_block_comment_keeps_spacing() {
        let source = "let x = <div>{\n    // ???\n/* ??? */}</div>;";
        let output = emit_jsx(source);
        assert!(
            output.contains("/* ??? */ }"),
            "Trailing inline block comment inside JSX expression should keep leading space before closing brace.\nOutput: {output}"
        );
    }
// TSZ_INLINE_TEST_END 5706b9f77e61fcf778bf142e9e253fe06ea75088afca10a4fce4635a7a99aa6e

// TSZ_INLINE_TEST_BEGIN 7ef018cc68580e8360ea849d9940bbea0a58e2675fceb32afdfef79ed5a5fc32 1352 decode_jsx_entities_decodes_extended_named_entities
    #[test]
    fn decode_jsx_entities_decodes_extended_named_entities() {
        // Latin-1 supplement and other letters tsc decodes but the previous
        // hardcoded list omitted.
        assert_eq!(super::decode_jsx_entities("&eacute;"), "\u{00E9}");
        assert_eq!(super::decode_jsx_entities("&Aacute;"), "\u{00C1}");
        assert_eq!(super::decode_jsx_entities("&iquest;"), "\u{00BF}");
        assert_eq!(super::decode_jsx_entities("&Eacute;"), "\u{00C9}");
        assert_eq!(super::decode_jsx_entities("&szlig;"), "\u{00DF}");
        // Greek letters
        assert_eq!(super::decode_jsx_entities("&alpha;"), "\u{03B1}");
        assert_eq!(super::decode_jsx_entities("&Omega;"), "\u{03A9}");
        assert_eq!(super::decode_jsx_entities("&Delta;"), "\u{0394}");
        // Math symbols
        assert_eq!(super::decode_jsx_entities("&sum;"), "\u{2211}");
        assert_eq!(super::decode_jsx_entities("&infin;"), "\u{221E}");
        // Currency
        assert_eq!(super::decode_jsx_entities("&euro;"), "\u{20AC}");
    }
// TSZ_INLINE_TEST_END 7ef018cc68580e8360ea849d9940bbea0a58e2675fceb32afdfef79ed5a5fc32

// TSZ_INLINE_TEST_BEGIN bc51f3f5eadfb70863363b2f058afcc5c85001f1603fde117871600db00bb0d6 1372 decode_jsx_entities_preserves_previously_supported_entities
    #[test]
    fn decode_jsx_entities_preserves_previously_supported_entities() {
        // Regression net for entities the old short table covered.
        assert_eq!(super::decode_jsx_entities("&amp;"), "&");
        assert_eq!(super::decode_jsx_entities("&lt;"), "<");
        assert_eq!(super::decode_jsx_entities("&gt;"), ">");
        assert_eq!(super::decode_jsx_entities("&quot;"), "\"");
        assert_eq!(super::decode_jsx_entities("&apos;"), "'");
        assert_eq!(super::decode_jsx_entities("&nbsp;"), "\u{00A0}");
        assert_eq!(super::decode_jsx_entities("&middot;"), "\u{00B7}");
        assert_eq!(super::decode_jsx_entities("&hellip;"), "\u{2026}");
        assert_eq!(super::decode_jsx_entities("&copy;"), "\u{00A9}");
        assert_eq!(super::decode_jsx_entities("&trade;"), "\u{2122}");
        assert_eq!(super::decode_jsx_entities("&hearts;"), "\u{2665}");
        assert_eq!(super::decode_jsx_entities("&rarr;"), "\u{2192}");
    }
// TSZ_INLINE_TEST_END bc51f3f5eadfb70863363b2f058afcc5c85001f1603fde117871600db00bb0d6

// TSZ_INLINE_TEST_BEGIN e755a28c1bfab1a0d2522244b64ef85a12c2d77304b90aa96ac14e4359bb0dd1 1389 decode_jsx_entities_mixes_text_and_entities
    #[test]
    fn decode_jsx_entities_mixes_text_and_entities() {
        assert_eq!(
            super::decode_jsx_entities("caf&eacute; &amp; the&aacute;tre"),
            "caf\u{00E9} & the\u{00E1}tre",
        );
    }
// TSZ_INLINE_TEST_END e755a28c1bfab1a0d2522244b64ef85a12c2d77304b90aa96ac14e4359bb0dd1

// TSZ_INLINE_TEST_BEGIN 50c6d78f892ad2f99baa65def3b2ea21a40c26f2cbe36bc7ce018035f7e8ed62 1397 decode_jsx_entities_leaves_unknown_named_entity_alone
    #[test]
    fn decode_jsx_entities_leaves_unknown_named_entity_alone() {
        // Truly unknown names round-trip verbatim, including the trailing semi.
        assert_eq!(
            super::decode_jsx_entities("&notARealEntity;"),
            "&notARealEntity;",
        );
    }
// TSZ_INLINE_TEST_END 50c6d78f892ad2f99baa65def3b2ea21a40c26f2cbe36bc7ce018035f7e8ed62

// TSZ_INLINE_TEST_BEGIN 00bf3c33ad279c4b3ae37b008d91c73789ce3ba35a29ef7435b44a1ae035018a 1406 decode_jsx_entities_decodes_numeric_entities
    #[test]
    fn decode_jsx_entities_decodes_numeric_entities() {
        // Existing behavior we must keep.
        assert_eq!(super::decode_jsx_entities("&#233;"), "\u{00E9}");
        assert_eq!(super::decode_jsx_entities("&#xE9;"), "\u{00E9}");
        assert_eq!(super::decode_jsx_entities("&#x2026;"), "\u{2026}");
    }
// TSZ_INLINE_TEST_END 00bf3c33ad279c4b3ae37b008d91c73789ce3ba35a29ef7435b44a1ae035018a

// TSZ_INLINE_TEST_BEGIN 9e1ea60c7d8048fe6236823baf28e74d8c2da9b8a61fce940911efb6ead7abd1 1437 jsx_import_source_pragma_upgrades_classic_global_to_automatic
    #[test]
    fn jsx_import_source_pragma_upgrades_classic_global_to_automatic() {
        // Structural rule: with global `jsx: react` (classic) but a per-file
        // `@jsxImportSource` pragma, tsc routes the file through the automatic
        // jsx-runtime import path rather than `React.createElement`.
        let source = "/* @jsxImportSource preact */\nexport const Comp = () => <div/>;";
        let output = emit_jsx_global_react(source);
        assert!(
            output.contains("jsx-runtime") && output.contains("preact"),
            "Automatic runtime import for the pragma source expected.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("React.createElement"),
            "Classic createElement must not be emitted when @jsxImportSource is present.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END 9e1ea60c7d8048fe6236823baf28e74d8c2da9b8a61fce940911efb6ead7abd1

// TSZ_INLINE_TEST_BEGIN 09da5baa1440ecc64c975e124aa973ea4f2247f9b949cb2c79f8c7161610beb0 1454 jsx_import_source_pragma_upgrade_is_not_source_name_specific
    #[test]
    fn jsx_import_source_pragma_upgrade_is_not_source_name_specific() {
        // Same rule, a different (multi-segment) import source. Keying on the
        // pragma's mere presence — not a hardcoded package name — must drive
        // the automatic-runtime upgrade.
        let source = "/* @jsxImportSource @emotion/react */\nexport const Comp = () => <div/>;";
        let output = emit_jsx_global_react(source);
        assert!(
            output.contains("@emotion/react/jsx-runtime"),
            "Automatic runtime import for the multi-segment source expected.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("React.createElement"),
            "Classic createElement must not be emitted for any @jsxImportSource source.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END 09da5baa1440ecc64c975e124aa973ea4f2247f9b949cb2c79f8c7161610beb0

// TSZ_INLINE_TEST_BEGIN e439a57085c0420f80374f373529bead03b4b01e11bf5ad671bf119f3e139965 1471 explicit_jsx_runtime_classic_pragma_overrides_import_source
    #[test]
    fn explicit_jsx_runtime_classic_pragma_overrides_import_source() {
        // Precedence: an explicit `@jsxRuntime classic` keeps the classic
        // transform even alongside a `@jsxImportSource` pragma. The explicit
        // runtime pragma wins.
        let source = concat!(
            "/* @jsxRuntime classic */\n",
            "/* @jsxImportSource preact */\n",
            "export const Comp = () => <div/>;"
        );
        let output = emit_jsx_global_react(source);
        assert!(
            output.contains("React.createElement"),
            "Explicit @jsxRuntime classic must keep the classic transform.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("jsx-runtime"),
            "No automatic jsx-runtime import when @jsxRuntime classic is set.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END e439a57085c0420f80374f373529bead03b4b01e11bf5ad671bf119f3e139965

// TSZ_INLINE_TEST_BEGIN 330dea264ffcd1ae24cfd9e95fbf89fd8d4faa7d83f77793161a880df45f2a96 1492 classic_global_without_import_source_stays_classic
    #[test]
    fn classic_global_without_import_source_stays_classic() {
        // Negative case: classic global mode and no `@jsxImportSource` pragma
        // must keep the classic `React.createElement` transform.
        let source = "export const Comp = () => <div/>;";
        let output = emit_jsx_global_react(source);
        assert!(
            output.contains("React.createElement"),
            "Classic transform expected without a pragma.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("jsx-runtime"),
            "No automatic jsx-runtime import without @jsxImportSource.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END 330dea264ffcd1ae24cfd9e95fbf89fd8d4faa7d83f77793161a880df45f2a96
