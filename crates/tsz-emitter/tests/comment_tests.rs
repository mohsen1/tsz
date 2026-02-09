//! Integration tests for comment preservation in emitter

use tsz_emitter::printer::PrintOptions;
use tsz_parser::ParserState;

#[test]
fn test_comment_between_call_arguments() {
    let source = r#"function test() {
    var x = foo(/*comment*/ "arg");
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    // Need to use Printer directly to set source text for comment preservation
    use tsz_emitter::printer::Printer;
    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.set_source_text(source);
    printer.print(root);
    let output = printer.finish().code;

    // The comment should be preserved before the string literal
    if !output.contains("/*comment*/") {
        println!("Full output:\n{}", output);
    }
    assert!(
        output.contains("/*comment*/"),
        "Comment should be preserved in output: {}",
        output
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
