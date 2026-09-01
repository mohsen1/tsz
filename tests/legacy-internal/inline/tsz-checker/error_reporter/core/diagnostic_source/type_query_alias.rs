//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/error_reporter/core/diagnostic_source/type_query_alias.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN d6eded5abc6fb1fcdaccf48eff23de86e07cc404a6725ced56b114d6817e0abf 963 collapses_runs_outside_literals
    #[test]
    fn collapses_runs_outside_literals() {
        assert_eq!(norm("P   &   Q"), "P & Q");
        assert_eq!(norm("A   |   B"), "A | B");
        assert_eq!(norm("readonly   number[]"), "readonly number[]");
    }
// TSZ_INLINE_TEST_END d6eded5abc6fb1fcdaccf48eff23de86e07cc404a6725ced56b114d6817e0abf

// TSZ_INLINE_TEST_BEGIN 562233621a90507bf1648bc681a490fc73ba38c5c7d57d73288494864b7ea04f 970 collapses_tabs_and_trims_but_keeps_newlines
    #[test]
    fn collapses_tabs_and_trims_but_keeps_newlines() {
        assert_eq!(norm("\tP\t&\tQ\t"), "P & Q");
        assert_eq!(norm("  P & Q  "), "P & Q");
        // A line break is preserved verbatim: the downstream sanitizer's
        // first-newline guard depends on it to reject multi-line annotations.
        assert_eq!(norm("P &\n  Q"), "P &\n Q");
        assert!(norm("A & C & {\n  f0: F0;\n}").contains('\n'));
    }
// TSZ_INLINE_TEST_END 562233621a90507bf1648bc681a490fc73ba38c5c7d57d73288494864b7ea04f

// TSZ_INLINE_TEST_BEGIN 26307a61d6a2a9a3c594d39ea99c606d2a63a5e918f1f1d674ba21b4de1949f0 980 already_canonical_is_idempotent
    #[test]
    fn already_canonical_is_idempotent() {
        assert_eq!(norm("P & Q"), "P & Q");
        assert_eq!(norm("{ a: number; b: string }"), "{ a: number; b: string }");
    }
// TSZ_INLINE_TEST_END 26307a61d6a2a9a3c594d39ea99c606d2a63a5e918f1f1d674ba21b4de1949f0

// TSZ_INLINE_TEST_BEGIN dd2670c497e25aafb15509af99ba5c8259061070837b97d1dbce9b365b1070fa 986 preserves_string_literal_interior
    #[test]
    fn preserves_string_literal_interior() {
        // A string-literal type's own spelling is never re-spaced.
        assert_eq!(norm(r#""a  b" | "c""#), r#""a  b" | "c""#);
        assert_eq!(norm("'x   y'  &  Q"), "'x   y' & Q");
        // An escaped closing quote does not end the literal early.
        assert_eq!(norm(r#""a\"  b""#), r#""a\"  b""#);
    }
// TSZ_INLINE_TEST_END dd2670c497e25aafb15509af99ba5c8259061070837b97d1dbce9b365b1070fa

// TSZ_INLINE_TEST_BEGIN 03579018b5ad57673fb1a98d29353c107d8d36cdf77891f38ad058bba165d435 995 preserves_template_literal_interior
    #[test]
    fn preserves_template_literal_interior() {
        assert_eq!(norm("`a  ${T}` | X"), "`a  ${T}` | X");
    }
// TSZ_INLINE_TEST_END 03579018b5ad57673fb1a98d29353c107d8d36cdf77891f38ad058bba165d435
