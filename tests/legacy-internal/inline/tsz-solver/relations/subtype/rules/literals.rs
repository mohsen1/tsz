//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/relations/subtype/rules/literals.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 1cbf9cc7745417e5f91e4a236727d42ab2c02d8e17b68e4299dd3847f59e9537 1229 number_prefix_backtracks_invalid_exponent_to_marker
    #[test]
    fn number_prefix_backtracks_invalid_exponent_to_marker() {
        assert_eq!(find_number_length("1.5em"), 3);
        assert!(is_valid_number("1.5"));
        assert_eq!(find_number_length("1.5Em"), 3);
        assert_eq!(find_number_length("1.5e2em"), 5);
        assert!(is_valid_number("1.5e2"));
        assert_eq!(find_number_length("1e-em"), 1);
        assert_eq!(find_number_length("1.5e-em"), 3);
    }
// TSZ_INLINE_TEST_END 1cbf9cc7745417e5f91e4a236727d42ab2c02d8e17b68e4299dd3847f59e9537
