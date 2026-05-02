//! Tests for literal widening in unannotated async function return types.
//!
//! tsc widens literal return types before wrapping in `Promise<T>`:
//!   `async () => 0`        infers `() => Promise<number>` (not `Promise<0>`)
//!   `async () => { return "y"; }` infers `() => Promise<string>` (not `Promise<"y">`)
//!
//! tsz previously kept the literal form (`Promise<0>` / `Promise<"y">`), which
//! showed up as fingerprint-only divergence in TS2345 / TS2322 messages for
//! `f(async () => { return 0 })` where `f: (p: () => string) => void`.

use tsz_checker::test_utils::check_source_code_messages as get_diagnostics;

#[test]
fn async_arrow_literal_return_widens_in_ts2345() {
    // `f(async () => { return 0 })` against `(p: () => string) => void`:
    // the TS2345 diagnostic must display the inferred argument type with the
    // inner return widened to `number`, NOT preserved as literal `0`.
    // The Promise-wrapper name may render as `Promise<...>` (with lib types
    // loaded) or `object<...>` (the synthetic fallback used when no lib is
    // loaded) — either is fine; the invariant under test is widening.
    let source = r#"
declare function f(p: () => string): void;
f(async () => { return 0 });
"#;
    let diags = get_diagnostics(source);
    let ts2345: Vec<_> = diags.iter().filter(|(code, _)| *code == 2345).collect();
    assert!(
        !ts2345.is_empty(),
        "expected a TS2345 for async arrow argument mismatch, got: {diags:#?}"
    );
    let msg = ts2345[0].1.as_str();
    assert!(
        msg.contains("<number>"),
        "expected widened `<number>` in inferred async return, got: {msg}"
    );
    assert!(
        !msg.contains("<0>"),
        "literal return `0` must be widened to `number` before Promise wrapping, got: {msg}"
    );
}

#[test]
fn async_arrow_string_literal_return_widens() {
    // `async () => "y"` should widen inner to `string`, not preserve `"y"`.
    let source = r#"
declare function g(p: () => number): void;
g(async () => { return "y" });
"#;
    let diags = get_diagnostics(source);
    let ts2345: Vec<_> = diags.iter().filter(|(code, _)| *code == 2345).collect();
    assert!(
        !ts2345.is_empty(),
        "expected a TS2345 for async arrow string return vs number param, got: {diags:#?}"
    );
    let msg = ts2345[0].1.as_str();
    assert!(
        msg.contains("<string>"),
        "expected widened `<string>` in inferred async return, got: {msg}"
    );
    assert!(
        !msg.contains("<\"y\">"),
        "literal return `\"y\"` must be widened to `string` before Promise wrapping, got: {msg}"
    );
}

/// Inline lib stub: enough Promise / Awaited shape for the async-return
/// unwrap path to engage. The intrinsic `Awaited<T>` lib alias is the
/// natural conditional-type form; here we use a structurally-equivalent
/// alias so the test does not depend on lib loading.
const AWAITED_AND_ASYNC_GEN_PRELUDE: &str = r#"
type Awaited<T> =
    T extends null | undefined ? T :
    T extends object & { then(onfulfilled: infer F, ...args: infer _): any } ?
        F extends ((value: infer V, ...args: infer _) => any) ? Awaited<V> : never :
        T;

interface PromiseLike<T> {
    then<TResult1 = T, TResult2 = never>(
        onfulfilled?: ((value: T) => TResult1 | PromiseLike<TResult1>) | null,
        onrejected?: ((reason: any) => TResult2 | PromiseLike<TResult2>) | null
    ): PromiseLike<TResult1 | TResult2>;
}
interface Promise<T> {
    then<TResult1 = T, TResult2 = never>(
        onfulfilled?: ((value: T) => TResult1 | PromiseLike<TResult1>) | null,
        onrejected?: ((reason: any) => TResult2 | PromiseLike<TResult2>) | null
    ): Promise<TResult1 | TResult2>;
}
interface PromiseConstructor {
    resolve<T>(value: T): Promise<Awaited<T>>;
}
declare var Promise: PromiseConstructor;

