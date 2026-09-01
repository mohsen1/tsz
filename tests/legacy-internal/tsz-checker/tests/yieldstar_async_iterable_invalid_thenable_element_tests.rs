//! Regression tests for #16378: `yield*` of an async iterable whose *element*
//! type is an invalid thenable never reported TS1322.
//!
//! Structural rule: when an `async function*` delegates with `yield*` to an
//! async iterable, `tsc` validates each iterated *element* the same way it
//! validates an `await` operand — thenable (`isThenableType`) but no
//! promised type recoverable (`getPromisedTypeOfPromiseEx`) — reporting its
//! own code, TS1322, distinct from TS1320 (the delegate's `next()` result)
//! and TS1321/TS1058 (plain `yield`/annotated return, #16374). tsz reuses
//! `CheckerState::await_operand_is_invalid_thenable`'s leaf rule through the
//! new `async_iterated_element_is_invalid_thenable` in
//! `checkers/promise_checker.rs`, wired into the `yield_expr.asterisk_token`
//! async-generator arm of `dispatch/yield_::check_yield_expression`.
//!
//! Two things the sibling codes' predicate could not be reused for verbatim,
//! both oracle-verified against `typescript@7.0.2`:
//!
//! 1. **Element resolution must go through the checker's env-aware
//!    `for_of_element_type`, not the solver-only fallback.** `get_iterator_info`
//!    cannot evaluate through the `TypeData::Lazy(DefId)` alias body every
//!    non-array/tuple lib iterable (`AsyncIterable<T>`, `AsyncGenerator<T>`)
//!    exposes its iterator member behind, so the solver-only fallback
//!    silently resolved the element to `ANY` and the check never fired on
//!    any lib-typed delegate — including the issue's own witness.
//! 2. **A union's constituents combine with opposite semantics from `await`.**
//!    `await_operand_is_invalid_thenable` reports as soon as *any* union
//!    member is an invalid thenable (correct for `await`, oracle-confirmed by
//!    #16374). At this position `tsc` instead reports only when *every*
//!    member is invalid — `AsyncIterable<Good | Bad>` is clean,
//!    `AsyncIterable<Bad1 | Bad2>` reports. Naively reusing the `await`
//!    predicate here is a false positive on the one-bad-branch shape.

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

// ---------------------------------------------------------------------------
// Positives: an invalid-thenable element reachable through the iterated
// element type.
// ---------------------------------------------------------------------------

/// The issue's own witness: a callable but non-callback `then` on a lib
/// `AsyncIterable<T>` element.
#[test]
fn yieldstar_lib_async_iterable_invalid_thenable_element_reports_ts1322() {
    let codes = strict_codes(
        r#"
export {};
interface BadT { then(cb: string): void }
declare const src: AsyncIterable<BadT>;
async function* ag1() { yield* src; }
"#,
    );
    assert!(
        codes.contains(&1322),
        "a lib AsyncIterable element with a non-callback `then` must report TS1322: {codes:?}"
    );
}

#[test]
fn yieldstar_optional_then_element_reports_ts1322() {
    let codes = strict_codes(
        r#"
export {};
interface BadT { then?: (cb: (v: number) => void) => void }
declare const src: AsyncIterable<BadT>;
async function* ag() { yield* src; }
"#,
    );
    assert!(
        codes.contains(&1322),
        "an optional `then` is thenable but has no raw call signature: {codes:?}"
    );
}

#[test]
fn yieldstar_then_with_no_parameters_element_reports_ts1322() {
    let codes = strict_codes(
        r#"
export {};
interface BadT { then(): void }
declare const src: AsyncIterable<BadT>;
async function* ag() { yield* src; }
"#,
    );
    assert!(codes.contains(&1322), "{codes:?}");
}

#[test]
fn yieldstar_then_with_non_callable_parameter_element_reports_ts1322() {
    let codes = strict_codes(
        r#"
export {};
interface BadT { then(cb: any): void }
declare const src: AsyncIterable<BadT>;
async function* ag() { yield* src; }
"#,
    );
    assert!(codes.contains(&1322), "{codes:?}");
}

/// The `this`-rejected sub-case: TS1322 here, not TS1320 — same predicate as
/// #16374, different code because of the operand position.
#[test]
fn yieldstar_this_rejected_element_reports_ts1322() {
    let codes = strict_codes(
        r#"
export {};
interface BadThenable<T> {
    then(this: { required: string }, onfulfilled?: ((value: T) => void) | null): void;
}
declare const src: AsyncIterable<BadThenable<string>>;
async function* ag() { yield* src; }
"#,
    );
    assert!(codes.contains(&1322), "{codes:?}");
}

