//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-emitter/src/declaration_emitter/helpers/computed_declarations.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 5712e49a90a7a1fe1b6c8c3839d664a6e3c2dfbf6d802460ff0cd9c6456ba094 349 recursive_static_class_expression_elision_rewrites_exact_member_line
    #[test]
    fn recursive_static_class_expression_elision_rewrites_exact_member_line() {
        let printed = "{\n    new(): Root;\n    Root: any;\n}\n";

        let actual = DeclarationEmitter::elide_recursive_static_class_expression_member_text(
            printed, "Root",
        );

        assert_eq!(
            "{\n    new(): Root;\n    Root: /*elided*/ any;\n}\n",
            actual
        );
    }
// TSZ_INLINE_TEST_END 5712e49a90a7a1fe1b6c8c3839d664a6e3c2dfbf6d802460ff0cd9c6456ba094

// TSZ_INLINE_TEST_BEGIN 4184b37be8baf1022449e085088033b01a83669f6f7d1b39108718cc71f0d9c1 363 recursive_static_class_expression_elision_preserves_unmatched_text
    #[test]
    fn recursive_static_class_expression_elision_preserves_unmatched_text() {
        let printed = "{ Root: any; }\n    OtherRoot: any;\n";

        let actual = DeclarationEmitter::elide_recursive_static_class_expression_member_text(
            printed, "Root",
        );

        assert_eq!(printed, actual);
    }
// TSZ_INLINE_TEST_END 4184b37be8baf1022449e085088033b01a83669f6f7d1b39108718cc71f0d9c1

// TSZ_INLINE_TEST_BEGIN 18c88c721da1a4a0ee171e6fdc74f03a50432b16a4a716f487d94e72a0bf9498 374 recursive_static_class_expression_elision_rewrites_nested_constructor_member
    #[test]
    fn recursive_static_class_expression_elision_rewrites_nested_constructor_member() {
        let printed =
            "{\n    new (): {};\n    D: {\n        new (): {};\n        D: any;\n    };\n}";

        let actual =
            DeclarationEmitter::elide_recursive_static_class_expression_member_text(printed, "D");

        assert_eq!("{\n    new (): {};\n    D: /*elided*/ any;\n}", actual);
    }
// TSZ_INLINE_TEST_END 18c88c721da1a4a0ee171e6fdc74f03a50432b16a4a716f487d94e72a0bf9498
