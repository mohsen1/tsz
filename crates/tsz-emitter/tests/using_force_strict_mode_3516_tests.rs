//! Regression test for issue #3516: when the `using`/`await using` downlevel
//! transform fires, the emitter must include a `"use strict";` prologue so
//! the lowered code runs in strict mode (matching tsc).

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

#[test]
fn script_with_using_declaration_emits_use_strict() {
    let source = "class R { [Symbol.dispose]() {} }\nusing r = new R();\n";
    let opts = PrinterOptions {
        target: ScriptTarget::ES2022,
        module: ModuleKind::None,
        ..Default::default()
    };
    let output = parse_lower_emit(source, opts);
    assert!(
        output.starts_with("\"use strict\";") || output.contains("\n\"use strict\";"),
        "Script using `using` must include `\"use strict\";` prologue.\nOutput:\n{output}"
    );
    // Sanity: the using transform must have actually fired, otherwise this
    // test would pass for the wrong reason.
    assert!(
        output.contains("__addDisposableResource"),
        "Expected the using transform to fire.\nOutput:\n{output}"
    );
}

#[test]
fn es5_top_level_await_using_reserves_resource_temps_before_catch_binding() {
    let source = r#"
await using a = { async [Symbol.asyncDispose]() {} };
try {
}
catch {
    await using b = { async [Symbol.asyncDispose]() {} };
}
finally {
    await using c = { async [Symbol.asyncDispose]() {} };
}
for (const x in {}) {
}
for (const x of []) {
    await using d = { async [Symbol.asyncDispose]() {} };
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
        output.contains("var _a, _b, _c, _d;"),
        "Resource initializer temps should be the only file-level hoisted temps.\nOutput:\n{output}"
    );
    assert!(
        output.contains("catch (_e)"),
        "The ES2019 synthetic catch binding should be allocated after reserved resource temps.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var b = __addDisposableResource(env_2, (_b = {},"),
        "The catch resource initializer should consume the second reserved hoisted temp.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var c = __addDisposableResource(env_3, (_c = {},"),
        "The finally resource initializer should consume the third reserved hoisted temp.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var d = __addDisposableResource(env_4, (_d = {},"),
        "The for-of body resource initializer should consume the last reserved hoisted temp.\nOutput:\n{output}"
    );
    assert!(
        output.contains("for (var _i = 0, _f = [];"),
        "The for-of array temp should be allocated after the catch binding.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var x = _f[_i];"),
        "A sibling for-in block binding should not force the for-of binding to be renamed.\nOutput:\n{output}"
    );
}

#[test]
fn es5_for_of_await_using_missing_binding_uses_disposable_region() {
    let source = r#"
declare const x: any[];
for (await using of x);
export async function test() {
    for (await using of x);
}
"#;
    let opts = PrinterOptions {
        target: ScriptTarget::ES5,
        module: ModuleKind::ESNext,
        ..Default::default()
    };
    let output = parse_lower_emit(source, opts);

    assert!(
        output.contains("for (var _i = 0, x_1 = x; _i < x_1.length; _i++)"),
        "The top-level malformed await using for-of should still use ES5 array indexing.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var _a_1 = x_1[_i];"),
        "The top-level loop should synthesize the per-iteration resource value temp.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var _a = __addDisposableResource(env_1, _a_1, true);"),
        "The recovered missing binding should be emitted through addDisposableResource.\nOutput:\n{output}"
    );
    assert!(
        output.contains("var _i, x_2, _a_2, env_2, _a, e_2, result_2;"),
        "The async function loop should hoist planned for-of disposable region locals.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_a_2 = x_2[_i];"),
        "The async function loop should stage the per-iteration value before registering it.\nOutput:\n{output}"
    );
    assert!(
        output.contains("_a = __addDisposableResource(env_2, _a_2, true);"),
        "The async function loop should register the recovered resource binding inside the state machine.\nOutput:\n{output}"
    );
}

// Sanity: a regular script without using must NOT spontaneously add
// "use strict" — that would be a regression from the existing default.
#[test]
fn script_without_using_keeps_existing_strict_emit_behavior() {
    let source = "var x = 1;\nx;\n";
    let opts = PrinterOptions {
        target: ScriptTarget::ES2022,
        module: ModuleKind::None,
        ..Default::default()
    };
    let output = parse_lower_emit(source, opts);
    assert!(
        !output.starts_with("\"use strict\";"),
        "Script without `using` must not gain a `use strict` prologue.\nOutput:\n{output}"
    );
}
