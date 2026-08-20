//! ES5 async lowering: `var _this = this;` capture inside the `__awaiter`
//! callback.
//!
//! When a closure nested in an async body references `this`, the lowered
//! generator body renders that reference as `_this`, and tsc declares
//! `var _this = this;` inside the `__awaiter` callback (after the hoisted
//! `var` groups). The capture decision is an `IR`-level fact
//! (`needs_lexical_this_capture`), never a scan of rendered output.

use crate::context::emit::EmitContext;
use crate::emitter::{Printer as EmitterPrinter, PrinterOptions};
use crate::lowering::LoweringPass;
use tsz_common::ScriptTarget;

fn emit_es5(source: &str) -> String {
    let mut parser = tsz_parser::ParserState::new("test.ts".to_string(), source.to_string());
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
fn async_function_declaration_nested_arrow_captures_this() {
    let source = "async function f() {\n    await Promise.resolve();\n    const g = () => this;\n    g();\n}\n";
    let output = emit_es5(source);

    assert!(
        output.contains("var _this = this;"),
        "Nested arrow capturing `this` after an await must declare `var _this = this;` inside the __awaiter callback.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var g;\n        var _this = this;"),
        "The lexical-this capture belongs after the hoisted var groups (tsc placement).\nOutput:\n{output}"
    );
}

#[test]
fn async_function_expression_nested_arrow_captures_this() {
    let source = "const make = () => {\n    return async function inner() {\n        await Promise.resolve();\n        const h = () => this;\n        return h();\n    };\n};\n";
    let output = emit_es5(source);

    assert!(
        output.contains("var _this = this;"),
        "Async function expressions get the same lexical-this capture as declarations.\nOutput:\n{output}"
    );
}

#[test]
fn async_function_single_line_body_with_captured_this_is_not_inlined() {
    let source = "async function f() { return () => this; }\n";
    let output = emit_es5(source);

    assert!(
        output.contains("var _this = this;"),
        "A single-line body whose nested arrow captures `this` still needs the capture, so the inline wrapper format is unavailable.\nOutput:\n{output}"
    );
    assert!(
        output.contains("return [2 /*return*/, function () { return _this; }];"),
        "The nested arrow must reference the captured `_this`.\nOutput:\n{output}"
    );
}

#[test]
fn async_function_top_level_this_does_not_capture() {
    let source = "async function f() {\n    await Promise.resolve();\n    return this;\n}\n";
    let output = emit_es5(source);

    assert!(
        !output.contains("var _this = this;"),
        "`this` at the top level of the async body runs with the __awaiter thisArg; no capture is needed.\nOutput:\n{output}"
    );
}

#[test]
fn async_function_string_literal_mentioning_this_capture_does_not_capture() {
    let source =
        "async function f() {\n    await Promise.resolve();\n    return \"return _this\";\n}\n";
    let output = emit_es5(source);

    assert!(
        !output.contains("var _this = this;"),
        "A string literal spelling `return _this` must not trigger the capture; the decision is an `IR` fact, not an output scan.\nOutput:\n{output}"
    );
}
