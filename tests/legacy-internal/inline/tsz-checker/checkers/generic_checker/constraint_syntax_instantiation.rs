//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/checkers/generic_checker/constraint_syntax_instantiation.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN f0c97bd82316d42cc908f30cabce5bec1d530f9309df6260261d018c6595d7d7 358 unknown_type_arg_detection_does_not_read_source_text
    #[test]
    fn unknown_type_arg_detection_does_not_read_source_text() {
        let source = include_str!("constraint_syntax_instantiation.rs");
        for forbidden in [
            ["node_text", "(type_arg_idx)"].join(""),
            ["text.trim()", " == ", "\"unknown\""].join(""),
        ] {
            assert!(
                !source.contains(&forbidden),
                "`unknown` type-argument detection must use syntax/name facts, \
                 not source text: found {forbidden}"
            );
        }
    }
// TSZ_INLINE_TEST_END f0c97bd82316d42cc908f30cabce5bec1d530f9309df6260261d018c6595d7d7
