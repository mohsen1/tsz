//! Regression tests for #16116 item 1: a union `yield*` delegate inside an
//! `async function*` spuriously reported TS1320 even though `tsc` accepts it.
//!
//! Structural rule: `async_iterator_has_invalid_thenable_next_result`
//! (`checkers/iterable_checker.rs`) resolved `[Symbol.asyncIterator]`/`next()`
//! on the delegate's `TypeId` as a single receiver. For a union delegate that
//! reads an inconsistent cross-member result instead of distributing over the
//! constituents the way every other iterable predicate in this file already
//! does (see `is_array_or_tuple_type`/`has_string_constituent` a few hundred
//! lines below, which recurse via `union_members_for_type` and combine with
//! `all()`/`any()`). `tsc` accepts a union delegate as long as *every* member
//! has a valid async-iterator `next()` result, so the union is invalid only
//! when *some* member is -- the fix distributes with `any()`.
//!
//! Each "invalid" member below returns an anonymous `{ then(): void }` from
//! `next()` -- a callable `then` with no resolve-callback parameter, which
//! `extract_awaited_type_from_thenable` cannot extract a payload type from.
//! A `then` that *does* take an `(value) => void` callback (even on a bare
//! object type) is a well-formed thenable and must NOT trigger TS1320 -- see
//! `mere_thenable_shape_with_resolve_callback_is_not_invalid` below.

use crate::test_utils::{check_source_with_libs, load_default_lib_files};

fn strict_codes(source: &str) -> Vec<u32> {
    let libs = load_default_lib_files();
    check_source_with_libs(
        source,
        "test.ts",
        crate::context::CheckerOptions {
            strict: true,
            ..crate::context::CheckerOptions::default()
        },
        &libs,
    )
    .into_iter()
    .map(|diagnostic| diagnostic.code)
    .collect()
}

/// Core repro: two differently-instantiated `AsyncGenerator`s unioned as a
/// `yield*` delegate must not report TS1320 -- each member's `next()` result
/// is a well-formed promise.
#[test]
fn union_of_valid_async_generators_does_not_report_invalid_thenable() {
    let codes = strict_codes(
        r#"
export {};
declare const zzSource: AsyncGenerator<string> | AsyncGenerator<number>;
const d = async function* () {
    yield* zzSource;
};
"#,
    );
    assert!(
        !codes.contains(&1320),
        "a union of two valid AsyncGenerators must not report TS1320: {codes:?}"
    );
}

/// Renamed/wrapper control: the same union through user-declared interfaces
/// (not the lib `AsyncGenerator` alias), each returning a real `Promise`
/// from `next()`, proving the fix is about union distribution in general,
/// not something specific to the lib type's shape.
#[test]
fn union_of_valid_user_declared_async_iterators_does_not_report_invalid_thenable() {
    let codes = strict_codes(
        r#"
export {};
interface GoodAsyncIteratorA {
    [Symbol.asyncIterator](): GoodAsyncIteratorA;
    next(): Promise<{ value: string; done: boolean }>;
}
interface GoodAsyncIteratorB {
    [Symbol.asyncIterator](): GoodAsyncIteratorB;
    next(): Promise<{ value: number; done: boolean }>;
}
declare const src: GoodAsyncIteratorA | GoodAsyncIteratorB;
const d = async function* () {
    yield* src;
};
"#,
    );
    assert!(
        !codes.contains(&1320),
        "a union of two valid user-declared async iterators must not report TS1320: {codes:?}"
    );
}

/// Negative/fallback control: a union where one member's `next()` returns a
/// bare, argument-less thenable must still report TS1320 -- proves the fix
/// distributes with `any()` rather than vacuously returning `false` for
/// every union.
#[test]
fn union_with_one_invalid_thenable_member_still_reports_invalid_thenable() {
    let codes = strict_codes(
        r#"
export {};
interface GoodAsyncIterator {
    [Symbol.asyncIterator](): GoodAsyncIterator;
    next(): Promise<{ value: string; done: boolean }>;
}
interface BadAsyncIterator {
    [Symbol.asyncIterator](): BadAsyncIterator;
    next(): { then(): void };
}
declare const src: GoodAsyncIterator | BadAsyncIterator;
const d = async function* () {
    yield* src;
};
"#,
    );
    assert!(
        codes.contains(&1320),
        "a union with one invalid-thenable member must still report TS1320: {codes:?}"
    );
}

/// Baseline control: a single (non-union) valid `AsyncGenerator` delegate
/// stays clean, matching the issue's "each member alone is clean" witness.
#[test]
fn single_valid_async_generator_delegate_stays_clean() {
    let codes = strict_codes(
        r#"
export {};
declare const zzSource: AsyncGenerator<string>;
const d = async function* () {
    yield* zzSource;
};
"#,
    );
    assert!(
        !codes.contains(&1320),
        "a single valid AsyncGenerator delegate must not report TS1320: {codes:?}"
    );
}

/// Baseline control: a single (non-union) invalid-thenable delegate still
/// reports TS1320 on its own, independent of any union handling.
#[test]
fn single_invalid_thenable_delegate_still_reports_invalid_thenable() {
    let codes = strict_codes(
        r#"
export {};
interface BadAsyncIterator {
    [Symbol.asyncIterator](): BadAsyncIterator;
    next(): { then(): void };
}
declare const src: BadAsyncIterator;
const d = async function* () {
    yield* src;
};
"#,
    );
    assert!(
        codes.contains(&1320),
        "a single invalid-thenable delegate must report TS1320: {codes:?}"
    );
}

/// Fallback/false-negative guard: a bare object `then` that DOES take a
/// resolve callback is a well-formed thenable per `tsc`'s own rules (it can
/// extract an awaited payload type from the callback parameter), so it must
/// not be misclassified as invalid merely for lacking a named `Promise`
/// base. Distinguishes "no resolve callback" (invalid) from "any non-lib
/// thenable" (not automatically invalid).
#[test]
fn mere_thenable_shape_with_resolve_callback_is_not_invalid() {
    let codes = strict_codes(
        r#"
export {};
interface OkAsyncIterator {
    [Symbol.asyncIterator](): OkAsyncIterator;
    next(): { then(onfulfilled: (value: { value: string; done: boolean }) => void): void };
}
declare const src: OkAsyncIterator;
const d = async function* () {
    yield* src;
};
"#,
    );
    assert!(
        !codes.contains(&1320),
        "a thenable with a resolve callback must not report TS1320: {codes:?}"
    );
}
