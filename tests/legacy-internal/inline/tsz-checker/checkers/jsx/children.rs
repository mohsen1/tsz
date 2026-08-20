//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/checkers/jsx/children.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 7d7fbb2334dba1caea4a24902f34e1e517b142348b20b084776571c436ef0d15 1774 jsx_children_display_policy_avoids_formatted_type_name_decisions
    #[test]
    fn jsx_children_display_policy_avoids_formatted_type_name_decisions() {
        let source = include_str!("children.rs");
        for forbidden in [
            ["format_type(type_id)", " == ", "\"ReactChild\""].join(""),
            ["format_type(actual_child_type)", " == ", "\"Element\""].join(""),
        ] {
            assert!(
                !source.contains(&forbidden),
                "JSX children display policy must use TypeId/query facts, \
                 not formatted type-name comparisons: found {forbidden}"
            );
        }
    }
// TSZ_INLINE_TEST_END 7d7fbb2334dba1caea4a24902f34e1e517b142348b20b084776571c436ef0d15