/// User-defined async iterable (`[Symbol.asyncIterator]`/`next()`), not the
/// lib `AsyncIterable<T>` alias — exercises the `async_info.is_some()`
/// (structural) branch of element resolution rather than the
/// `for_of_element_type` fallback.
#[test]
fn yieldstar_user_defined_async_iterator_invalid_thenable_element_reports_ts1322() {
    let codes = strict_codes(
        r#"
export {};
interface BadT { then(cb: string): void }
interface UserAsyncIterator {
    [Symbol.asyncIterator](): UserAsyncIterator;
    next(): Promise<{ value: BadT; done: boolean }>;
}
declare const src: UserAsyncIterator;
async function* ag() { yield* src; }
"#,
    );
    assert!(codes.contains(&1322), "{codes:?}");
}

/// A delegated `async function*` (not a bare iterable binding) reaches the
/// same element-resolution path.
#[test]
fn yieldstar_delegated_async_generator_invalid_thenable_element_reports_ts1322() {
    let codes = strict_codes(
        r#"
export {};
interface BadT { then(cb: string): void }
declare function inner(): AsyncGenerator<BadT>;
async function* outer() { yield* inner(); }
"#,
    );
    assert!(codes.contains(&1322), "{codes:?}");
}

/// Every union member invalid: the whole union stays invalid.
#[test]
fn yieldstar_union_element_all_branches_invalid_reports_ts1322() {
    let codes = strict_codes(
        r#"
export {};
interface BadA { then(cb: string): void }
interface BadB { then(cb: number): void }
declare const src: AsyncIterable<BadA | BadB>;
async function* ag() { yield* src; }
"#,
    );
    assert!(
        codes.contains(&1322),
        "a union whose every member is an invalid thenable must still report: {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// Negatives: shapes `tsc` accepts, which the check must not claim.
// ---------------------------------------------------------------------------

#[test]
fn yieldstar_valid_thenable_element_reports_nothing() {
    let codes = strict_codes(
        r#"
export {};
interface GoodT { then(cb: (v: number) => void): void }
declare const src: AsyncIterable<GoodT>;
async function* ag() { yield* src; }
"#,
    );
    assert!(!codes.contains(&1322), "{codes:?}");
}

/// Load-bearing negative: a union with *one* valid branch is clean even
/// though the sibling branch alone would report — the opposite of `await`'s
/// union semantics (`await_union_with_one_invalid_thenable_branch_reports_ts1320`
/// in `invalid_thenable_no_fulfillment_payload_tests.rs`). Proves the fix
/// does not simply reuse the `await` predicate's union combination.
#[test]
fn yieldstar_union_element_one_invalid_branch_reports_nothing() {
    let codes = strict_codes(
        r#"
export {};
interface GoodT { then(cb: (v: number) => void): void }
interface BadT { then(cb: string): void }
declare const src: AsyncIterable<GoodT | BadT>;
async function* ag() { yield* src; }
"#,
    );
    assert!(
        !codes.contains(&1322),
        "one valid union member absorbs an invalid sibling at this position: {codes:?}"
    );
}

#[test]
fn yieldstar_promise_element_reports_nothing() {
    let codes = strict_codes(
        r#"
export {};
declare const src: AsyncIterable<Promise<number>>;
async function* ag() { yield* src; }
"#,
    );
    assert!(!codes.contains(&1322), "{codes:?}");
}

#[test]
fn yieldstar_plain_non_thenable_element_reports_nothing() {
    let codes = strict_codes(
        r#"
export {};
declare const src: AsyncIterable<number>;
async function* ag() { yield* src; }
"#,
    );
    assert!(!codes.contains(&1322), "{codes:?}");
}

/// A primitive never adopts a `then` member, however the intersection's
/// property lookup resolves it.
#[test]
fn yieldstar_primitive_intersection_element_reports_nothing() {
    let codes = strict_codes(
        r#"
export {};
declare const src: AsyncIterable<string & { then(cb: string): void }>;
async function* ag() { yield* src; }
"#,
    );
    assert!(!codes.contains(&1322), "{codes:?}");
}

/// A *synchronous* `yield*` never awaits its elements, so no invalid-thenable
/// check applies at all — control that keeps the fix scoped to
/// `is_async_generator`.
#[test]
fn sync_yieldstar_invalid_thenable_element_reports_nothing() {
    let codes = strict_codes(
        r#"
export {};
interface BadT { then(cb: string): void }
declare const src: Iterable<BadT>;
function* g() { yield* src; }
"#,
    );
    assert!(
        !codes.contains(&1322),
        "a synchronous generator never awaits its yield* elements: {codes:?}"
    );
}

/// Renamed-binder control: the same shape behind differently-named
/// interfaces and generator still reports, proving the rule is structural.
#[test]
fn yieldstar_invalid_thenable_element_through_renamed_interfaces_reports_ts1322() {
    let codes = strict_codes(
        r#"
export {};
interface WidgetElement { then(callbackParam: string): void }
type WidgetAlias = WidgetElement;
declare const widgetSource: AsyncIterable<WidgetAlias>;
async function* produceWidgets() { yield* widgetSource; }
"#,
    );
    assert!(codes.contains(&1322), "{codes:?}");
}
