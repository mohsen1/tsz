use tsz_common::common::{ModuleKind, ScriptTarget};
use tsz_emitter::emitter::{Printer as EmitterPrinter, PrinterOptions};
use tsz_parser::ParserState;

#[test]
fn debugger_prefix_namespace_is_erased_as_ambient_declaration() {
    let source =
        "declare namespace debuggerX {\n    export const value: number;\n}\nconst keep = 1;\n";
    let mut parser = ParserState::new("a.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut printer = EmitterPrinter::with_options(
        &parser.arena,
        PrinterOptions {
            target: ScriptTarget::ES2020,
            module: ModuleKind::CommonJS,
            ..Default::default()
        },
    );
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        !output.contains("debugger;"),
        "`debuggerX` must not trigger debugger namespace recovery.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("namespace;"),
        "ambient namespace should be erased without recovery artifacts.\nOutput:\n{output}"
    );
    assert!(
        output.contains("const keep = 1;"),
        "following statements should still emit.\nOutput:\n{output}"
    );
}
