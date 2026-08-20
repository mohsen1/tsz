//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-parser/src/parser/state_types.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 5de08e7184c6c8c7be0b2d4ffd60927fbd42e9423bb62061df79964a93094a03 1001 missing_constituent_after_separator_or_operator_reports_ts1110
    /// A consumed union/intersection separator or type operator with no following
    /// constituent must surface TS1110, regardless of the (varied) binder name.
    #[test]
    fn missing_constituent_after_separator_or_operator_reports_ts1110() {
        // Vary the leading type name so the rule is structural, not name-keyed.
        for lhs in ["string", "number", "Foo", "ns.Bar"] {
            assert_eq!(
                count_type_expected(&format!("type T = {lhs} |;")),
                1,
                "trailing `|` after `{lhs}` should report exactly one TS1110"
            );
            assert_eq!(
                count_type_expected(&format!("type T = {lhs} &;")),
                1,
                "trailing `&` after `{lhs}` should report exactly one TS1110"
            );
        }

        // Chained separators anchor the error at the final missing constituent.
        assert_eq!(count_type_expected("type T = string | number |;"), 1);
        // A consumed leading `|`/`&` is also a required constituent.
        assert_eq!(count_type_expected("type T = |;"), 1);
        assert_eq!(count_type_expected("type T = &;"), 1);

        // Required constituents reached through annotation/parameter positions.
        assert_eq!(count_type_expected("let x: string |;"), 1);
        assert_eq!(count_type_expected("function f(a: number |) {}"), 1);

        // Type operators require an operand.
        assert_eq!(count_type_expected("type U = keyof ;"), 1);
        assert_eq!(count_type_expected("type U = unique ;"), 1);
        assert_eq!(count_type_expected("type U = readonly ;"), 1);
        assert_eq!(count_type_expected("type U = keyof number |;"), 1);
    }
// TSZ_INLINE_TEST_END 5de08e7184c6c8c7be0b2d4ffd60927fbd42e9423bb62061df79964a93094a03

// TSZ_INLINE_TEST_BEGIN 73380aa5a22a4ccdf5f8fc9d4e2fabe1e0152360e40478d349bf5f21695541f8 1036 well_formed_types_report_no_ts1110
    /// Well-formed unions/intersections and a leading `|`/`&` followed by a real
    /// constituent must not regress into a spurious TS1110.
    #[test]
    fn well_formed_types_report_no_ts1110() {
        for source in [
            "type T = string | number;",
            "type T = string & number;",
            "type V = | string;",
            "type V = | string | number;",
            "type W = & Base;",
            "type K = keyof number;",
            "type R = readonly string[];",
            "declare const s: unique symbol;",
        ] {
            assert_eq!(
                count_type_expected(source),
                0,
                "`{source}` should not report TS1110"
            );
        }
    }
// TSZ_INLINE_TEST_END 73380aa5a22a4ccdf5f8fc9d4e2fabe1e0152360e40478d349bf5f21695541f8
