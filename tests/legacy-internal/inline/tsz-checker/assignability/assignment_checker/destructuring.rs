//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/assignability/assignment_checker/destructuring.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 57979591813f8ec1f43f7fc72420c85fd59ef86f0cc74a9eb2c40db4e31ed818 1556 inline_type_property_offset_uses_whole_identifier_match
    #[test]
    fn inline_type_property_offset_uses_whole_identifier_match() {
        let line = "{ foobar: number; foo: string }";

        assert_eq!(
            CheckerState::inline_type_property_offset(line, "foo"),
            line.find("foo: string")
        );
    }
// TSZ_INLINE_TEST_END 57979591813f8ec1f43f7fc72420c85fd59ef86f0cc74a9eb2c40db4e31ed818

// TSZ_INLINE_TEST_BEGIN a898f7cfadcb0c6c24e61f354087883cd283be6025633fb4dbaab004ede8701f 1566 inline_type_property_offset_rejects_identifier_continuations
    #[test]
    fn inline_type_property_offset_rejects_identifier_continuations() {
        assert_eq!(
            CheckerState::inline_type_property_offset("{ $foo: string }", "foo"),
            None
        );
        assert_eq!(
            CheckerState::inline_type_property_offset("{ foo_bar: string }", "foo"),
            None
        );
    }
// TSZ_INLINE_TEST_END a898f7cfadcb0c6c24e61f354087883cd283be6025633fb4dbaab004ede8701f

// TSZ_INLINE_TEST_BEGIN e8caaa1621adfa5ff214baed18c52ed62c1745deb2d73977c53c7f86c9fcf0b1 1578 inline_type_property_offset_returns_none_for_empty_property_name
    #[test]
    fn inline_type_property_offset_returns_none_for_empty_property_name() {
        // Guard against an infinite loop when property_name is the empty string:
        // `find("")` returns Some(0), and match_end == match_start would never advance
        // search_start if the byte at match_end happened to be an identifier char.
        assert_eq!(
            CheckerState::inline_type_property_offset("{ a: string }", ""),
            None
        );
        assert_eq!(CheckerState::inline_type_property_offset("", ""), None);
    }
// TSZ_INLINE_TEST_END e8caaa1621adfa5ff214baed18c52ed62c1745deb2d73977c53c7f86c9fcf0b1
