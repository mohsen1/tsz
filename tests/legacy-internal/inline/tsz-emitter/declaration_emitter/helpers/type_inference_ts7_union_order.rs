//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-emitter/src/declaration_emitter/helpers/type_inference_ts7_union_order.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN db73cc75d68a38c3ee6ee89f7c63f9d26829a54a9da70a2955071c85e4177c58 331 numeric_literals_order_by_value_including_negatives
    #[test]
    fn numeric_literals_order_by_value_including_negatives() {
        assert_eq!(order(&["5", "-1", "3", "-2"]), ["-2", "-1", "3", "5"]);
    }
// TSZ_INLINE_TEST_END db73cc75d68a38c3ee6ee89f7c63f9d26829a54a9da70a2955071c85e4177c58

// TSZ_INLINE_TEST_BEGIN e346a40cef27b788175f4995ba4f9adb49ff15c35309583c0a95647b43e48022 336 string_literals_order_lexicographically
    #[test]
    fn string_literals_order_lexicographically() {
        assert_eq!(order(&["\"foo\"", "\"bar\""]), ["\"bar\"", "\"foo\""]);
    }
// TSZ_INLINE_TEST_END e346a40cef27b788175f4995ba4f9adb49ff15c35309583c0a95647b43e48022

// TSZ_INLINE_TEST_BEGIN c4567bcc26963516d1a66ff259396213e46f76e5c818b02fc2d01c941f01b513 341 literals_precede_object_and_typeof_members
    #[test]
    fn literals_precede_object_and_typeof_members() {
        // string-literal < number-literal < object bucket (`typeof`), and
        // `undefined` is pushed to the tail.
        assert_eq!(
            order(&["typeof a", "\"ok\"", "1", "undefined"]),
            ["\"ok\"", "1", "typeof a", "undefined"]
        );
    }
// TSZ_INLINE_TEST_END c4567bcc26963516d1a66ff259396213e46f76e5c818b02fc2d01c941f01b513

// TSZ_INLINE_TEST_BEGIN 788c14d0d80356d253a45a789e53031edb752b982a61b837636a972901fab24b 351 keyword_primitives_precede_literals
    #[test]
    fn keyword_primitives_precede_literals() {
        assert_eq!(order(&["1", "string", "number"]), ["string", "number", "1"]);
    }
// TSZ_INLINE_TEST_END 788c14d0d80356d253a45a789e53031edb752b982a61b837636a972901fab24b

// TSZ_INLINE_TEST_BEGIN 79542e6a61b7b3aa7a5c7843529e42aca1dd24e582e2c9c4c485b9d2b0db9042 356 object_bucket_members_keep_relative_order
    #[test]
    fn object_bucket_members_keep_relative_order() {
        assert_eq!(order(&["B", "A", "C"]), ["B", "A", "C"]);
    }
// TSZ_INLINE_TEST_END 79542e6a61b7b3aa7a5c7843529e42aca1dd24e582e2c9c4c485b9d2b0db9042

// TSZ_INLINE_TEST_BEGIN c939a2accac6b0806e18de4a560d0dfba727c039565b66ec85bd024cd34d154a 361 reorders_a_parenthesized_element_union
    #[test]
    fn reorders_a_parenthesized_element_union() {
        assert_eq!(
            DeclarationEmitter::reorder_ts7_unions_in_text("(2 | 4 | 1 | 3)[]"),
            "(1 | 2 | 3 | 4)[]"
        );
    }
// TSZ_INLINE_TEST_END c939a2accac6b0806e18de4a560d0dfba727c039565b66ec85bd024cd34d154a

// TSZ_INLINE_TEST_BEGIN 7fb533a71281f434a1fa13396c2dd250f8b71baf3ea42163cdc368b5e40e4170 369 reorders_a_top_level_union
    #[test]
    fn reorders_a_top_level_union() {
        assert_eq!(
            DeclarationEmitter::reorder_ts7_unions_in_text("1 | -1"),
            "-1 | 1"
        );
    }
// TSZ_INLINE_TEST_END 7fb533a71281f434a1fa13396c2dd250f8b71baf3ea42163cdc368b5e40e4170

// TSZ_INLINE_TEST_BEGIN 214f6cb1b6fe962279027aa9d604a9b6f26cebfa0c1e527404044371665f0ff9 377 arrow_return_union_is_not_split_as_a_top_level_union
    #[test]
    fn arrow_return_union_is_not_split_as_a_top_level_union() {
        // A bare function type is one member: the `|` binds into the arrow
        // return, so the whole type is preserved rather than being reordered as
        // if it were `((x: T) => "b") | "a"`.
        let text = "(x: T) => \"b\" | \"a\"";
        assert_eq!(
            DeclarationEmitter::split_top_level_union_members(text).len(),
            1
        );
        assert_eq!(DeclarationEmitter::reorder_ts7_unions_in_text(text), text);
    }
// TSZ_INLINE_TEST_END 214f6cb1b6fe962279027aa9d604a9b6f26cebfa0c1e527404044371665f0ff9

// TSZ_INLINE_TEST_BEGIN 545e1d88ad79ed4bad1984a66878af14fdfdcf29c578b041101748af95e99d9c 390 a_union_pipe_inside_a_string_literal_is_not_a_separator
    #[test]
    fn a_union_pipe_inside_a_string_literal_is_not_a_separator() {
        assert_eq!(
            DeclarationEmitter::split_top_level_union_members("\"a|b\" | \"c\""),
            ["\"a|b\"", "\"c\""]
        );
    }
// TSZ_INLINE_TEST_END 545e1d88ad79ed4bad1984a66878af14fdfdcf29c578b041101748af95e99d9c
