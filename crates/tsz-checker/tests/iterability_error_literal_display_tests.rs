//! A fresh primitive-literal operand that is not iterable keeps its own literal
//! type in the not-iterable diagnostic (TS2488 / TS2495 / TS2504), matching
//! `tsc`: `for (const x of 42)` reports `Type '42'`, not the widened
//! `Type 'number'`. Widening happens only for the element binding, never for the
//! message. Non-literal operands (a `number`-typed variable) and literal unions
//! are unaffected. Pinned against `tsc` 6.0.2. Closes #15366.

use std::sync::{Arc, OnceLock};
use tsz_binder::lib_loader::LibFile;
use tsz_checker::CheckerOptions;
use tsz_checker::test_utils::{check_source_with_libs_code_messages, load_default_lib_files};
use tsz_common::common::ScriptTarget;

fn libs() -> &'static Vec<Arc<LibFile>> {
    static LIBS: OnceLock<Vec<Arc<LibFile>>> = OnceLock::new();
    LIBS.get_or_init(load_default_lib_files)
}

/// The first not-iterable diagnostic message for `source` at `target`.
fn not_iterable_message(source: &str, target: ScriptTarget) -> String {
    check_source_with_libs_code_messages(
        source,
        "test.ts",
        CheckerOptions {
            target,
            ..CheckerOptions::default()
        },
        libs(),
    )
    .into_iter()
    .find(|(code, _)| matches!(code, 2488 | 2495 | 2504 | 2461))
    .map(|(_, message)| message)
    .unwrap_or_default()
}

fn modern(source: &str) -> String {
    not_iterable_message(source, ScriptTarget::ES2020)
}

#[test]
fn for_of_number_literal_keeps_literal() {
    let m = modern("for (const x of 42) {}");
    assert!(m.contains("Type '42'"), "got: {m}");
}

#[test]
fn for_of_boolean_literal_keeps_literal() {
    let m = modern("for (const x of true) {}");
    assert!(m.contains("Type 'true'"), "got: {m}");
}

#[test]
fn for_of_bigint_literal_keeps_literal() {
    let m = modern("for (const x of 123n) {}");
    assert!(m.contains("Type '123n'"), "got: {m}");
}

#[test]
fn for_of_negative_literal_keeps_literal() {
    let m = modern("for (const x of -5) {}");
    assert!(m.contains("Type '-5'"), "got: {m}");
}

#[test]
fn for_of_parenthesized_literal_keeps_literal() {
    let m = modern("for (const x of (7)) {}");
    assert!(m.contains("Type '7'"), "got: {m}");
}

#[test]
fn yield_star_number_literal_keeps_literal() {
    let m = modern("function* g() { yield* 42; }");
    assert!(m.contains("Type '42'"), "got: {m}");
}

#[test]
fn for_await_number_literal_keeps_literal() {
    let m = modern("async function f() { for await (const x of 42) {} }");
    assert!(m.contains("Type '42'"), "got: {m}");
}

#[test]
fn es5_for_of_number_literal_keeps_literal() {
    let m = not_iterable_message("for (const x of 42) {}", ScriptTarget::ES5);
    assert!(m.contains("Type '42'"), "got: {m}");
}

// --- controls: these must NOT change ---

#[test]
fn control_number_variable_stays_widened() {
    let m = modern("const n: number = 42; for (const x of n) {}");
    assert!(m.contains("Type 'number'"), "got: {m}");
    assert!(
        !m.contains("Type '42'"),
        "widened variable must not show a literal: {m}"
    );
}

#[test]
fn control_literal_union_is_preserved() {
    let m = modern("declare const p: boolean; for (const x of (p ? 1 : 2)) {}");
    assert!(m.contains("Type '1 | 2'"), "got: {m}");
}
