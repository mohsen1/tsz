//! Regression tests for `System.register` `execute` async-ness.
//!
//! When a `module=system` source has top-level `await` — directly, via an
//! `await using` declaration, or via a downleveled `for await...of` loop — the
//! generated `execute` callback must be `async function`, otherwise the inlined
//! `await` operators produce non-runnable JS (a `node --check` `SyntaxError`).
//!
//! Structural rule: an `execute` body that inlines top-level statements
//! containing top-level await (not nested inside a function-like boundary)
//! must be emitted as `execute: async function () { ... }`.

use crate::context::emit::EmitContext;
use crate::emitter::{ModuleKind, Printer, PrinterOptions};
use crate::lowering::LoweringPass;
use tsz_common::ScriptTarget;

fn emit_system(source: &str, target: ScriptTarget) -> String {
    let mut parser = tsz_parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let mut printer = Printer::with_options(
        &parser.arena,
        PrinterOptions {
            module: ModuleKind::System,
            target,
            ..Default::default()
        },
    );
    printer.set_source_text(source);
    printer.emit(root);
    printer.get_output().to_string()
}

/// Lowered emit path so downlevel transforms (e.g. `for await...of`) run.
fn emit_system_lowered(source: &str, target: ScriptTarget) -> String {
    let mut parser = tsz_parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let options = PrinterOptions {
        module: ModuleKind::System,
        target,
        ..Default::default()
    };
    let ctx = EmitContext::with_options(options.clone());
    let emit_plan = LoweringPass::new(&parser.arena, &ctx).run_plan(root);
    let mut printer = Printer::with_emit_plan_and_options(&parser.arena, emit_plan, options);
    printer.set_source_text(source);
    printer.emit(root);
    printer.get_output().to_string()
}

#[test]
fn system_top_level_await_expression_makes_execute_async() {
    let output = emit_system("export const x = 1;\nawait x;\n", ScriptTarget::ES2017);
    assert!(
        output.contains("execute: async function () {"),
        "top-level await must mark execute async.\nOutput:\n{output}"
    );
    assert!(
        output.contains("await x;"),
        "await expression should survive in the async execute body.\nOutput:\n{output}"
    );
}

#[test]
fn system_no_top_level_await_keeps_execute_sync() {
    let output = emit_system("export const x = 1;\nx;\n", ScriptTarget::ES2017);
    assert!(
        output.contains("execute: function () {"),
        "no top-level await must keep execute synchronous.\nOutput:\n{output}"
    );
    assert!(
        !output.contains("execute: async function"),
        "execute must not be async without top-level await.\nOutput:\n{output}"
    );
}

#[test]
fn system_await_nested_in_function_keeps_execute_sync() {
    // The `await` lives inside an async function body — a fresh async context —
    // so it must NOT force the module wrapper's execute callback to be async.
    let output = emit_system(
        "export const x = 1;\nasync function run() { await x; }\nrun;\n",
        ScriptTarget::ES2017,
    );
    assert!(
        !output.contains("execute: async function"),
        "await inside a nested function must not make execute async.\nOutput:\n{output}"
    );
}

#[test]
fn system_top_level_await_inside_block_makes_execute_async() {
    // Top-level await reachable through nested control flow (an `if` block) still
    // forces the wrapper to be async.
    let output = emit_system(
        "export const x = 1;\nif (x) {\n    await x;\n}\n",
        ScriptTarget::ES2017,
    );
    assert!(
        output.contains("execute: async function () {"),
        "top-level await inside a block must mark execute async.\nOutput:\n{output}"
    );
}

#[test]
fn system_for_await_of_downlevel_makes_execute_async_es2015() {
    // `for await...of` downleveled to ES2015 emits `await iterator.next()` /
    // `await iterator.return()` inside execute, which requires async.
    let output = emit_system_lowered(
        "export {};\nconst arr = [Promise.resolve()];\nfor await (const item of arr) {\n    item;\n}\n",
        ScriptTarget::ES2015,
    );
    assert!(
        output.contains("execute: async function () {"),
        "downleveled top-level for-await-of must mark execute async.\nOutput:\n{output}"
    );
    assert!(
        output.contains("await "),
        "downleveled for-await-of should emit await operators.\nOutput:\n{output}"
    );
}

#[test]
fn system_for_await_of_renamed_binder_makes_execute_async_es2015() {
    // Renamed iteration variable (`element` instead of `item`) proves the rule
    // is structural (await-modifier on the for-of), not keyed on any identifier.
    let output = emit_system_lowered(
        "export {};\nconst xs = [Promise.resolve()];\nfor await (const element of xs) {\n    element;\n}\n",
        ScriptTarget::ES2015,
    );
    assert!(
        output.contains("execute: async function () {"),
        "renamed for-await-of binder must still mark execute async.\nOutput:\n{output}"
    );
}

#[test]
fn system_await_using_makes_execute_async() {
    let output = emit_system(
        "export {};\ndeclare const d: AsyncDisposable;\nawait using r = d;\nr;\n",
        ScriptTarget::ESNext,
    );
    assert!(
        output.contains("execute: async function () {"),
        "top-level await-using must mark execute async.\nOutput:\n{output}"
    );
}
