//! ES5 ordinary `for` assignment-target destructuring temp-ordering tests.
//!
//! Structural rule: hoisted assignment-destructuring temps created by ordinary
//! `for` initializer expressions claim their auto-temp names before inline temps
//! from block-scoped `for` header declarations in the same function/file scope.

use crate::context::emit::EmitContext;
use crate::emitter::{Printer as EmitterPrinter, PrinterOptions};
use crate::lowering::LoweringPass;
use tsz_common::ScriptTarget;
use tsz_parser::ParserState;

fn emit_es5(source: &str) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let options = PrinterOptions {
        target: ScriptTarget::ES5,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer =
        EmitterPrinter::with_transforms_and_options(&parser.arena, transforms, options);
    printer.set_source_text(source);
    printer.emit(root);
    printer.get_output().to_string()
}

#[test]
fn assignment_for_temps_precede_block_scoped_header_temps() {
    let source = "let a, b, c, i;\n\
         let source = [1, [2, 3]];\n\
         for ([a, [b, c] = [0, 0]] = source, i = 0; i < 1; i++) { }\n\
         for (let [a = 0, [b = 1, c = 2] = [0, 0]] = source, i = 0; i < 1; i++) { }\n\
         for ([a, [b, c] = [0, 0]] = getSource(), i = 0; i < 1; i++) { }\n";
    let output = emit_es5(source);

    assert!(
        output.contains("var _a, _b, _c, _d, _e;"),
        "Hoisted ordinary-for assignment temps should reserve the low names.\nOutput:\n{output}"
    );
    assert!(
        output.contains(
            "for (a = source[0], _a = source[1], _b = _a === void 0 ? [0, 0] : _a, b = _b[0], c = _b[1], i = 0;"
        ),
        "First assignment loop should replay the first reserved temps.\nOutput:\n{output}"
    );
    assert!(
        output.contains(
            "for (var _f = source[0], a_1 = _f === void 0 ? 0 : _f, _g = source[1], _h = _g === void 0 ? [0, 0] : _g, _j = _h[0], b_1 = _j === void 0 ? 1 : _j, _k = _h[1], c_1 = _k === void 0 ? 2 : _k, i_1 = 0;"
        ),
        "Block-scoped header temps should follow all hoisted assignment temps.\nOutput:\n{output}"
    );
    assert!(
        output.contains(
            "for (_c = getSource(), a = _c[0], _d = _c[1], _e = _d === void 0 ? [0, 0] : _d, b = _e[0], c = _e[1], i = 0;"
        ),
        "Later assignment loop should replay the remaining reserved source temp.\nOutput:\n{output}"
    );
}
