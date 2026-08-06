//! Tests for the TS2766 send-type check on `yield*` delegation.
//!
//! When a generator with a declared `TNext` delegates with `yield* e`, the
//! container forwards whatever its own caller sends straight into the
//! delegate's `next()`. `tsc` therefore requires the container's `TNext` to be
//! assignable to the delegate iterator's `TNext`, and reports TS2766 when it is
//! not:
//!
//! ```text
//! error TS2766: Cannot delegate iteration to value because the 'next' method
//! of its iterator expects type 'string', but the containing generator will
//! always send 'unknown'.
//! ```
//!
//! The regression these tests pin: an annotated container whose `TNext` is
//! `unknown` (the most common shape) delegating to an iterator with a concrete
//! `TNext` produced no diagnostic, because the send-type check short-circuited
//! on `unknown` before ever asking whether `unknown` is assignable to the
//! delegate's `next()` parameter (it is not).
//!
//! These run against the real bundled lib assets rather than hand-rolled
//! stubs: the container/delegate `next()` shapes come straight from
//! `Generator`/`AsyncGenerator`, and a generator function *expression* assigned
//! to a `const` only materializes correctly with the full iterator protocol
//! wired in.

use std::sync::Arc;

use crate::test_utils::{check_source_with_libs, load_default_lib_files, strict_checker_options};
use tsz_binder::lib_loader::LibFile;

fn codes_with(source: &str, libs: &[Arc<LibFile>]) -> Vec<u32> {
    check_source_with_libs(source, "test.ts", strict_checker_options(), libs)
        .iter()
        .map(|d| d.code)
        .collect()
}

// =========================================================================
// The regression: an `unknown`-TNext container into a concrete-TNext delegate.
// =========================================================================

#[test]
fn unknown_container_next_into_concrete_delegate_reports_ts2766() {
    let libs = load_default_lib_files();
    let src = r#"
declare function srcStr(): Generator<string, void, string>;
function* annotated(): Generator<string, void, unknown> {
    yield* srcStr();
}
"#;
    assert!(
        codes_with(src, &libs).contains(&2766),
        "unknown container TNext delegating to a `string` next() must report TS2766"
    );
}

#[test]
fn concrete_incompatible_container_next_reports_ts2766() {
    // container sends `number`, delegate's next() expects `string`.
    let libs = load_default_lib_files();
    let src = r#"
declare function srcStr(): Generator<string, void, string>;
function* annotated(): Generator<string, void, number> {
    yield* srcStr();
}
"#;
    assert!(codes_with(src, &libs).contains(&2766));
}

#[test]
fn union_container_next_not_assignable_reports_ts2766() {
    // container sends `string | number`, delegate expects only `string`.
    let libs = load_default_lib_files();
    let src = r#"
declare function srcStr(): Generator<string, void, string>;
function* annotated(): Generator<string, void, string | number> {
    yield* srcStr();
}
"#;
    assert!(codes_with(src, &libs).contains(&2766));
}

// =========================================================================
// Compatible containers must stay silent.
// =========================================================================

#[test]
fn matching_container_next_is_silent() {
    let libs = load_default_lib_files();
    let src = r#"
declare function srcStr(): Generator<string, void, string>;
function* annotated(): Generator<string, void, string> {
    yield* srcStr();
}
"#;
    assert!(!codes_with(src, &libs).contains(&2766));
}

#[test]
fn subtype_container_next_is_silent() {
    // container sends `string`, delegate accepts `string | number`.
    let libs = load_default_lib_files();
    let src = r#"
declare function srcStr(): Generator<string, void, string | number>;
function* annotated(): Generator<string, void, string> {
    yield* srcStr();
}
"#;
    assert!(!codes_with(src, &libs).contains(&2766));
}

#[test]
fn any_container_next_is_silent() {
    let libs = load_default_lib_files();
    let src = r#"
declare function srcStr(): Generator<string, void, string>;
function* annotated(): Generator<string, void, any> {
    yield* srcStr();
}
"#;
    assert!(!codes_with(src, &libs).contains(&2766));
}

// =========================================================================
// Delegates that declare no TNext must stay silent.
// =========================================================================

#[test]
fn array_delegate_is_silent() {
    let libs = load_default_lib_files();
    let src = r#"
declare function srcArr(): string[];
function* annotated(): Generator<string, void, unknown> {
    yield* srcArr();
}
"#;
    assert!(!codes_with(src, &libs).contains(&2766));
}

#[test]
fn container_annotated_without_next_type_is_silent() {
    // `Iterable<string>` has no `TNext`; the container's send type is unknowable,
    // so `tsc` falls back to `any` and reports nothing. This pins the
    // fallback: the unresolved container next type must default to `any`, not
    // `undefined` (which would manufacture a false-positive TS2766 against the
    // delegate's `string` next() parameter).
    let libs = load_default_lib_files();
    let src = r#"
declare function srcStr(): Generator<string, void, string>;
function* annotated(): Iterable<string> {
    yield* srcStr();
}
"#;
    assert!(!codes_with(src, &libs).contains(&2766));
}

// =========================================================================
// Async generators behave identically.
// =========================================================================

#[test]
fn async_unknown_container_next_reports_ts2766() {
    let libs = load_default_lib_files();
    let src = r#"
declare function srcStr(): AsyncGenerator<string, void, string>;
async function* annotated(): AsyncGenerator<string, void, unknown> {
    yield* srcStr();
}
"#;
    assert!(codes_with(src, &libs).contains(&2766));
}

#[test]
fn async_matching_container_next_is_silent() {
    let libs = load_default_lib_files();
    let src = r#"
declare function srcStr(): AsyncGenerator<string, void, string>;
async function* annotated(): AsyncGenerator<string, void, string> {
    yield* srcStr();
}
"#;
    assert!(!codes_with(src, &libs).contains(&2766));
}

// =========================================================================
// Generator methods and function expressions, and renamed binders.
// =========================================================================

#[test]
fn generator_method_unknown_container_reports_ts2766() {
    let libs = load_default_lib_files();
    let src = r#"
declare function srcStr(): Generator<string, void, string>;
class Host {
    *gen(): Generator<string, void, unknown> {
        yield* srcStr();
    }
}
"#;
    assert!(codes_with(src, &libs).contains(&2766));
}

#[test]
fn generator_function_expression_unknown_container_reports_ts2766() {
    let libs = load_default_lib_files();
    let src = r#"
declare function srcStr(): Generator<string, void, string>;
const gen = function* (): Generator<string, void, unknown> {
    yield* srcStr();
};
"#;
    assert!(codes_with(src, &libs).contains(&2766));
}

#[test]
fn renamed_binders_still_report_ts2766() {
    // Binder names carry no semantic weight: renaming everything must not change
    // the diagnostic.
    let libs = load_default_lib_files();
    let src = r#"
declare function produce(): Generator<string, void, string>;
function* consume(): Generator<string, void, unknown> {
    yield* produce();
}
"#;
    assert!(codes_with(src, &libs).contains(&2766));
}
