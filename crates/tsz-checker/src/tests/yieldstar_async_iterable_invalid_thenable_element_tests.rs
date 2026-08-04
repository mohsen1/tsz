//! Regression tests for #16378: TS1322 never fired for `yield*` of an async
//! iterable whose **iterated element** is an invalid thenable.
//!
//! Structural rule: when an `async function*` delegates with `yield*` to an
//! async iterable, `tsc` awaits each iterated element and validates it as a
//! thenable — distinct from the TS1320 check on the delegated iterator's own
//! `next()` **result**. If the element type is thenable (`isThenableType`
//! holds) but not a valid promise (`getPromisedTypeOfPromiseEx` recovers
//! nothing), it reports TS1322 at the `yield*` operand.
//!
//! tsz already has the exact predicate this needs —
//! `CheckerState::await_operand_is_invalid_thenable`, landed by #16374 for
//! the plain `await`/`yield` operand and TS1320's `next()`-result check — but
//! `dispatch/yield_::check_yield_expression`'s async `yield*` arm never
//! applied it to the resolved iterated element.
//!
//! One case does NOT reuse that predicate as-is: a **union** element type.
//! Oracle-verified against pinned `typescript@7.0.2`
//! (`--noEmit --strict --pretty false`), `AsyncIterable<BadThenable | number>`
//! is clean (the `number` branch resolves, and iterated-element resolution
//! keeps whichever union branches succeed), while
//! `AsyncIterable<BadThenable | OtherBadThenable>` (every branch invalid)
//! reports TS1322. That is the opposite of `await`'s union handling, where a
//! *single* bad branch is enough (`await (badThenable as BadThenable | number)`
//! still reports TS1320). See `yield_star_element_is_invalid_thenable`'s doc
//! comment in `checkers/promise_checker.rs` for why the two call sites
//! genuinely differ, not a special case invented for this fix.

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

const BAD_THENABLE: &str = "interface BadThenable { then(cb: string): void }";

/// Positive: the witness from the issue — a non-callable `onfulfilled`
/// parameter on the iterated element.
#[test]
fn yieldstar_async_iterable_invalid_thenable_element_reports_ts1322() {
    let codes = strict_codes(&format!(
        r#"
export {{}};
{BAD_THENABLE}
declare const zzSource: AsyncIterable<BadThenable>;
async function* g() {{
  yield* zzSource;
}}
"#,
    ));
    assert!(
        codes.contains(&1322),
        "an invalid-thenable iterated element must report TS1322: {codes:?}"
    );
}

/// Renamed/wrapper control: a differently-named interface and generator must
/// still report TS1322 — proves the check is not keyed off identifiers.
#[test]
fn yieldstar_invalid_thenable_element_in_renamed_generator_reports_ts1322() {
    let codes = strict_codes(
        r#"
export {};
interface MyBadElement { then(cb: string): void }
declare const delegateSource: AsyncIterable<MyBadElement>;
async function* differentlyNamedGenerator() {
  yield* delegateSource;
}
"#,
    );
    assert!(
        codes.contains(&1322),
        "a renamed interface with the same invalid-thenable shape must still report TS1322: {codes:?}"
    );
}

/// Negative: a valid thenable element (payload extractable) must not report.
#[test]
fn yieldstar_valid_thenable_element_does_not_report_ts1322() {
    let codes = strict_codes(
        r#"
export {};
interface GoodThenable { then(cb: (value: number) => void): void }
declare const zzSource: AsyncIterable<GoodThenable>;
async function* g() {
  yield* zzSource;
}
"#,
    );
    assert!(
        !codes.contains(&1322),
        "a valid thenable element must not report TS1322: {codes:?}"
    );
}

/// Negative: a real `Promise<T>` element must not report.
#[test]
fn yieldstar_promise_element_does_not_report_ts1322() {
    let codes = strict_codes(
        r#"
export {};
declare const zzSource: AsyncIterable<Promise<number>>;
async function* g() {
  yield* zzSource;
}
"#,
    );
    assert!(
        !codes.contains(&1322),
        "a real Promise element must not report TS1322: {codes:?}"
    );
}

/// Negative: a plain non-thenable element must not report.
#[test]
fn yieldstar_ordinary_element_does_not_report_ts1322() {
    let codes = strict_codes(
        r#"
export {};
declare const zzSource: AsyncIterable<number>;
async function* g() {
  yield* zzSource;
}
"#,
    );
    assert!(
        !codes.contains(&1322),
        "an ordinary non-thenable element must not report TS1322: {codes:?}"
    );
}

/// Fallback control: the identical invalid-thenable element delegated from a
/// *sync* `yield*` (no async generator, so no await happens at all) must not
/// report — the check is gated on the containing generator being async.
#[test]
fn yieldstar_invalid_thenable_element_in_sync_generator_does_not_report_ts1322() {
    let codes = strict_codes(&format!(
        r#"
export {{}};
{BAD_THENABLE}
declare const zzSource: BadThenable[];
function* g() {{
  yield* zzSource;
}}
"#,
    ));
    assert!(
        !codes.contains(&1322),
        "a sync generator's yield* delegate must not report TS1322: {codes:?}"
    );
}

