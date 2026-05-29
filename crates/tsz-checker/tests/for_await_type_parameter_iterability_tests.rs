//! `for await ... of` over a **type parameter** must resolve the parameter to
//! its apparent type (constraint) when deciding async-iterability, exactly like
//! the sync `for ... of` path. Regression coverage for the false `TS2504`
//! emitted when the operand's type is a type parameter whose constraint is a
//! generic `AsyncIterableIterator<...>` / `AsyncIterable<...>` application.
//!
//! These run against the full default lib bundle (which includes
//! `es2018.asynciterable` / `es2018.asyncgenerator`) so `AsyncIterableIterator`
//! and friends are real global types, not stubs.

use std::sync::Arc;
use tsz_binder::lib_loader::LibFile;
use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_source_with_libs_code_messages, load_default_lib_files};
use tsz_common::common::ScriptTarget;

const TS2504: u32 = 2504;

fn check(source: &str, libs: &[Arc<LibFile>]) -> Vec<(u32, String)> {
    check_source_with_libs_code_messages(
        source,
        "test.ts",
        CheckerOptions {
            target: ScriptTarget::ES2018,
            ..CheckerOptions::default()
        },
        libs,
    )
}

fn assert_no_ts2504(source: &str, libs: &[Arc<LibFile>], context: &str) {
    let diags = check(source, libs);
    assert!(
        !diags.iter().any(|(code, _)| *code == TS2504),
        "{context}: expected no TS2504, got: {diags:#?}"
    );
}

fn assert_has_ts2504(source: &str, libs: &[Arc<LibFile>], context: &str) {
    let diags = check(source, libs);
    assert!(
        diags.iter().any(|(code, _)| *code == TS2504),
        "{context}: expected TS2504, got: {diags:#?}"
    );
}

#[test]
fn type_param_constrained_to_async_iterable_iterator_is_async_iterable() {
    let libs = load_default_lib_files();
    assert!(!libs.is_empty(), "default lib files must be available");
    assert_no_ts2504(
        r#"
async function f<T extends AsyncIterableIterator<number>>(t: T) {
    for await (const x of t) { void x; }
}
"#,
        &libs,
        "T extends AsyncIterableIterator<number>",
    );
}

#[test]
fn type_param_constrained_to_async_iterable_is_async_iterable() {
    let libs = load_default_lib_files();
    assert_no_ts2504(
        r#"
async function f<S extends AsyncIterable<string>>(s: S) {
    for await (const x of s) { void x; }
}
"#,
        &libs,
        "S extends AsyncIterable<string>",
    );
}

#[test]
fn type_param_constrained_to_async_generator_is_async_iterable() {
    let libs = load_default_lib_files();
    assert_no_ts2504(
        r#"
async function f<G extends AsyncGenerator<boolean>>(g: G) {
    for await (const x of g) { void x; }
}
"#,
        &libs,
        "G extends AsyncGenerator<boolean>",
    );
}

#[test]
fn nested_type_param_constrained_to_async_iterable_is_async_iterable() {
    // The constraint is itself a type parameter, so resolution must be
    // transitive. Renamed bound variables (`Outer`/`Inner`) prove the fix is
    // structural rather than keyed on a `T`/`U` spelling.
    let libs = load_default_lib_files();
    assert_no_ts2504(
        r#"
async function f<Outer extends AsyncIterableIterator<number>, Inner extends Outer>(t: Inner) {
    for await (const x of t) { void x; }
}
"#,
        &libs,
        "Inner extends Outer extends AsyncIterableIterator<number>",
    );
}

#[test]
fn type_param_constrained_to_intersection_with_async_iterable_is_async_iterable() {
    let libs = load_default_lib_files();
    assert_no_ts2504(
        r#"
async function f<T extends AsyncIterableIterator<number> & { extra: string }>(t: T) {
    for await (const x of t) { void x; }
}
"#,
        &libs,
        "T extends AsyncIterableIterator<number> & { extra }",
    );
}

#[test]
fn direct_async_iterable_iterator_remains_async_iterable() {
    // Regression guard: the non-type-parameter case already worked and must
    // keep working.
    let libs = load_default_lib_files();
    assert_no_ts2504(
        r#"
async function f(t: AsyncIterableIterator<number>) {
    for await (const x of t) { void x; }
}
"#,
        &libs,
        "direct AsyncIterableIterator<number>",
    );
}

#[test]
fn type_param_constrained_to_non_iterable_object_still_reports_ts2504() {
    // Negative case: an object-shaped constraint without [Symbol.asyncIterator]
    // is not async iterable, so tsc (and tsz) still report TS2504.
    let libs = load_default_lib_files();
    assert_has_ts2504(
        r#"
async function f<T extends { foo: number }>(t: T) {
    for await (const x of t) { void x; }
}
"#,
        &libs,
        "T extends { foo: number }",
    );
}

#[test]
fn type_param_constrained_to_number_still_reports_ts2504() {
    // Negative case: a primitive constraint is neither async- nor sync-iterable.
    let libs = load_default_lib_files();
    assert_has_ts2504(
        r#"
async function f<T extends number>(t: T) {
    for await (const x of t) { void x; }
}
"#,
        &libs,
        "T extends number",
    );
}
