//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-solver/src/evaluation/evaluate_rules/keyof.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 5e9fe4e8a4cd27cfde31e444918d3006dd6fa83cd303755a7ceab8bf15721ee5 1421 numeric_and_quoted_numeric_keys_have_a_total_order
    #[test]
    fn numeric_and_quoted_numeric_keys_have_a_total_order() {
        let interner = TypeInterner::new();
        let name = interner.intern_string("1");
        let numeric = ExactLiteralPropertyKey {
            name,
            is_symbol_named: false,
            is_string_named: false,
        };
        let quoted = ExactLiteralPropertyKey {
            name,
            is_symbol_named: false,
            is_string_named: true,
        };
        let mut keys = [quoted, numeric];

        keys.sort_by_key(exact_property_key_sort_key);

        assert_eq!(keys, [numeric, quoted]);
    }
// TSZ_INLINE_TEST_END 5e9fe4e8a4cd27cfde31e444918d3006dd6fa83cd303755a7ceab8bf15721ee5
