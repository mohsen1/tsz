//! Tests for async function return type checking with nested Promise types.
//!
//! TypeScript's Promise flattening rule: `async (): Promise<Promise<T>>` has
//! the same awaited body return type as `async (): Promise<T>` — both accept
//! `T | PromiseLike<T>` as return expressions. This is because JavaScript
//! runtimes flatten Promises at the boundary, so nested Promises collapse.
//!
//! These tests verify that tsz correctly implements tsc's `getAwaitedType`
//! semantics when computing the body return type for annotated async functions.
//! The structural rule: the body return type for `async (): Promise<X>` is
//! `Awaited<X>`, not `X` verbatim — so `Promise<Promise<string>>` → `string`.

use tsz_checker::test_utils::check_source_code_messages as get_diagnostics;

/// Minimal inline lib stub for async-return tests — no external lib needed.
/// Provides enough Promise/Awaited shape for the unwrap path to engage.
const PROMISE_PRELUDE: &str = r#"
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
"#;

// ── Core issue repro (issue #6367) ───────────────────────────────────────────

#[test]
fn double_wrapped_promise_return_no_error() {
    // async function doubleWrap(): Promise<Promise<string>> {
    //   return Promise.resolve("nested");  // Promise<string> — tsc: OK, tsz: was TS2322
    // }
    let src = format!(
        "{PROMISE_PRELUDE}
async function doubleWrap(): Promise<Promise<string>> {{
  return Promise.resolve(\"nested\");
}}"
    );
    let diags = get_diagnostics(&src);
    assert!(
        diags.iter().all(|(c, _)| *c != 2322),
        "returning Promise<string> from async (): Promise<Promise<string>> must not produce TS2322; got: {diags:#?}"
    );
}

#[test]
fn double_wrapped_promise_plain_string_return_no_error() {
    // Returning the raw value (string) is also fine because Awaited<Promise<string>> = string.
    let src = format!(
        "{PROMISE_PRELUDE}
async function doubleWrap(): Promise<Promise<string>> {{
  return \"nested\";
}}"
    );
    let diags = get_diagnostics(&src);
    assert!(
        diags.iter().all(|(c, _)| *c != 2322),
        "returning string from async (): Promise<Promise<string>> must not produce TS2322; got: {diags:#?}"
    );
}

#[test]
fn double_wrapped_promise_wrong_type_still_errors() {
    // Returning number when string is expected must still fail.
    let src = format!(
        "{PROMISE_PRELUDE}
async function doubleWrap(): Promise<Promise<string>> {{
  return 42;
}}"
    );
    let diags = get_diagnostics(&src);
    assert!(
        diags.iter().any(|(c, _)| *c == 2322),
        "returning number from async (): Promise<Promise<string>> must produce TS2322; got: {diags:#?}"
    );
}

// ── Single-wrap still works (regression guard) ────────────────────────────────

#[test]
fn single_wrapped_promise_return_no_error() {
    let src = format!(
        "{PROMISE_PRELUDE}
async function singleWrap(): Promise<string> {{
  return Promise.resolve(\"hello\");
}}"
    );
    let diags = get_diagnostics(&src);
    assert!(
        diags.iter().all(|(c, _)| *c != 2322),
        "returning Promise<string> from async (): Promise<string> must not produce TS2322; got: {diags:#?}"
    );
}

#[test]
fn single_wrapped_promise_wrong_type_still_errors() {
    let src = format!(
        "{PROMISE_PRELUDE}
async function singleWrap(): Promise<string> {{
  return 42;
}}"
    );
    let diags = get_diagnostics(&src);
    assert!(
        diags.iter().any(|(c, _)| *c == 2322),
        "returning number from async (): Promise<string> must produce TS2322; got: {diags:#?}"
    );
}

// ── Name-independence: different type-parameter / alias spellings ─────────────

#[test]
fn double_wrapped_promise_with_type_alias_no_error() {
    // The fix must not depend on the name of the inner type.
    let src = format!(
        "{PROMISE_PRELUDE}
type Payload = {{ data: number }};
async function fetchPayload(): Promise<Promise<Payload>> {{
  return Promise.resolve({{ data: 42 }});
}}"
    );
    let diags = get_diagnostics(&src);
    assert!(
        diags.iter().all(|(c, _)| *c != 2322),
        "returning Promise<Payload> from async (): Promise<Promise<Payload>> must not error; got: {diags:#?}"
    );
}

#[test]
fn double_wrapped_promise_number_inner_type_no_error() {
    // Prove the fix is not string-specific; works for any inner primitive.
    let src = format!(
        "{PROMISE_PRELUDE}
async function countWrapper(): Promise<Promise<number>> {{
  return Promise.resolve(99);
}}"
    );
    let diags = get_diagnostics(&src);
    assert!(
        diags.iter().all(|(c, _)| *c != 2322),
        "returning Promise<number> from async (): Promise<Promise<number>> must not error; got: {diags:#?}"
    );
}

// ── Triple nesting ────────────────────────────────────────────────────────────

#[test]
fn triple_wrapped_promise_no_error() {
    // Promise<Promise<Promise<string>>> should also collapse to string.
    let src = format!(
        "{PROMISE_PRELUDE}
async function tripleWrap(): Promise<Promise<Promise<string>>> {{
  return Promise.resolve(\"deep\");
}}"
    );
    let diags = get_diagnostics(&src);
    assert!(
        diags.iter().all(|(c, _)| *c != 2322),
        "returning Promise<string> from async (): Promise<Promise<Promise<string>>> must not error; got: {diags:#?}"
    );
}

// ── Generic type parameters ───────────────────────────────────────────────────
// Note: `async function wrap<T>(val: T): Promise<Promise<T>> { return Promise.resolve(val); }`
// has additional complexity when the Promise type is user-defined (not from stdlib): the
// TS1064 check fires because `symbol_has_standard_lib_origin` returns false for inline
// Promise declarations, and type-parameter identity across instantiation boundaries
// creates a separate T vs T mismatch. Both are pre-existing issues unrelated to
// the nested-Promise body-return-type fix. Tracked separately from issue #6367.
