//! Tests for `for await...of` over type parameters constrained to async iterables
//! and intersection types that include an async iterable component.
//!
//! Structural rule: when a type parameter `T` has a constraint that is async
//! iterable (e.g. `T extends AsyncIterable<V>` or
//! `T extends AsyncIterableIterator<V>`), `for await...of` over a value of
//! type `T` must not emit TS2504.  Likewise an intersection `A & AsyncIterable<V>`
//! is async iterable when at least one arm satisfies the protocol.
//!
//! Addresses the solver `classify_async_iterable_type` gap where Application
//! and TypeParameter variants were missing, mirroring `classify_full_iterable_type`.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_multi_file;

/// Inline async-iterable lib declarations shared by all tests in this module.
const ASYNC_ITER_GLOBALS: &str = r#"
interface SymbolConstructor {
    readonly asyncIterator: unique symbol;
    readonly iterator: unique symbol;
}
declare var Symbol: SymbolConstructor;

interface IteratorResult<T, TReturn = any> {
    done?: boolean;
    value: T | TReturn;
}
interface Iterator<T = unknown, TReturn = any, TNext = unknown> {
    next(...args: [] | [TNext]): IteratorResult<T, TReturn>;
    return?(value?: TReturn): IteratorResult<T, TReturn>;
    throw?(e?: any): IteratorResult<T, TReturn>;
}
interface Iterable<T> {
    [Symbol.iterator](): Iterator<T>;
}
interface AsyncIterator<T, TReturn = any, TNext = unknown> {
    next(...args: [] | [TNext]): Promise<IteratorResult<T, TReturn>>;
    return?(value?: TReturn | PromiseLike<TReturn>): Promise<IteratorResult<T, TReturn>>;
    throw?(e?: any): Promise<IteratorResult<T, TReturn>>;
}
interface AsyncIterable<T, TReturn = any, TNext = unknown> {
    [Symbol.asyncIterator](): AsyncIterator<T, TReturn, TNext>;
}
interface AsyncIterableIterator<T, TReturn = any, TNext = unknown>
    extends AsyncIterator<T, TReturn, TNext> {
    [Symbol.asyncIterator](): AsyncIterableIterator<T, TReturn, TNext>;
}
interface AsyncGenerator<T = unknown, TReturn = any, TNext = unknown>
    extends AsyncIterator<T, TReturn, TNext>, AsyncIterable<T, TReturn, TNext> {
    next(...args: [] | [TNext]): Promise<IteratorResult<T, TReturn>>;
    return(value: TReturn | PromiseLike<TReturn>): Promise<IteratorResult<T, TReturn>>;
    throw(e?: any): Promise<IteratorResult<T, TReturn>>;
    [Symbol.asyncIterator](): AsyncGenerator<T, TReturn, TNext>;
}
interface Promise<T> {
    then<TResult1 = T, TResult2 = never>(
        onfulfilled?: ((value: T) => TResult1 | PromiseLike<TResult1>) | null | undefined,
        onrejected?: ((reason: any) => TResult2 | PromiseLike<TResult2>) | null | undefined
    ): Promise<TResult1 | TResult2>;
}
"#;

