//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-emitter/src/emitter/expressions/literals.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 93ad37d82aa376e54d10ebdcc0c3ffbd484f6b0e155d56f54c263515ffc16728 1786 trailing_comma_preserved_in_single_line_object_literal
    /// tsc preserves trailing commas in single-line object literals.
    /// `{ a: 1, b: 2, }` must stay as `{ a: 1, b: 2, }`, not `{ a: 1, b: 2 }`.
    #[test]
    fn trailing_comma_preserved_in_single_line_object_literal() {
        let source = "var o = { a: 1, b: 2, };\n";

        let (parser, root) = parse_test_source(source);

        let mut printer = Printer::new(&parser.arena);
        printer.set_source_text(source);
        printer.emit(root);
        let output = printer.get_output().to_string();

        assert!(
            output.contains("{ a: 1, b: 2, }"),
            "Trailing comma should be preserved in single-line object literal.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END 93ad37d82aa376e54d10ebdcc0c3ffbd484f6b0e155d56f54c263515ffc16728

// TSZ_INLINE_TEST_BEGIN 4b55bd0b712d9e49b86cb748fb89fb40bfe42c9a9c58a1e779ad48e85b9e76dd 1804 no_trailing_comma_when_source_has_none
    /// Without a trailing comma in source, no trailing comma should be emitted.
    #[test]
    fn no_trailing_comma_when_source_has_none() {
        let source = "var o = { a: 1, b: 2 };\n";

        let (parser, root) = parse_test_source(source);

        let mut printer = Printer::new(&parser.arena);
        printer.set_source_text(source);
        printer.emit(root);
        let output = printer.get_output().to_string();

        assert!(
            output.contains("{ a: 1, b: 2 }"),
            "No trailing comma should be added when source has none.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END 4b55bd0b712d9e49b86cb748fb89fb40bfe42c9a9c58a1e779ad48e85b9e76dd

// TSZ_INLINE_TEST_BEGIN d4f71ac1c6ab2af9f67142f81ac3724b2898419ed437264af5f3937adb418b8b 1822 trailing_comma_preserved_in_object_binding_pattern
    /// Trailing comma in object binding pattern: `{ b1, } = expr`.
    #[test]
    fn trailing_comma_preserved_in_object_binding_pattern() {
        let source = "var { b1, } = { b1: 1, };\n";

        let (parser, root) = parse_test_source(source);

        let mut printer = Printer::new(&parser.arena);
        printer.set_source_text(source);
        printer.emit(root);
        let output = printer.get_output().to_string();

        assert!(
            output.contains("{ b1, }"),
            "Trailing comma should be preserved in object binding pattern.\nOutput:\n{output}"
        );
        assert!(
            output.contains("{ b1: 1, }"),
            "Trailing comma should be preserved in object literal initializer.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END d4f71ac1c6ab2af9f67142f81ac3724b2898419ed437264af5f3937adb418b8b

// TSZ_INLINE_TEST_BEGIN 4888d73bae1c6b1a7d7a54529bd458829a1034f5ebca8b9cd6d20b68d60eaac7 1846 trailing_comma_with_inline_comment_detected
    /// Trailing comma + inline comment detection: `x: 1, // comment` preserves comma.
    /// `find_token_end_before_trivia` treats `,` as non-trivia, so `token_end` is
    /// past the comma. The fallback comma detection must find it.
    #[test]
    fn trailing_comma_with_inline_comment_detected() {
        let source = "var b = {\n    x: 1, // comment\n};\n";

        let (parser, root) = parse_test_source(source);

        let mut printer = Printer::new(&parser.arena);
        printer.set_source_text(source);
        printer.emit(root);
        let output = printer.get_output().to_string();

        // The trailing comma must be preserved even when followed by an inline comment
        assert!(
            output.contains("x: 1,"),
            "Trailing comma should be preserved.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END 4888d73bae1c6b1a7d7a54529bd458829a1034f5ebca8b9cd6d20b68d60eaac7

// TSZ_INLINE_TEST_BEGIN c36c865f9c7b0ea31efb721ab3f547df2958e898135e86ec0a3178debe9e1c5a 1865 empty_object_literal_with_inner_comment_preserved
    /// Comment-only empty object literals should not collapse to `{}`.
    #[test]
    fn empty_object_literal_with_inner_comment_preserved() {
        let source = "var o = {\n    value: {\n        // keep\n    },\n};\n";

        let (parser, root) = parse_test_source(source);

        let mut printer = Printer::new(&parser.arena);
        printer.set_source_text(source);
        printer.emit(root);
        let output = printer.get_output().to_string();

        assert!(
            output.contains("{\n        // keep\n    }"),
            "Comment-only empty object literal should keep its multiline body.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END c36c865f9c7b0ea31efb721ab3f547df2958e898135e86ec0a3178debe9e1c5a

// TSZ_INLINE_TEST_BEGIN 998b07f60bfbecefbb9d72d18a8d6f777db35276f61777295e350b42442bb83c 1883 block_comment_between_properties_preserved
    /// Block comment between properties on same line should be preserved.
    #[test]
    fn block_comment_between_properties_preserved() {
        let source = "var o = {\n    a: 1, /* trailing */\n    b: 2\n};\n";

        let (parser, root) = parse_test_source(source);

        let mut printer = Printer::new(&parser.arena);
        printer.set_source_text(source);
        printer.emit(root);
        let output = printer.get_output().to_string();

        assert!(
            output.contains("1, /* trailing */"),
            "Block comment should stay on same line after comma.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END 998b07f60bfbecefbb9d72d18a8d6f777db35276f61777295e350b42442bb83c

// TSZ_INLINE_TEST_BEGIN 4ebad95948b2216412c15d568df18349262e738fe636e8969ddc9297b10c77e6 1900 es5_object_literal_recovery_shorthand_drops_initializer
    #[test]
    fn es5_object_literal_recovery_shorthand_drops_initializer() {
        let source = "var h = {\n    x = 1,\n    y = 2\n};\n";

        let (parser, root) = parse_test_source(source);

        let mut printer = Printer::with_options(
            &parser.arena,
            PrinterOptions {
                target: ScriptTarget::ES5,
                ..Default::default()
            },
        );
        printer.set_source_text(source);
        printer.emit(root);
        let output = printer.get_output().to_string();

        assert!(
            output.contains("x: x,"),
            "ES5 recovery shorthand should expand without its initializer.\nOutput:\n{output}"
        );
        assert!(
            output.contains("y: y"),
            "ES5 recovery shorthand should expand without its initializer.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("x: x = 1") && !output.contains("y: y = 2"),
            "ES5 recovery shorthand must not keep invalid assignment syntax.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END 4ebad95948b2216412c15d568df18349262e738fe636e8969ddc9297b10c77e6

// TSZ_INLINE_TEST_BEGIN b0050cc8a2b37ae88eb122f19307888f696730f2039f585132446d5706a80b5a 1931 object_literal_private_identifier_property_key_recovers_as_missing_name
    #[test]
    fn object_literal_private_identifier_property_key_recovers_as_missing_name() {
        let source = "var h = {\n    #secret: 3\n};\n";

        let (parser, root) = parse_test_source(source);

        let mut printer = Printer::with_options(
            &parser.arena,
            PrinterOptions {
                target: ScriptTarget::ES2015,
                ..Default::default()
            },
        );
        printer.set_source_text(source);
        printer.emit(root);
        let output = printer.get_output().to_string();

        assert!(
            output.contains("    : 3"),
            "Invalid private object-literal keys should print the missing-name recovery slot.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("#secret"),
            "Invalid private object-literal keys should not survive as property names.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END b0050cc8a2b37ae88eb122f19307888f696730f2039f585132446d5706a80b5a

// TSZ_INLINE_TEST_BEGIN 7a6bf22f76ae8b9cc8bcee36ca81f82df0638a3b9bbe472b3c2dd9607871a37b 1958 es5_object_literal_private_identifier_property_key_recovers_as_missing_name
    #[test]
    fn es5_object_literal_private_identifier_property_key_recovers_as_missing_name() {
        let source = "var h = {\n    #renamed: 3\n};\n";

        let (parser, root) = parse_test_source(source);

        let mut printer = Printer::with_options(
            &parser.arena,
            PrinterOptions {
                target: ScriptTarget::ES5,
                ..Default::default()
            },
        );
        printer.set_source_text(source);
        printer.emit(root);
        let output = printer.get_output().to_string();

        assert!(
            output.contains("    : 3"),
            "ES5 invalid private object-literal keys should print the same missing-name recovery slot.\nOutput:\n{output}"
        );
        assert!(
            !output.contains("#renamed"),
            "Recovery should be independent of the private identifier spelling.\nOutput:\n{output}"
        );
    }
// TSZ_INLINE_TEST_END 7a6bf22f76ae8b9cc8bcee36ca81f82df0638a3b9bbe472b3c2dd9607871a37b
