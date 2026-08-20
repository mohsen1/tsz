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
fn invalid_numeric_declaration_names_recover_runtime_statements() {
    let source = "\
namespace 42 {}
interface 7 {}
type 9 {}

export namespace 123 {}
export interface 456 {}
export type 789 {}
";
    let output = emit_es2015(source);
    let expected = "\
\"use strict\";
namespace;
42;
{ }
interface;
7;
{ }
type;
9;
{ }
namespace;
123;
{ }
interface;
456;
{ }
type;
789;
{ }
";
    assert_eq!(output, expected);
    assert!(
        !output.contains("export {};"),
        "Invalid exported numeric declarations should not synthesize an empty export.\nOutput:\n{output}"
    );
}
