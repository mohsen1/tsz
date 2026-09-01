//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-lsp/src/jsdoc/markdown_escape.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN f214561ec0a631ef6fa17a67e5b5fb8beaae2919ef0812f5a2a7142e25f28759 111 escape_label_passes_alphanumeric_unchanged
    #[test]
    fn escape_label_passes_alphanumeric_unchanged() {
        assert_eq!(
            escape_markdown_label("Adds two numbers"),
            "Adds two numbers"
        );
        assert_eq!(escape_markdown_label(""), "");
        assert_eq!(escape_markdown_label("123 abc"), "123 abc");
    }
// TSZ_INLINE_TEST_END f214561ec0a631ef6fa17a67e5b5fb8beaae2919ef0812f5a2a7142e25f28759

// TSZ_INLINE_TEST_BEGIN ef43870167eb5eed446f972c8af3200400e9ed34c9d20e22d3e5ebd5022e18b1 121 escape_label_escapes_link_brackets_and_parens
    #[test]
    fn escape_label_escapes_link_brackets_and_parens() {
        assert_eq!(escape_markdown_label("foo[0]"), "foo\\[0\\]");
        assert_eq!(escape_markdown_label("see (note)"), "see \\(note\\)");
        assert_eq!(escape_markdown_label("[a](b)"), "\\[a\\]\\(b\\)",);
    }
// TSZ_INLINE_TEST_END ef43870167eb5eed446f972c8af3200400e9ed34c9d20e22d3e5ebd5022e18b1

// TSZ_INLINE_TEST_BEGIN f88d961d65fcacbc43fbf707de832e0d1969835252c21a3b64b004742cc5dfaa 128 escape_label_escapes_emphasis_and_code_markers
    #[test]
    fn escape_label_escapes_emphasis_and_code_markers() {
        assert_eq!(escape_markdown_label("*hi*"), "\\*hi\\*");
        assert_eq!(escape_markdown_label("__bold__"), "\\_\\_bold\\_\\_");
        assert_eq!(escape_markdown_label("`code`"), "\\`code\\`");
    }
// TSZ_INLINE_TEST_END f88d961d65fcacbc43fbf707de832e0d1969835252c21a3b64b004742cc5dfaa

// TSZ_INLINE_TEST_BEGIN 17976380273ab4194e805ba428b20cf461fa5c196c53ff8b78143432bb6b914c 135 escape_label_escapes_html_angles_and_backslash
    #[test]
    fn escape_label_escapes_html_angles_and_backslash() {
        assert_eq!(escape_markdown_label("<b>"), "\\<b\\>");
        assert_eq!(escape_markdown_label("a\\b"), "a\\\\b");
    }
// TSZ_INLINE_TEST_END 17976380273ab4194e805ba428b20cf461fa5c196c53ff8b78143432bb6b914c

// TSZ_INLINE_TEST_BEGIN 6bbf774da64206bfed073c04beade6a2382ddd3de996d2e05074279f744ca81c 141 escape_label_is_name_independent
    #[test]
    fn escape_label_is_name_independent() {
        // The function must react to delimiter characters, not to specific
        // identifier spellings. Renaming alphanumeric content must not change
        // the escape pattern.
        let a = escape_markdown_label("Alpha[Beta]");
        let b = escape_markdown_label("Foo[Bar]");
        let c = escape_markdown_label("X[Y]");
        assert!(a.contains("\\[") && a.contains("\\]"));
        assert!(b.contains("\\[") && b.contains("\\]"));
        assert!(c.contains("\\[") && c.contains("\\]"));
    }
// TSZ_INLINE_TEST_END 6bbf774da64206bfed073c04beade6a2382ddd3de996d2e05074279f744ca81c

// TSZ_INLINE_TEST_BEGIN f12ac73211175393f2b58ce7e96939e9b8b54ad088d236f15220c9ab4474704a 154 inline_code_alphanumeric_uses_single_fence
    #[test]
    fn inline_code_alphanumeric_uses_single_fence() {
        assert_eq!(format_inline_code("name"), "`name`");
        assert_eq!(format_inline_code("a b c"), "`a b c`");
    }
// TSZ_INLINE_TEST_END f12ac73211175393f2b58ce7e96939e9b8b54ad088d236f15220c9ab4474704a

// TSZ_INLINE_TEST_BEGIN 737d57750bd0b53ffdf59a7b0ec4ff19841c0706c6e8028603511c4a4df4d81d 160 inline_code_empty_returns_empty
    #[test]
    fn inline_code_empty_returns_empty() {
        assert_eq!(format_inline_code(""), "");
    }
// TSZ_INLINE_TEST_END 737d57750bd0b53ffdf59a7b0ec4ff19841c0706c6e8028603511c4a4df4d81d

// TSZ_INLINE_TEST_BEGIN 61fc702d67e5b717bc7292413ef3de166307133298485e94751f5ebe5ee0d541 165 inline_code_with_single_backtick_uses_double_fence
    #[test]
    fn inline_code_with_single_backtick_uses_double_fence() {
        // Content `foo`bar should render as ``foo`bar`` so the inner ` does
        // not close the span.
        assert_eq!(format_inline_code("foo`bar"), "``foo`bar``");
    }
// TSZ_INLINE_TEST_END 61fc702d67e5b717bc7292413ef3de166307133298485e94751f5ebe5ee0d541

// TSZ_INLINE_TEST_BEGIN d45163fd851d991abd4ade655e102b3806919ef97db0b7b325d40543d35105b5 172 inline_code_with_double_backtick_uses_triple_fence
    #[test]
    fn inline_code_with_double_backtick_uses_triple_fence() {
        assert_eq!(format_inline_code("a``b"), "```a``b```");
    }
// TSZ_INLINE_TEST_END d45163fd851d991abd4ade655e102b3806919ef97db0b7b325d40543d35105b5

// TSZ_INLINE_TEST_BEGIN 1ac0330c10f5fd7711ef11b0d63c313bd51f223d0a66bf4ac5d365f249b972ae 177 inline_code_with_leading_backtick_pads_with_space
    #[test]
    fn inline_code_with_leading_backtick_pads_with_space() {
        // `CommonMark` §6.1: when the content starts or ends with a backtick,
        // a single space pad makes the fence unambiguous; both pads are
        // stripped by the renderer.
        assert_eq!(format_inline_code("`leading"), "`` `leading ``");
        assert_eq!(format_inline_code("trailing`"), "`` trailing` ``");
    }
// TSZ_INLINE_TEST_END 1ac0330c10f5fd7711ef11b0d63c313bd51f223d0a66bf4ac5d365f249b972ae

// TSZ_INLINE_TEST_BEGIN 6261dcb561a1c9e761aa0bbe527fd043ac2808f793f49a7cafd91cb4d51cd106 186 inline_code_with_only_backticks_picks_longer_fence_and_pads
    #[test]
    fn inline_code_with_only_backticks_picks_longer_fence_and_pads() {
        assert_eq!(format_inline_code("`"), "`` ` ``");
        assert_eq!(format_inline_code("``"), "``` `` ```");
    }
// TSZ_INLINE_TEST_END 6261dcb561a1c9e761aa0bbe527fd043ac2808f793f49a7cafd91cb4d51cd106
