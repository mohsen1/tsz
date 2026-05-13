//! Regression tests for issue #6336: stack overflow when multiple generic function calls
//! with mapped tuple return types have their types queried via `typeof`.

use crate::test_utils::check_source_code_messages;

const PROMISE_STUB: &str = r#"
interface Promise<T> {
    then<TResult1 = T, TResult2 = never>(
        onfulfilled?: ((value: T) => TResult1 | Promise<TResult1>) | null,
        onrejected?: ((reason: unknown) => TResult2 | Promise<TResult2>) | null,
    ): Promise<TResult1 | TResult2>;
}
interface PromiseConstructor {
    resolve<T>(value: T | Promise<T>): Promise<T>;
}
declare var Promise: PromiseConstructor;
"#;

fn src(body: &str) -> String {
    format!(
        r#"{PROMISE_STUB}
type Awaited<T> = T extends Promise<infer U> ? Awaited<U> : T;

type AwaitedTuple<T extends readonly unknown[]> = {{
  [K in keyof T]: Awaited<T[K]>
}};

declare function PromiseAll<T extends readonly unknown[]>(
  values: T
): Promise<AwaitedTuple<T>>;

{body}"#
    )
}

fn assert_no_errors(source: &str) {
    let diags = check_source_code_messages(source);
    assert!(diags.is_empty(), "expected no errors, got: {diags:?}");
}

#[test]
fn calls_without_typeof_work() {
    assert_no_errors(&src(r#"const test1 = PromiseAll([1, 2, 3] as const);
const test2 = PromiseAll([1, 2, Promise.resolve(3)] as const);
export {};
"#));
}

#[test]
fn first_typeof_only_works() {
    assert_no_errors(&src(r#"const test1 = PromiseAll([1, 2, 3] as const);
type T1 = typeof test1;
export {};
"#));
}

#[test]
fn second_typeof_only_works() {
    assert_no_errors(&src(
        r#"const test2 = PromiseAll([1, 2, Promise.resolve(3)] as const);
type T2 = typeof test2;
export {};
"#,
    ));
}

#[test]
fn two_typeof_queries_on_mapped_tuple_results_do_not_crash() {
    assert_no_errors(&src(r#"const test1 = PromiseAll([1, 2, 3] as const);
type T1 = typeof test1;

const test2 = PromiseAll([1, 2, Promise.resolve(3)] as const);
type T2 = typeof test2;

export {};
"#));
}

// Uses different type-parameter names to confirm the rule is structural.
#[test]
fn two_typeof_queries_with_renamed_type_params_do_not_crash() {
    let source = format!(
        r#"{PROMISE_STUB}
type Resolved<V> = V extends Promise<infer R> ? Resolved<R> : V;

type ResolvedTuple<Items extends readonly unknown[]> = {{
  [I in keyof Items]: Resolved<Items[I]>
}};

declare function resolveAll<Items extends readonly unknown[]>(
  values: Items
): Promise<ResolvedTuple<Items>>;

const a = resolveAll([true, false] as const);
type A = typeof a;

const b = resolveAll([true, Promise.resolve("x")] as const);
type B = typeof b;

export {{}};
"#
    );
    assert_no_errors(&source);
}

#[test]
fn three_typeof_queries_on_mapped_tuple_results_do_not_crash() {
    assert_no_errors(&src(r#"const r1 = PromiseAll([1, 2, 3] as const);
type R1 = typeof r1;

const r2 = PromiseAll([1, 2, Promise.resolve(3)] as const);
type R2 = typeof r2;

const r3 = PromiseAll([Promise.resolve(1), Promise.resolve(2)] as const);
type R3 = typeof r3;

export {};
"#));
}
