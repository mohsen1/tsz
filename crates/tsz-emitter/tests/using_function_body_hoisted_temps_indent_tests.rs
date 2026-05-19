//! Regression tests for resource-management lowering parity bugs.
//!
//! These tests pin two structural bugs in the `using` / `await using`
//! lowering pipeline that diverged from tsc:
//!
//! 1. Hoisted temp `var _a;` lines injected at the top of a function body
//!    must use the indent captured at the insertion point, not the indent
//!    that happens to be active when the insertion is replayed at the end
//!    of block emission. When a function body also contains a `using`
//!    declaration, the block-level `try { … }` wrapper increases indent
//!    before the closing pass runs — earlier code recomputed indent at
//!    that point and produced doubly-indented hoist lines.
//!
//! 2. The async-function disposable region must not emit
//!    `result_N = __disposeResources(env_N);` for pure-`using` regions.
//!    tsc only allocates `result_N` when an `await using` declaration is
//!    present (so the dispose result needs to be awaited); for plain
//!    `using`, tsc emits `__disposeResources(env_N);` as a bare expression
//!    statement and never declares `result_N`.
//!
//! Both shapes are independent of name choices — they should hold for any
//! identifier names a user picks, so the assertions probe the structural
//! invariants rather than a single fixture's exact text.

use tsz_common::common::{ModuleKind, ScriptTarget};
use tsz_emitter::context::emit::EmitContext;
use tsz_emitter::emitter::{Printer as EmitterPrinter, PrinterOptions};
use tsz_emitter::lowering::LoweringPass;
use tsz_parser::parser::ParserState;

fn parse_lower_emit(source: &str, opts: PrinterOptions) -> String {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let ctx = EmitContext::with_options(opts.clone());
    let transforms = LoweringPass::new(&parser.arena, &ctx).run(root);
    let mut printer = EmitterPrinter::with_transforms_and_options(&parser.arena, transforms, opts);
    printer.set_source_text(source);
    printer.emit(root);
    printer.get_output().to_string()
}

/// When a function body contains a `using` declaration, the lowered output
/// wraps the body in a `try { … }` and increases indent before any
/// statement is printed. The function-body hoist line (`var _a;` for the
/// object-literal computed-key temp) must still land at the body's indent
/// (one level), not at the inner try-block's indent (two levels).
#[test]
fn function_body_hoist_temps_use_body_indent_not_inner_try_indent() {
    // The computed-key temp `_a` is only synthesized when the target needs
    // ES5 object-literal lowering, which is also when the function-body
    // block-using try wrapper increases indent before any statement emits
    // — that combination is what triggered the doubly-indented hoist.
    let source = r#"
function f() {
    using d = { [Symbol.dispose]() {} };
}
export {};
"#;
    let opts = PrinterOptions {
        target: ScriptTarget::ES5,
        module: ModuleKind::ESNext,
        ..Default::default()
    };
    let output = parse_lower_emit(source, opts);

    assert!(
        output.contains("function f() {\n    var _a;\n"),
        "Hoisted `var _a;` for the disposable-resource computed key must sit \
         at the function body indent (4 spaces), not inside the inner try.\n\
         Output:\n{output}"
    );
    assert!(
        !output.contains("function f() {\n        var _a;"),
        "Hoisted `var _a;` must not pick up the inner try-block indent.\n\
         Output:\n{output}"
    );
}

/// Same invariant under a non-default name choice and ES5 target. The fix
/// is structural — it must hold regardless of the user-chosen identifier
/// and regardless of which downlevel target requires the lowering.
#[test]
fn function_body_hoist_temps_indent_holds_under_renamed_binding_and_es5_target() {
    let source = r#"
function g() {
    using resource = { [Symbol.dispose]() {} };
}
export {};
"#;
    let opts = PrinterOptions {
        target: ScriptTarget::ES5,
        module: ModuleKind::ESNext,
        ..Default::default()
    };
    let output = parse_lower_emit(source, opts);

    assert!(
        output.contains("function g() {\n    var _a;\n"),
        "ES5 hoist line for the disposable-resource temp must sit at body \
         indent.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("function g() {\n        var _a;"),
        "ES5 hoist line must not be doubly-indented.\nOutput:\n{output}"
    );
}

/// An async function with a plain `using` declaration must not declare or
/// assign to `result_N` in the lowered state machine. tsc emits
/// `__disposeResources(env_N);` as a bare expression statement.
#[test]
fn async_function_pure_using_dispose_does_not_capture_result() {
    let source = r#"
async function af() {
    using d = { [Symbol.dispose]() {} };
    await null;
}
export {};
"#;
    let opts = PrinterOptions {
        target: ScriptTarget::ES5,
        module: ModuleKind::ESNext,
        ..Default::default()
    };
    let output = parse_lower_emit(source, opts);

    assert!(
        output.contains("__disposeResources(env_1);"),
        "Pure-using region should call `__disposeResources(env_1);` as a \
         bare statement.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("result_1 = __disposeResources"),
        "Pure-using region must not assign the dispose result to result_1.\n\
         Output:\n{output}"
    );
    assert!(
        !output.contains(", result_1;") && !output.contains("var result_1"),
        "Pure-using region must not hoist `result_1` in the state-machine \
         local vars.\nOutput:\n{output}"
    );
}

/// An async function with `await using` still needs `result_N` so the
/// disposal can be awaited before endfinally. This guards against an
/// over-eager fix that would drop the capture in both cases.
#[test]
fn async_function_await_using_still_captures_result_for_await() {
    let source = r#"
async function af() {
    await using d = { async [Symbol.asyncDispose]() {} };
    await null;
}
export {};
"#;
    let opts = PrinterOptions {
        target: ScriptTarget::ES5,
        module: ModuleKind::ESNext,
        ..Default::default()
    };
    let output = parse_lower_emit(source, opts);

    assert!(
        output.contains("result_1 = __disposeResources(env_1);"),
        "`await using` region must assign the dispose result to result_1 \
         so it can be awaited.\nOutput:\n{output}"
    );
}

/// Plain (non-async) `using` inside a synchronous function uses the
/// block-level try/catch/finally lowering path (not the state machine).
/// In that path the finally block already emits a bare
/// `__disposeResources(env_N);` — this test pins both invariants together
/// so the function-body indent fix does not inadvertently regress the
/// dispose-call shape.
#[test]
fn sync_function_using_emits_bare_dispose_at_body_indent() {
    let source = r#"
function f() {
    using d = { [Symbol.dispose]() {} };
}
export {};
"#;
    let opts = PrinterOptions {
        target: ScriptTarget::ES5,
        module: ModuleKind::ESNext,
        ..Default::default()
    };
    let output = parse_lower_emit(source, opts);

    assert!(
        output.contains("__disposeResources(env_1);"),
        "Sync function `using` should still emit bare \
         `__disposeResources(env_1);` in finally.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("result_1 = __disposeResources"),
        "Sync function `using` should not assign the dispose result to \
         result_1.\nOutput:\n{output}"
    );
}
