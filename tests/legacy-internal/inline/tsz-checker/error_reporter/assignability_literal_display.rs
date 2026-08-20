//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/error_reporter/assignability_literal_display.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 261b26ed180468e10ac2a3e53184deca57a33fd08272c5ef4b4a746f4c574a0c 53 boolean_member_literal_display_scan_ignores_string_literal_contents
    #[test]
    fn boolean_member_literal_display_scan_ignores_string_literal_contents() {
        assert!(display_has_boolean_member_literal_assignability(
            "{ c: true; }"
        ));
        assert!(display_has_boolean_member_literal_assignability(
            "{ c: false; }"
        ));
        assert!(!display_has_boolean_member_literal_assignability(
            r#"{ c: "foo: true"; }"#
        ));
        assert!(!display_has_boolean_member_literal_assignability(
            r#"{ c: 'foo: false'; }"#
        ));
        assert!(!display_has_boolean_member_literal_assignability(
            "{ c: trueish; }"
        ));
    }
// TSZ_INLINE_TEST_END 261b26ed180468e10ac2a3e53184deca57a33fd08272c5ef4b4a746f4c574a0c
