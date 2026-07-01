//! Down-leveled (`target: ES5`) async-function `__awaiter` callback layout
//! parity tests.
//!
//! `tsc` lowers an `async` function/expression to
//! `return __awaiter(this, void 0, void 0, function () { … })`, where the
//! callback body is a synthesized **single-line** block: its braces hug the
//! lone `return __generator(...)` statement (and any hoisted `var`
//! declarations) even though the inner state machine spans multiple lines —
//! `function () { [var a;] return __generator(this, function (_a) {` … `}); }`.
//!
//! The callback body is only broken onto multiple lines for the two structural
//! triggers `tsc` uses: a directive prologue (`"use strict";`) or a
//! `var _this = this;` lexical-this capture emitted inside the callback. A
//! multi-line *source* body and hoisted `var` groups do NOT force the
//! multi-line form.
//!
//! All differential expectations were verified against the pinned `tsc` 6.0.3
//! (`tsc in.ts --target es5`). Binder names are varied to keep the decision
//! structural (no identifier-string special-casing).

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
fn async_function_with_await_awaiter_callback_is_inline() {
    let output = emit_es5("async function run(x: number) { await x; return x + 1; }\n");
    assert!(
        output.contains(
            "return __awaiter(this, void 0, void 0, function () { return __generator(this, \
             function (_a) {"
        ),
        "Awaiter callback body should hug `return __generator(...)` on its opening line.\n\
         Output:\n{output}"
    );
    assert!(
        !output.contains("function () {\n"),
        "Awaiter callback body must not be broken onto its own line.\nOutput:\n{output}"
    );
}

#[test]
fn async_function_hoisted_var_stays_inline_before_generator() {
    // A body-local `let` hoists to a `var` declaration in the awaiter wrapper
    // scope; `tsc` keeps it inline: `function () { var a; return __generator(...`.
    let output = emit_es5("async function run(x: number) { let a = x + 1; await a; return a; }\n");
    assert!(
        output.contains(
            "return __awaiter(this, void 0, void 0, function () { var a; return __generator(this, \
             function (_a) {"
        ),
        "Hoisted `var` must stay inline on the awaiter callback's opening line.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("function () {\n        var a;"),
        "Hoisted `var` must not force the multi-line callback form.\nOutput:\n{output}"
    );
}

#[test]
fn async_function_for_of_hoists_group_inline() {
    // `for..of` down-leveling hoists a comma-joined `var _i, items_1, i;` group;
    // `tsc` emits the whole group inline before `return __generator`.
    let output =
        emit_es5("async function run(items: number[]) { for (const i of items) { await i; } }\n");
    assert!(
        output
            .contains("function () { var _i, items_1, i; return __generator(this, function (_a) {"),
        "A hoisted `var` group must be emitted inline (comma-joined).\nOutput:\n{output}"
    );
}

#[test]
fn async_function_directive_prologue_forces_multiline_callback() {
    // A `"use strict";` directive prologue is the one case where `tsc` breaks
    // the callback body across lines, emitting the directive before
    // `return __generator`.
    let output = emit_es5("async function run(x: number) { \"use strict\"; await x; return x; }\n");
    assert!(
        output.contains("function () {\n"),
        "A directive prologue must force the multi-line callback form.\nOutput:\n{output}"
    );
    assert!(
        output.contains("\"use strict\";"),
        "The directive prologue must be emitted inside the callback.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("function () { \"use strict\"; return __generator"),
        "The directive must not be collapsed onto the callback's opening line.\nOutput:\n{output}"
    );
}

#[test]
fn async_function_expression_hoisted_var_inline_is_binder_name_agnostic() {
    // Same structural rule with different binder names and function-expression
    // syntax — the decision is not keyed on any identifier string.
    let output =
        emit_es5("const zeta = async function (q: number) { let w = q; await w; return w; };\n");
    assert!(
        output.contains("function () { var w; return __generator(this, function (_a) {"),
        "Function-expression hoisted `var` must stay inline regardless of binder names.\n\
         Output:\n{output}"
    );
}
