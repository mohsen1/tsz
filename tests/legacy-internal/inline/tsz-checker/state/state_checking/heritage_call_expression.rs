//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/state/state_checking/heritage_call_expression.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 415978fd77264d3ab17d03d6ff423d835d9a793e76810ec8260460edfad14f1c 554 test_prefers_explicit_type_argument_node_start
    #[test]
    fn test_prefers_explicit_type_argument_node_start() {
        let anchor = find_heritage_call_expression_type_argument_anchor_impl(15, Some(23), 5);
        assert_eq!(anchor, 15);
    }
// TSZ_INLINE_TEST_END 415978fd77264d3ab17d03d6ff423d835d9a793e76810ec8260460edfad14f1c

// TSZ_INLINE_TEST_BEGIN e3c7fab97fe17689cfa964549a99496e91c924804f97d13bb71e72ed162482a2 560 test_falls_back_to_call_start_when_source_text_missing
    #[test]
    fn test_falls_back_to_call_start_when_source_text_missing() {
        let anchor = find_heritage_call_expression_type_argument_anchor_impl(26, Some(2), 5);
        assert_eq!(anchor, 26);
    }
// TSZ_INLINE_TEST_END e3c7fab97fe17689cfa964549a99496e91c924804f97d13bb71e72ed162482a2

// TSZ_INLINE_TEST_BEGIN 9253e637f3e9c115f3ac37c3e0d28f80445cfb9524ab541625c9c7db5ea81032 566 test_falls_back_to_call_start_without_type_arguments
    #[test]
    fn test_falls_back_to_call_start_without_type_arguments() {
        let anchor = find_heritage_call_expression_type_argument_anchor_impl(15, None, 7);
        assert_eq!(anchor, 7);
    }
// TSZ_INLINE_TEST_END 9253e637f3e9c115f3ac37c3e0d28f80445cfb9524ab541625c9c7db5ea81032