interface IteratorYieldResult<TYield> { done?: false; value: TYield }
interface IteratorReturnResult<TReturn> { done: true; value: TReturn }
type IteratorResult<T, TReturn = any> = IteratorYieldResult<T> | IteratorReturnResult<TReturn>;
interface AsyncIterator<T, TReturn = any, TNext = undefined> {
    next(...args: [] | [TNext]): Promise<IteratorResult<T, TReturn>>;
}
interface AsyncGenerator<T = unknown, TReturn = any, TNext = unknown>
    extends AsyncIterator<T, TReturn, TNext> {}
"#;

/// `Promise.resolve<T>(value: T): Promise<Awaited<T>>` returns an Awaited
/// application. After tsz unwraps `Promise<...>` for async function bodies,
/// the inner `Awaited<X>` must be evaluated rather than left as a raw alias
/// application. Otherwise TS2322 messages render `Awaited<X>` instead of the
/// underlying structural form, mismatching tsc's `getAwaitedType`.
#[test]
fn async_return_promise_resolve_unfolds_awaited_in_ts2322_source_display() {
    // When the async-return assignability check fails, the source-type
    // display must not contain a raw `Awaited<` substring — the alias
    // must be evaluated to its conditional-type result.
    let source = format!(
        "{AWAITED_AND_ASYNC_GEN_PRELUDE}\n\
         async function* g(): AsyncGenerator<any, {{ x: \"x\" }}, any> {{\n\
           const r = {{ x: \"x\" }};\n\
           return Promise.resolve(r);\n\
         }}\n"
    );
    let diags = get_diagnostics(&source);
    let ts2322: Vec<_> = diags.iter().filter(|(code, _)| *code == 2322).collect();
    assert!(
        !ts2322.is_empty(),
        "expected a TS2322 for async-generator return-type mismatch, got: {diags:#?}"
    );
    for (_, msg) in &ts2322 {
        assert!(
            !msg.contains("Awaited<"),
            "async-return TS2322 source-type display must not contain raw `Awaited<` (the alias must be evaluated before display), got: {msg}"
        );
    }
}

/// Same invariant as above, but with `AsyncIterator` and a different
/// property/literal name choice — the unwrap path is shared, and the test
/// guarantees the fix is not specific to one type-alias spelling or to
/// one bound-variable name.
#[test]
fn async_iterator_return_promise_resolve_unfolds_awaited_in_ts2322_source_display() {
    let source = format!(
        "{AWAITED_AND_ASYNC_GEN_PRELUDE}\n\
         async function* h(): AsyncIterator<any, {{ y: \"y\" }}, any> {{\n\
           const s = {{ y: \"y\" }};\n\
           return Promise.resolve(s);\n\
         }}\n"
    );
    let diags = get_diagnostics(&source);
    let ts2322: Vec<_> = diags.iter().filter(|(code, _)| *code == 2322).collect();
    assert!(
        !ts2322.is_empty(),
        "expected a TS2322 for async-iterator return-type mismatch, got: {diags:#?}"
    );
    for (_, msg) in &ts2322 {
        assert!(
            !msg.contains("Awaited<"),
            "async-iterator return TS2322 source-type display must not contain raw `Awaited<`, got: {msg}"
        );
    }
}

#[test]
fn async_arrow_returning_promise_expression_preserves_inner() {
    // When an async function returns an already-Promise-wrapped value, the
    // outer Promise wrapping preserves the inner type (no double-wrap, no
    // spurious widening).
    let source = r#"
interface Promise<T> {}
interface Foo { bar: number }
declare function makePromise(): Promise<Foo>;
declare function h(p: () => string): void;
h(async () => makePromise());
"#;
    let diags = get_diagnostics(source);
    // Surfaces as TS2322 or TS2345 depending on which elaboration path wins;
    // the invariant under test is that the inner `Foo` survives unchanged
    // (not widened, not re-wrapped twice).
    let mismatch: Vec<_> = diags
        .iter()
        .filter(|(code, msg)| (*code == 2345 || *code == 2322) && msg.contains("Foo"))
        .collect();
    assert!(
        !mismatch.is_empty(),
        "expected a TS2322/TS2345 mentioning Foo for async arrow returning Promise<Foo>, got: {diags:#?}"
    );
    let msg = mismatch[0].1.as_str();
    assert!(
        msg.contains("<Foo>"),
        "async-return of Promise<Foo> must preserve inner `Foo`, got: {msg}"
    );
}
