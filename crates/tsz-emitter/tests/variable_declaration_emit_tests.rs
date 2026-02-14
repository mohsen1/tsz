use tsz_emitter::printer::PrintOptions;
use tsz_parser::ParserState;

#[test]
fn empty_let_declaration_has_no_space_before_semicolon() {
    let source = "\"use strict\";\nlet;";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    use tsz_emitter::printer::Printer;
    let mut printer = Printer::new(&parser.arena, PrintOptions::default());
    printer.print(root);
    let output = printer.finish().code;

    assert!(output.contains("\nlet;"), "unexpected output: {}", output);
    assert!(!output.contains("\nlet ;"), "unexpected output: {}", output);
}
