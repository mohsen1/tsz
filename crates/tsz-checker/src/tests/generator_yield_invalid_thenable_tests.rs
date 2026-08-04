//! Regression tests for #16291's "async/promise return types" family: TS1321
//! (`yield` operand in an async generator) was entirely unwired.
//!
//! Structural rule: `tsc` validates a plain `yield` operand inside an
//! `async function*` the same way it validates a real `await` operand —
//! "must either be a valid promise or must not contain a callable `then`
//! member" — but reports it under its own code, TS1321, rather than TS1320.
//! tsz already runs the right check for this
//! (`await_operand_invalid_thenable_this_type`, the same predicate the real
//! `await` path uses) but only ever wired it into the `await` call site
//! (`types/computation/access_await.rs`, TS1320); the plain-yield arm of
//! `dispatch/yield_::check_yield_expression` computed `expression_type` and
//! used it directly with no validation at all.
//!
//! Note on scope: tsz's `await_operand_invalid_thenable_this_type` only
//! implements the `this`-type-mismatch sub-case of "invalid thenable" (see
//! `crates/tsz-checker/tests/await_thenable_this_context_tests.rs`), not
//! tsc's full "callable `then` with no extractable payload" rule — so every
//! witness below uses the `this`-type-mismatch shape, oracle-confirmed
//! against `typescript@7.0.2` to report the code under test.

use crate::context::CheckerOptions;
use crate::test_utils::{check_source_with_libs, load_default_lib_files};

fn strict_codes(source: &str) -> Vec<u32> {
    let libs = load_default_lib_files();
    check_source_with_libs(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
        &libs,
    )
    .into_iter()
    .map(|diagnostic| diagnostic.code)
    .collect()
}

const BAD_THENABLE: &str = r#"
interface BadThenable<T> {
    then(this: { required: string }, onfulfilled?: ((value: T) => void) | null): void;
}
"#;

/// Positive: a plain `yield` of an invalid-thenable value inside an
/// `async function*` must report TS1321.
#[test]
fn plain_yield_of_invalid_thenable_in_async_generator_reports_ts1321() {
    let codes = strict_codes(&format!(
        r#"
export {{}};
{BAD_THENABLE}
declare const zzSource: BadThenable<string>;
async function* g(): AsyncGenerator<any> {{
  yield zzSource;
}}
"#,
    ));
    assert!(
        codes.contains(&1321),
        "an invalid-thenable yield operand in an async generator must report TS1321: {codes:?}"
    );
}

/// Renamed/wrapper control: the same shape under a different interface name
/// and a differently-named generator function must still report TS1321 —
/// proves the check is not keyed off the `BadThenable`/`g` identifiers.
#[test]
fn plain_yield_of_invalid_thenable_in_renamed_generator_reports_ts1321() {
    let codes = strict_codes(
        r#"
export {};
interface MyBad<T> {
    then(this: { required: string }, onfulfilled?: ((value: T) => void) | null): void;
}
declare const alias: MyBad<string>;
async function* generatorRenamed(): AsyncGenerator<any> {
  yield alias;
}
"#,
    );
    assert!(
        codes.contains(&1321),
        "a renamed interface with the same invalid-thenable shape must still report TS1321: {codes:?}"
    );
}

/// Negative: a plain `yield` of a valid `Promise<T>` inside an
/// `async function*` must not report TS1321.
#[test]
fn plain_yield_of_valid_promise_in_async_generator_does_not_report_ts1321() {
    let codes = strict_codes(
        r#"
export {};
declare const zzSource: Promise<string>;
async function* g(): AsyncGenerator<any> {
  yield zzSource;
}
"#,
    );
    assert!(
        !codes.contains(&1321),
        "a valid promise yield operand must not report TS1321: {codes:?}"
    );
}

/// Negative: a plain non-thenable value yielded from an `async function*`
/// must not report TS1321.
#[test]
fn plain_yield_of_ordinary_value_in_async_generator_does_not_report_ts1321() {
    let codes = strict_codes(
        r#"
export {};
async function* g(): AsyncGenerator<any> {
  yield 42;
}
"#,
    );
    assert!(
        !codes.contains(&1321),
        "an ordinary non-thenable yield operand must not report TS1321: {codes:?}"
    );
}

/// Fallback control: the identical invalid-thenable shape yielded from a
/// *sync* generator must not report TS1321 — the check is gated on the
/// generator being `async`.
#[test]
fn plain_yield_of_invalid_thenable_in_sync_generator_does_not_report_ts1321() {
    let codes = strict_codes(&format!(
        r#"
export {{}};
{BAD_THENABLE}
declare const zzSource: BadThenable<string>;
function* g(): Generator<any> {{
  yield zzSource;
}}
"#,
    ));
    assert!(
        !codes.contains(&1321),
        "a sync generator's yield operand must not report TS1321: {codes:?}"
    );
}

/// Negative control: a real `await` expression on the identical
/// invalid-thenable shape is untouched and keeps reporting TS1320, not
/// TS1321 — proves the new plain-yield check is additive, not a rename of
/// the existing `await` call site.
#[test]
fn await_of_invalid_thenable_still_reports_ts1320_not_ts1321() {
    let codes = strict_codes(&format!(
        r#"
export {{}};
{BAD_THENABLE}
declare const zzSource: BadThenable<string>;
async function f(): Promise<any> {{
  await zzSource;
}}
"#,
    ));
    assert!(
        codes.contains(&1320),
        "a real await operand with an invalid thenable must still report TS1320: {codes:?}"
    );
    assert!(
        !codes.contains(&1321),
        "a real await operand must not report the yield-only TS1321 code: {codes:?}"
    );
}
