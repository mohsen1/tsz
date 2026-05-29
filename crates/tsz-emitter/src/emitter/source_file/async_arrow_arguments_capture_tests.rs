//! Regression tests for ES5 lowering of async arrow functions that reference
//! `arguments`.
//!
//! Arrow functions do not own an `arguments` binding, so an async arrow whose
//! body references `arguments` must capture the enclosing function's
//! `arguments` into a `var arguments_N = arguments;` temp in the outer wrapper
//! function before the `__awaiter` call. The generator callback already rewrote
//! those references to the capture name, so without the wrapper capture the
//! emitted `arguments_N` is undefined.
//!
//! Witness: `asyncArrowFunctionCapturesArguments_es5(target=es5)`.

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
fn class_member_async_arrow_captures_arguments_in_wrapper() {
    // The reported witness shape: async arrow inside a class method forwarding
    // `arguments` to another call.
    let source = "class C {\n    method() {\n        function other() {}\n        const fn = async () => await other.apply(this, arguments);\n    }\n}\n";
    let output = emit_es5(source);

    assert!(
        output.contains("var arguments_1 = arguments;"),
        "async arrow referencing `arguments` must capture it in the wrapper.\nOutput:\n{output}"
    );
    assert!(
        output.contains("other.apply(this, arguments_1)"),
        "generator body should use the captured `arguments_1`.\nOutput:\n{output}"
    );
    // Capture must precede the `__awaiter(` call in the wrapper scope, not
    // appear inside the generator callback. (`__awaiter` also appears earlier as
    // the helper definition `var __awaiter = ...`, so anchor on the call site.)
    let capture_at = output.find("var arguments_1 = arguments;").unwrap();
    let awaiter_call_at = output
        .find("return __awaiter(")
        .expect("wrapper should call __awaiter");
    assert!(
        capture_at < awaiter_call_at,
        "`var arguments_1 = arguments;` must come before the `__awaiter` call.\nOutput:\n{output}"
    );
}

#[test]
fn class_member_async_arrow_block_body_with_param_captures_arguments() {
    // Equivalent shape, varied: block body + a parameter + `arguments[index]`.
    // Proves the rule is about the `arguments` reference, not the exact spelling
    // of the witness.
    let source = "class C {\n    m() {\n        const f = async (x: number) => { return await Promise.resolve(arguments[x]); };\n    }\n}\n";
    let output = emit_es5(source);

    assert!(
        output.contains("var arguments_1 = arguments;"),
        "async arrow with a parameter referencing `arguments` must still capture it.\nOutput:\n{output}"
    );
    assert!(
        output.contains("arguments_1[x]"),
        "generator body should index into the captured `arguments_1`.\nOutput:\n{output}"
    );
}

#[test]
fn class_member_async_arrow_without_arguments_has_no_capture() {
    // Negative case: an async arrow that does NOT reference `arguments` must not
    // gain a wrapper capture and must keep the compact expression-body form.
    let source = "class C {\n    m() {\n        const f = async (x: number) => { return await Promise.resolve(x); };\n    }\n}\n";
    let output = emit_es5(source);

    assert!(
        !output.contains("arguments_1"),
        "async arrow not referencing `arguments` must not capture it.\nOutput:\n{output}"
    );
    assert!(
        output.contains("__awaiter"),
        "async arrow should still lower through __awaiter.\nOutput:\n{output}"
    );
}
