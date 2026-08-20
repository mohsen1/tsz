//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-emitter/src/declaration_emitter/helpers/type_inference_parts/part4.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN a5308beffc42198eaf9f19e37429e81cb4396f6af49e2af0b725445e5890d412 183 removes_fully_enclosing_outer_parentheses
    #[test]
    fn removes_fully_enclosing_outer_parentheses() {
        assert_eq!(strip("(Cond<string>)"), "Cond<string>");
        assert_eq!(strip("((Cond<string>))"), "Cond<string>");
        assert_eq!(strip("(  Cond<string>  )"), "Cond<string>");
        assert_eq!(strip("(() => void)"), "() => void");
        assert_eq!(strip("(A | B)"), "A | B");
    }
// TSZ_INLINE_TEST_END a5308beffc42198eaf9f19e37429e81cb4396f6af49e2af0b725445e5890d412

// TSZ_INLINE_TEST_BEGIN 61b621776f952565034bd8825658ba9d5c624fa5f97ff36671d8f74eeef9bd28 192 leaves_unparenthesized_text_unchanged
    #[test]
    fn leaves_unparenthesized_text_unchanged() {
        assert_eq!(strip("Cond<string>"), "Cond<string>");
        assert_eq!(strip("() => void"), "() => void");
        assert_eq!(strip("A | B"), "A | B");
    }
// TSZ_INLINE_TEST_END 61b621776f952565034bd8825658ba9d5c624fa5f97ff36671d8f74eeef9bd28

// TSZ_INLINE_TEST_BEGIN 0f7405dd7a148d11112a94eace09dea66e547e038a771710a2dbf38876da62e5 199 preserves_parentheses_that_do_not_wrap_the_whole_type
    #[test]
    fn preserves_parentheses_that_do_not_wrap_the_whole_type() {
        // Operand parentheses inside a larger type are not the outermost wrap.
        assert_eq!(strip("(() => void)[]"), "(() => void)[]");
        assert_eq!(strip("(A | B)[]"), "(A | B)[]");
        assert_eq!(strip("Array<(A | B)>"), "Array<(A | B)>");
        assert_eq!(strip("(A & B) & C"), "(A & B) & C");
        // Disjoint groups: the leading `(` does not pair with the trailing `)`.
        assert_eq!(strip("(A) | (B)"), "(A) | (B)");
    }
// TSZ_INLINE_TEST_END 0f7405dd7a148d11112a94eace09dea66e547e038a771710a2dbf38876da62e5

// TSZ_INLINE_TEST_BEGIN 989daef9be0d09cd5c13d57a9085ae643be82227642bb78f31a84faf7d66bfdd 210 ignores_parentheses_inside_string_and_import_segments
    #[test]
    fn ignores_parentheses_inside_string_and_import_segments() {
        // Literal-type strings and import specifiers may contain parens that must
        // not skew the enclosing-pair detection.
        assert_eq!(strip("(import(\"./m\").Foo)"), "import(\"./m\").Foo");
        assert_eq!(strip("\"(\" | \")\""), "\"(\" | \")\"");
        assert_eq!(strip("(\"(\" | \")\")"), "\"(\" | \")\"");
    }
// TSZ_INLINE_TEST_END 989daef9be0d09cd5c13d57a9085ae643be82227642bb78f31a84faf7d66bfdd
