use crate::emitter::{ModuleKind, Printer as EmitterPrinter, PrinterOptions};
use tsz_common::ScriptTarget;

fn parse_test_source(source: &str) -> (tsz_parser::ParserState, tsz_parser::parser::NodeIndex) {
    let mut parser = tsz_parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    (parser, root)
}

#[test]
fn commonjs_top_level_using_anonymous_default_legacy_class_uses_assignment_output() {
    let source =
        "export {};\ndeclare var dec: any;\nusing before = null;\n@dec\nexport default class {}\n";

    let (parser, root) = parse_test_source(source);
    let mut printer = EmitterPrinter::with_options(
        &parser.arena,
        PrinterOptions {
            module: ModuleKind::CommonJS,
            legacy_decorators: true,
            target: ScriptTarget::ES5,
            ..Default::default()
        },
    );
    printer.set_source_text(source);
    printer.emit(root);
    let output = printer.get_output().to_string();

    assert!(
        output.contains("default_1 = /** @class */"),
        "Anonymous default legacy class should assign the ES5 class IIFE to the hoisted binding.\nOutput:\n{output}"
    );
    assert!(
        output.contains("default_1 = __decorate(["),
        "Anonymous default legacy class should decorate the hoisted binding.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("var default_1 = /** @class */"),
        "Top-level using should not rely on rewriting a rendered variable declaration.\nOutput:\n{output}"
    );
}
