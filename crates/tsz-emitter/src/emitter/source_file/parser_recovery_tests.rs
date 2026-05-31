use crate::emitter::{ModuleKind, Printer as EmitterPrinter, PrinterOptions};
use tsz_common::ScriptTarget;
use tsz_parser::ParserState;

fn emit_es2015(source: &str) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let options = PrinterOptions {
        target: ScriptTarget::ES2015,
        module: ModuleKind::None,
        ..Default::default()
    };
    let mut printer = EmitterPrinter::with_options(&parser.arena, options);
    printer.set_source_text(source);
    printer.emit(root);
    printer.get_output().to_string()
}

#[test]
fn unparsed_equality_token_recovers_following_assignment_operand() {
    let source = "} alpha = ( beta = gamma ==== 'value') {";
    let output = emit_es2015(source);
    let expected = "\
alpha = (beta = gamma === ) = 'value';
{ }
";
    assert_eq!(output, expected);
}
