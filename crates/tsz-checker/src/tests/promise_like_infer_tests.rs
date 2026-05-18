//! Regression tests for `T extends PromiseLike<infer U>` pattern matching.
//!
//! When `T = Promise<string>`, TypeScript infers `U = string` because
//! `Promise<T> <: PromiseLike<T>` (one-directional subtype). The fix extends
//! Application-level infer matching to allow one-directional subtyping
//! (source_base <: pattern_base, same arity), rather than requiring mutual
//! subtyping (isomorphism). Positional type-argument correspondence is correct
//! for covariant interface hierarchies.
//!
//! Adjacent cases:
//! 1. `Promise<string> extends PromiseLike<infer U>` → U = string
//! 2. `Promise<number> extends PromiseLike<infer U>` → U = number (renamed type arg)
//! 3. `Promise<Promise<string>> extends PromiseLike<infer U>` → U = Promise<string>
//! 4. `MyAwaited<T> = T extends PromiseLike<infer U> ? MyAwaited<U> : T` with Promise<string> → string
//! 5. User-defined subtype: `MyPromise<T> <: PromiseLike<T>` → same inference

use crate::test_utils::check_source_diagnostics;

/// Inline PromiseLike/Promise definitions for self-contained tests.
fn with_promise_defs(body: &str) -> String {
    format!(
        r#"
interface PromiseLike<T> {{
    then<TResult1 = T, TResult2 = never>(
        onfulfilled?: ((value: T) => TResult1 | PromiseLike<TResult1>) | null,
        onrejected?: ((reason: any) => TResult2 | PromiseLike<TResult2>) | null
    ): PromiseLike<TResult1 | TResult2>;
}}

interface Promise<T> {{
    then<TResult1 = T, TResult2 = never>(
        onfulfilled?: ((value: T) => TResult1 | PromiseLike<TResult1>) | null,
        onrejected?: ((reason: any) => TResult2 | PromiseLike<TResult2>) | null
    ): Promise<TResult1 | TResult2>;
    catch<TResult = never>(
        onrejected?: ((reason: any) => TResult | PromiseLike<TResult>) | null
    ): Promise<T | TResult>;
}}

{body}
"#
    )
}

fn assert_no_ts2322(source: &str, context: &str) {
    let diags = check_source_diagnostics(source);
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&2322),
        "Expected no TS2322 for {context}. Got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

/// `Promise<string>` should match `PromiseLike<infer U>` → U = string.
/// The inferred MyAwaited<Promise<string>> = string, so assigning "hello" is valid.
#[test]
fn promise_string_extends_promiselike_infer_u_no_error() {
    let source = with_promise_defs(
        r#"
type MyAwaited<T> = T extends PromiseLike<infer U> ? MyAwaited<U> : T;
type A1 = MyAwaited<Promise<string>>;
declare let a1: A1;
const _ok: string = a1;
"#,
    );
    assert_no_ts2322(&source, "MyAwaited<Promise<string>> = string assignment");
}

/// With `Promise<number>`, U should be number (same rule, different type arg name).
#[test]
fn promise_number_extends_promiselike_infer_u_no_error() {
    let source = with_promise_defs(
        r#"
type MyAwaited<T> = T extends PromiseLike<infer U> ? MyAwaited<U> : T;
type A2 = MyAwaited<Promise<number>>;
declare let a2: A2;
const _ok: number = a2;
"#,
    );
    assert_no_ts2322(&source, "MyAwaited<Promise<number>> = number");
}

/// `Promise<Promise<string>>` → first unwrap gives Promise<string>, second gives string.
#[test]
fn promise_nested_extends_promiselike_infer_u_unwraps_correctly() {
    let source = with_promise_defs(
        r#"
type MyAwaited<T> = T extends PromiseLike<infer U> ? MyAwaited<U> : T;
type A3 = MyAwaited<Promise<Promise<string>>>;
declare let a3: A3;
const _ok: string = a3;
"#,
    );
    assert_no_ts2322(&source, "MyAwaited<Promise<Promise<string>>> = string");
}

/// Direct `Promise<string> extends PromiseLike<infer U>` in a conditional — non-recursive.
#[test]
fn direct_promise_extends_promiselike_infer_u_no_error() {
    let source = with_promise_defs(
        r#"
type Extract<T> = T extends PromiseLike<infer U> ? U : never;
type R = Extract<Promise<string>>;
declare let r: R;
const _ok: string = r;
"#,
    );
    assert_no_ts2322(&source, "Extract<Promise<string>> = string");
}

/// User-defined type implementing PromiseLike should also be matched.
/// `MyPromise<T>` is a subtype of `PromiseLike<T>` via structural compatibility.
#[test]
fn user_defined_promiselike_subtype_infer_u_no_error() {
    let source = with_promise_defs(
        r#"
interface MyPromise<T> {
    then<TResult1 = T, TResult2 = never>(
        onfulfilled?: ((value: T) => TResult1 | PromiseLike<TResult1>) | null,
        onrejected?: ((reason: any) => TResult2 | PromiseLike<TResult2>) | null
    ): MyPromise<TResult1 | TResult2>;
    myExtra(): void;
}

type Extract<T> = T extends PromiseLike<infer U> ? U : never;
type R = Extract<MyPromise<string>>;
declare let r: R;
const _ok: string = r;
"#,
    );
    assert_no_ts2322(&source, "user-defined PromiseLike subtype");
}

/// A wrong assignment should still produce TS2322 (type safety preserved).
#[test]
fn promise_awaited_wrong_type_still_errors() {
    let source = with_promise_defs(
        r#"
type MyAwaited<T> = T extends PromiseLike<infer U> ? MyAwaited<U> : T;
type A1 = MyAwaited<Promise<string>>;
declare let a1: A1;
const _bad: number = a1;
"#,
    );
    let codes: Vec<u32> = check_source_diagnostics(&source)
        .iter()
        .map(|d| d.code)
        .collect();
    assert!(
        codes.contains(&2322),
        "Expected TS2322 for assigning string to number. Got codes: {codes:?}"
    );
}

/// Non-PromiseLike types should take the false branch (U not inferred).
#[test]
fn non_promiselike_takes_false_branch() {
    let source = with_promise_defs(
        r#"
type Extract<T> = T extends PromiseLike<infer U> ? U : "not-a-promise";
type R = Extract<string>;
declare let r: R;
const _ok: "not-a-promise" = r;
"#,
    );
    assert_no_ts2322(&source, "non-PromiseLike type taking false branch");
}
