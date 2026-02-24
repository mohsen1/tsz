//! Integration tests for comment preservation in emitter

use tsz_emitter::output::printer::PrintOptions;
use tsz_parser::ParserState;

#[test]
fn test_comment_between_call_arguments() {
    let source = r#"function test() {
    var x = foo(/*comment*/ "arg");
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    // Need to use Printer directly to set source text for comment preservation
    use tsz_emitter::output::printer::Printer;
    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    // The comment should be preserved before the string literal
    assert!(
        output.contains("/*comment*/"),
        "Comment should be preserved in output: {output}"
    );
}

#[test]
fn test_skip_whitespace_forward_only_skips_whitespace() {
    use tsz_emitter::emitter::Printer;
    use tsz_parser::parser::node::NodeArena;

    let arena = NodeArena::new();
    let mut printer = Printer::new(&arena);
    printer.set_source_text("  /*comment*/ text");

    // Should skip whitespace but not comments
    let result = printer.skip_whitespace_forward(0, 20);
    assert_eq!(result, 2); // Only skips the two spaces, stops at '/*'
}

#[test]
fn test_skip_whitespace_forward_no_whitespace() {
    use tsz_emitter::emitter::Printer;
    use tsz_parser::parser::node::NodeArena;

    let arena = NodeArena::new();
    let mut printer = Printer::new(&arena);
    printer.set_source_text("abc");

    // Should return start position when no whitespace
    let result = printer.skip_whitespace_forward(0, 3);
    assert_eq!(result, 0);
}

#[test]
fn test_block_comment_after_comma_in_multiline_object() {
    // Block comments after commas in multi-line object literals need a space before them
    let source = r#"var x = {
    a: 1, /* comment */
    b: 2
};"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    use tsz_emitter::output::printer::Printer;
    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    // The block comment should have a space before it (not "1,/* comment */")
    assert!(
        output.contains(", /* comment */") || output.contains(",  /* comment */"),
        "Should have space before block comment after comma. Got:\n{output}"
    );
}

/// When an erased type-only declaration (interface) is followed by a non-erased
/// statement (`;`) on the same line, trailing comments on the non-erased
/// statement must be preserved. Regression test for the initialization filter
/// that was over-consuming comments belonging to non-erased siblings.
///
/// Reproduces the pattern from `circularBaseTypes`:
///   `interface Foo {};  // Error`
/// tsc output: `; // Error`
/// Previous bug: `// Error` was stripped because the erased range for the
/// *next* erased statement captured it.
#[test]
fn trailing_comment_after_erased_interface_sibling_preserved() {
    // Simplified version of circularBaseTypes
    let source = "interface Foo {}; // keep this\nvar x = 1;\n";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    use tsz_emitter::output::printer::Printer;
    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    // The `; // keep this` comment must be preserved in output
    assert!(
        output.contains("// keep this"),
        "Trailing comment on non-erased sibling after erased interface should be preserved.\nOutput:\n{output}"
    );
}

/// When an erased declaration is followed by another erased declaration,
/// comments between them (leading trivia of the second erased decl) should
/// still be erased. This ensures the fix for preserving non-erased sibling
/// comments doesn't break erasure of inter-erased comments.
#[test]
fn comments_between_consecutive_erased_declarations_are_erased() {
    let source = "interface Foo {}\n// belongs to type Bar\ntype Bar = string;\nvar x = 1;\n";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    use tsz_emitter::output::printer::Printer;
    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    // The comment between two erased declarations should be erased
    assert!(
        !output.contains("belongs to type Bar"),
        "Comment between consecutive erased declarations should be erased.\nOutput:\n{output}"
    );
    // The runtime statement should still be present
    assert!(
        output.contains("var x = 1"),
        "Runtime statement should be preserved.\nOutput:\n{output}"
    );
}
