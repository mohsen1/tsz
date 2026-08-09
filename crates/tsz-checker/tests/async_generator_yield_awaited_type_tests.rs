//! Tests for `Awaited<T>` unwrapping of an async generator's yielded value.
//!
//! Structural rule: `tsc`'s `getYieldedTypeOfYieldExpression` unconditionally
//! wraps the computed yielded/delegated type in `getAwaitedType(...)` when the
//! enclosing generator is async — for both a plain `yield expr` and a
//! `yield* iterable`. The async generator runtime (`AsyncGeneratorYield`)
//! awaits a yielded value before handing it to the consumer, and for a
//! `yield*` over a plain (sync) iterable, `AsyncFromSyncIteratorObject` awaits
//! each delegated item the same way. `tsz` computed the raw iterated/operand
//! type without that final `Awaited` step, so `yield Promise.resolve(1)` and
//! `yield* [Promise.resolve(1)]` in an async generator both left the inferred
//! `TYield` as `Promise<number>` instead of `number`, producing spurious
//! TS2322 against any `AsyncIterable<number>`/`AsyncIterableIterator<number>`/
//! `AsyncIterator<number>` annotation.
//!
//! Owner: checker (`crates/tsz-checker/src/dispatch/yield_.rs`), reusing the
//! existing solver-backed `Awaited<T>` computation
//! (`CheckerState::compute_awaited_type`, already used for `await` operands).

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
// The regression: plain `yield` of a promise in an async generator.
// =========================================================================

#[test]
fn async_generator_plain_yield_of_promise_unwraps_to_awaited_type() {
    let libs = load_default_lib_files();
    let src = r#"
const f: () => AsyncIterable<number> = async function* () {
    yield Promise.resolve(1);
};
"#;
    assert!(
        !codes_with(src, &libs).contains(&2322),
        "an async generator's plain `yield` of a promise must be Awaited-unwrapped, not compared raw"
    );
}

#[test]
fn async_generator_plain_yield_of_promise_wrong_awaited_type_still_errors() {
    let libs = load_default_lib_files();
    let src = r#"
const f: () => AsyncIterable<string> = async function* () {
    yield Promise.resolve(1);
};
"#;
    assert!(
        codes_with(src, &libs).contains(&2322),
        "Awaited<Promise<number>> = number is still not assignable to the declared `string` yield type"
    );
}

// =========================================================================
// The regression: `yield*` delegating to a sync iterable of promises.
// =========================================================================

#[test]
fn async_generator_yield_star_sync_array_of_promises_unwraps_to_awaited_type() {
    let libs = load_default_lib_files();
    let src = r#"
const f: () => AsyncIterable<number> = async function* () {
    yield* [Promise.resolve(1)];
};
"#;
    assert!(
        !codes_with(src, &libs).contains(&2322),
        "`yield*` over a sync array of promises in an async generator must await each element"
    );
}

#[test]
fn async_generator_yield_star_sync_array_of_promises_against_iterable_iterator() {
    let libs = load_default_lib_files();
    let src = r#"
const f: () => AsyncIterableIterator<number> = async function* () {
    yield* [Promise.resolve(1)];
};
"#;
    assert!(!codes_with(src, &libs).contains(&2322));
}

#[test]
fn async_generator_yield_star_sync_array_of_promises_against_iterator() {
    let libs = load_default_lib_files();
    let src = r#"
const f: () => AsyncIterator<number, any, any> = async function* () {
    yield* [Promise.resolve(1)];
};
"#;
    assert!(!codes_with(src, &libs).contains(&2322));
}

#[test]
fn async_generator_yield_star_sync_array_of_promises_wrong_awaited_type_still_errors() {
    let libs = load_default_lib_files();
    let src = r#"
const f: () => AsyncIterable<string> = async function* () {
    yield* [Promise.resolve(1)];
};
"#;
    assert!(
        codes_with(src, &libs).contains(&2322),
        "Awaited<Promise<number>> = number is still not assignable to the declared `string` yield type"
    );
}

#[test]
fn async_generator_yield_star_sync_array_union_of_promise_and_plain_unwraps() {
    // `Promise<number> | number` distributes: `Awaited<Promise<number> | number> = number`.
    let libs = load_default_lib_files();
    let src = r#"
const f: () => AsyncIterable<number> = async function* () {
    yield* [Promise.resolve(1), 2];
};
"#;
    assert!(!codes_with(src, &libs).contains(&2322));
}

// =========================================================================
// Fallback: delegating to an already-async iterable must stay unaffected.
// =========================================================================

#[test]
fn async_generator_yield_star_another_async_generator_unaffected() {
    let libs = load_default_lib_files();
    let src = r#"
async function* inner() {
    yield 1;
}
const f: () => AsyncGenerator<number, void, unknown> = async function* () {
    yield* inner();
};
"#;
    assert!(
        !codes_with(src, &libs).contains(&2322),
        "delegating to an async iterable that already yields `number` (not `Promise<number>`) must not regress"
    );
}

// =========================================================================
// Negative control: a plain (non-async) generator must not Awaited-unwrap.
// =========================================================================

#[test]
fn sync_generator_plain_yield_of_promise_is_not_unwrapped() {
    let libs = load_default_lib_files();
    let src = r#"
function* f(): IterableIterator<Promise<number>> {
    yield Promise.resolve(1);
}
"#;
    assert!(
        !codes_with(src, &libs).contains(&2322),
        "a sync generator's yielded promise must stay `Promise<number>`, matching the declared type"
    );
}

#[test]
fn sync_generator_yield_star_of_promise_array_is_not_unwrapped() {
    let libs = load_default_lib_files();
    let src = r#"
function* f(): IterableIterator<Promise<number>> {
    yield* [Promise.resolve(1)];
}
"#;
    assert!(
        !codes_with(src, &libs).contains(&2322),
        "a sync generator's `yield*` element type must stay `Promise<number>`, matching the declared type"
    );
}

#[test]
fn sync_generator_yield_of_promise_against_unwrapped_type_still_errors() {
    // Negative control's negative: a sync generator declared to yield `number`
    // (not `Promise<number>`) must still report the mismatch — proves the
    // async-only guard isn't accidentally suppressing sync-generator checks.
    let libs = load_default_lib_files();
    let src = r#"
function* f(): IterableIterator<number> {
    yield Promise.resolve(1);
}
"#;
    assert!(codes_with(src, &libs).contains(&2322));
}
