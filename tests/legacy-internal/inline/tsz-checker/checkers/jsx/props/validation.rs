//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/checkers/jsx/props/validation.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 7ea49b73d46092f361a05531f0024fdd616e2e3acbd0b537102d7eabb50e0225 1969 jsx_props_target_selection_avoids_anonymous_display_prefix_decision
    #[test]
    fn jsx_props_target_selection_avoids_anonymous_display_prefix_decision() {
        let source = include_str!("validation.rs");
        let formatted_member_call = ["format_type", "(member)"].join("");
        let starts_with_object = [".starts_with", "('{')"].join("");
        let inline_forbidden = format!("{formatted_member_call}{starts_with_object}");
        for forbidden in [
            inline_forbidden,
            [
                "let display = self.format_type(member);",
                "let is_anonymous = display.starts_with('{');",
            ]
            .join("\n"),
        ] {
            assert!(
                !source.contains(&forbidden),
                "JSX props target selection must use TypeId/query facts, \
                 not formatted anonymous-object display prefixes: found {forbidden}"
            );
        }
    }
// TSZ_INLINE_TEST_END 7ea49b73d46092f361a05531f0024fdd616e2e3acbd0b537102d7eabb50e0225