/// This-rejected element: must report TS1322 here (not TS1320, which is the
/// delegated iterator's own `next()`-result code).
#[test]
fn yieldstar_this_rejected_element_reports_ts1322_not_ts1320() {
    let codes = strict_codes(
        r#"
export {};
interface ThisRejected { then(this: { required: string }, cb: (value: number) => void): void }
declare const zzSource: AsyncIterable<ThisRejected>;
async function* g() {
  yield* zzSource;
}
"#,
    );
    assert!(
        codes.contains(&1322),
        "a this-rejected iterated element must report TS1322: {codes:?}"
    );
    assert!(
        !codes.contains(&1320),
        "a this-rejected iterated element must report TS1322, not the next()-result code TS1320: {codes:?}"
    );
}

/// Union element where every branch is invalid: reports TS1322.
#[test]
fn yieldstar_union_element_all_branches_invalid_reports_ts1322() {
    let codes = strict_codes(
        r#"
export {};
interface BadOne { then(cb: string): void }
interface BadTwo { then(cb: number): void }
declare const zzSource: AsyncIterable<BadOne | BadTwo>;
async function* g() {
  yield* zzSource;
}
"#,
    );
    assert!(
        codes.contains(&1322),
        "a union element whose every branch is an invalid thenable must report TS1322: {codes:?}"
    );
}

/// Union element with one valid, non-thenable branch: the element-resolution
/// call site keeps the successful branch and does not report — unlike
/// `await`'s union handling (see the file-level doc comment), the branch
/// that resolves suppresses the diagnostic entirely.
#[test]
fn yieldstar_union_element_one_valid_branch_does_not_report_ts1322() {
    let codes = strict_codes(&format!(
        r#"
export {{}};
{BAD_THENABLE}
declare const zzSource: AsyncIterable<BadThenable | number>;
async function* g() {{
  yield* zzSource;
}}
"#,
    ));
    assert!(
        !codes.contains(&1322),
        "a union element with one resolving branch must not report TS1322: {codes:?}"
    );
}

/// Negative control that pins the asymmetry above: the same union type
/// `await`ed directly (not through a `yield*` element) still reports TS1320
/// on its one bad branch — proves the difference is the call site, not a
/// weakened predicate.
#[test]
fn await_of_same_union_still_reports_ts1320_on_one_bad_branch() {
    let codes = strict_codes(&format!(
        r#"
export {{}};
{BAD_THENABLE}
declare const zzSource: BadThenable | number;
async function f() {{
  await zzSource;
}}
"#,
    ));
    assert!(
        codes.contains(&1320),
        "await of a union with one invalid-thenable branch must still report TS1320: {codes:?}"
    );
}

/// User-defined async iterable (a class implementing `[Symbol.asyncIterator]`
/// directly) must report TS1322 for an invalid-thenable element just like the
/// lib `AsyncIterable`.
#[test]
fn yieldstar_user_defined_async_iterable_invalid_thenable_element_reports_ts1322() {
    let codes = strict_codes(&format!(
        r#"
export {{}};
{BAD_THENABLE}
class MyAsyncIterable {{
  [Symbol.asyncIterator](): AsyncIterator<BadThenable> {{
    return undefined as unknown as AsyncIterator<BadThenable>;
  }}
}}
declare const zzSource: MyAsyncIterable;
async function* g() {{
  yield* zzSource;
}}
"#,
    ));
    assert!(
        codes.contains(&1322),
        "a user-defined async iterable's invalid-thenable element must report TS1322: {codes:?}"
    );
}

/// A delegated `AsyncGenerator<T>` (rather than a directly-typed
/// `AsyncIterable`) must also report TS1322 for its invalid-thenable
/// yielded/element type.
///
/// Oracle-verified with a *declared* `AsyncGenerator<BadThenable>`, not a
/// call to another `async function*` inferring `BadThenable` from its own
/// `yield` — the latter reports TS1321 at the inner `yield` first, which
/// widens the inferred generator's yield type before the outer `yield*` ever
/// sees it, so the outer position stays clean (`element == TypeId::ANY`).
/// That's a real, separate tsc behavior, not a gap in this fix.
#[test]
fn yieldstar_delegated_async_generator_invalid_thenable_element_reports_ts1322() {
    let codes = strict_codes(&format!(
        r#"
export {{}};
{BAD_THENABLE}
declare const zzDelegate: AsyncGenerator<BadThenable>;
async function* outer() {{
  yield* zzDelegate;
}}
"#,
    ));
    assert!(
        codes.contains(&1322),
        "a delegated AsyncGenerator's invalid-thenable element must report TS1322: {codes:?}"
    );
}

/// Negative: a primitive-like element type carrying `then` through an
/// intersection must not report — `tsc`'s `isThenableType` excludes
/// primitives before probing `then` at all.
#[test]
fn yieldstar_primitive_like_intersection_element_does_not_report_ts1322() {
    let codes = strict_codes(
        r#"
export {};
declare const zzSource: AsyncIterable<number & { then: string }>;
async function* g() {
  yield* zzSource;
}
"#,
    );
    assert!(
        !codes.contains(&1322),
        "a primitive-like element must not report TS1322 even if it structurally carries `then`: {codes:?}"
    );
}