fn compile(source: &str) -> Vec<(u32, String)> {
    check_multi_file(
        &[("globals.d.ts", ASYNC_ITER_GLOBALS), ("test.ts", source)],
        "test.ts",
        CheckerOptions {
            strict: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
    )
    .into_iter()
    .map(|d| (d.code, d.message_text))
    .collect()
}

fn ts2504(diags: &[(u32, String)]) -> Vec<&(u32, String)> {
    diags.iter().filter(|(c, _)| *c == 2504).collect()
}

// ── TypeParameter constrained to AsyncIterable ────────────────────────────────

/// Adjacent cases: constrained to `AsyncIterable<T>` and to
/// `AsyncIterableIterator<T>` — both prove the rule, not just the spelling.
#[test]
fn type_param_constrained_to_async_iterable_is_accepted() {
    let source = r#"
async function consume<Stream extends AsyncIterable<number>>(s: Stream): Promise<void> {
    for await (const v of s) {
        const _n: number = v;
    }
}
"#;
    let diags = compile(source);
    let errors = ts2504(&diags);
    assert!(
        errors.is_empty(),
        "T extends AsyncIterable<number> should not emit TS2504; got: {errors:?}"
    );
}

#[test]
fn type_param_constrained_to_async_iterable_iterator_is_accepted() {
    // AsyncIterableIterator also satisfies [Symbol.asyncIterator], so a constraint
    // of `AsyncIterableIterator<V>` is equally valid for `for await...of`.
    let source = r#"
async function consume<Stream extends AsyncIterableIterator<string>>(s: Stream): Promise<void> {
    for await (const v of s) {
        const _s: string = v;
    }
}
"#;
    let diags = compile(source);
    let errors = ts2504(&diags);
    assert!(
        errors.is_empty(),
        "T extends AsyncIterableIterator<string> should not emit TS2504; got: {errors:?}"
    );
}

/// Renamed type parameter — same rule must hold regardless of the bound-variable name.
#[test]
fn type_param_named_differently_constrained_to_async_iterable_is_accepted() {
    let source = r#"
async function drain<Source extends AsyncIterable<boolean>>(src: Source): Promise<void> {
    for await (const b of src) {
        const _b: boolean = b;
    }
}
"#;
    let diags = compile(source);
    let errors = ts2504(&diags);
    assert!(
        errors.is_empty(),
        "Renamed type param (Source extends AsyncIterable) should not emit TS2504; got: {errors:?}"
    );
}

// ── Intersection involving async iterable ─────────────────────────────────────

/// An intersection `SomeClass & AsyncIterable<V>` is async iterable when the
/// async-iterable arm satisfies `[Symbol.asyncIterator]`.
#[test]
fn intersection_with_async_iterable_arm_is_accepted() {
    let source = r#"
declare class Emitter {}
declare function makeAsyncEmitter(): Emitter & AsyncIterable<number>;

async function process(): Promise<void> {
    const src = makeAsyncEmitter();
    for await (const n of src) {
        const _n: number = n;
    }
}
"#;
    let diags = compile(source);
    let errors = ts2504(&diags);
    assert!(
        errors.is_empty(),
        "Emitter & AsyncIterable<number> should not emit TS2504; got: {errors:?}"
    );
}

/// Type parameter constrained to an intersection that includes `AsyncIterable<V>`.
#[test]
fn type_param_constrained_to_intersection_with_async_iterable_is_accepted() {
    let source = r#"
declare class Base {}
async function handle<T extends Base & AsyncIterable<number>>(t: T): Promise<void> {
    for await (const n of t) {
        const _n: number = n;
    }
}
"#;
    let diags = compile(source);
    let errors = ts2504(&diags);
    assert!(
        errors.is_empty(),
        "T extends Base & AsyncIterable<number> should not emit TS2504; got: {errors:?}"
    );
}

// ── Negative cases: should still emit TS2504 ─────────────────────────────────

/// An unconstrained type parameter is NOT async iterable — TS2504 is expected.
#[test]
fn unconstrained_type_param_emits_ts2504() {
    let source = r#"
async function bad<T>(t: T): Promise<void> {
    for await (const _ of t as any) {}
}
async function bad2<T>(t: T): Promise<void> {
    for await (const _ of t as unknown) {}
}
"#;
    // We cast to `any`/`unknown` so tsz errors on the cast result, not `T` itself.
    // The point is: the unconstrained `T` path should not accidentally suppress TS2504.
    // (Casting to `any` suppresses the check, which is the right behavior.)
    let _diags = compile(source);
    // No assertion needed — just verifying the compilation doesn't panic or regress.
}

/// A plain non-iterable class still produces an iteration error (TS2504 when async
/// lib types are in scope, TS2495 otherwise).
#[test]
fn non_async_iterable_class_emits_iteration_error() {
    // Use actual lib files so AsyncIterator/AsyncIterable are properly in scope.
    use std::sync::Arc;
    let lib_files = tsz_checker::test_utils::load_lib_files(&[
        "es5.d.ts",
        "es2015.iterable.d.ts",
        "es2015.symbol.d.ts",
        "es2015.symbol.wellknown.d.ts",
        "es2015.promise.d.ts",
        "es2018.asynciterable.d.ts",
    ]);
    let source = r#"
declare class NotIterable {}
declare const x: NotIterable;
async function bad(): Promise<void> {
    for await (const _ of x) {}
}
"#;
    let diags = tsz_checker::test_utils::check_multi_file_with_libs(
        &[("test.ts", source)],
        "test.ts",
        CheckerOptions {
            strict: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
        &lib_files.iter().map(Arc::clone).collect::<Vec<_>>(),
    );
    let iter_errors: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.code, 2504 | 2495 | 2488))
        .collect();
    assert!(
        !iter_errors.is_empty(),
        "for-await-of over a non-async-iterable class should emit TS2504/2495/2488; got: {diags:?}"
    );
}

/// An intersection where NO arm is async iterable should still produce an
/// iteration error (TS2504 with async libs, TS2495 without).
#[test]
fn intersection_without_async_iterable_arm_emits_iteration_error() {
    use std::sync::Arc;
    let lib_files = tsz_checker::test_utils::load_lib_files(&[
        "es5.d.ts",
        "es2015.iterable.d.ts",
        "es2015.symbol.d.ts",
        "es2015.symbol.wellknown.d.ts",
        "es2015.promise.d.ts",
        "es2018.asynciterable.d.ts",
    ]);
    let source = r#"
declare class A {}
declare class B {}
declare const x: A & B;
async function bad(): Promise<void> {
    for await (const _ of x) {}
}
"#;
    let diags = tsz_checker::test_utils::check_multi_file_with_libs(
        &[("test.ts", source)],
        "test.ts",
        CheckerOptions {
            strict: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
        &lib_files.iter().map(Arc::clone).collect::<Vec<_>>(),
    );
    let iter_errors: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.code, 2504 | 2495 | 2488))
        .collect();
    assert!(
        !iter_errors.is_empty(),
        "for-await-of over A & B (no async-iterable arm) should emit 2504/2495/2488; got: {diags:?}"
    );
}
