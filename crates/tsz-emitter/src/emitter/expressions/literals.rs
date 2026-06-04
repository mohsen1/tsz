use super::super::*;

include!("literals_parts/part1.rs");
include!("literals_parts/part2.rs");

#[cfg(test)]
mod object_recovery_tests;

#[cfg(test)]
mod tests {
    use crate::emitter::{Printer, PrinterOptions};
    use tsz_common::ScriptTarget;

    fn parse_test_source(source: &str) -> (tsz_parser::ParserState, tsz_parser::parser::NodeIndex) {
        let mut parser = tsz_parser::ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        (parser, root)
    }

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
}
