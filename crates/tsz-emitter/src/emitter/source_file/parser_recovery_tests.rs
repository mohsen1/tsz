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

#[test]
fn invalid_private_name_indexed_access_recovers_as_declarator_tail() {
    let source = "const badForNow: C[#bar] = 3;";
    let output = emit_es2015(source);
    let expected = "\
const badForNow, #bar;
3;
";
    assert_eq!(output, expected);
}

#[test]
fn bodyless_global_class_member_recovery_instantiates_namespace() {
    let source = "\
class C {
    global x
}
";
    let output = emit_es2015(source);
    let expected = "\
class C {
}
var global;
(function (global) {
})(global || (global = {}));
x;
";
    assert_eq!(output, expected);
}
