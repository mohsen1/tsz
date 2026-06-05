//! Regression tests for issue #11337: long generic operator chains such as
//! `of(1).pipe(map(...), filter(...), scan(...), take(...))`.
//!
//! Overload resolution for these chains used to instantiate every parameter of
//! the matched overload once per argument, making the per-argument loop
//! `O(args * params)` — quadratic for a `pipe(op1, ..., opN)` overload that
//! carries one parameter per stage. The shared
//! `instantiated_contextual_param_type_at` helper now instantiates only the
//! parameter that actually supplies each argument's contextual type, so the
//! loop is linear. These tests pin the *behavioral* contract that must survive
//! that refactor: each stage's callback parameter is still typed under the
//! correct contextual type, and the chain's result type is preserved.

use crate::test_utils::check_source_code_messages;

/// A small, self-contained RxJS-shaped prelude. `pipe` is overloaded with one
/// `OperatorFunction` parameter per stage (the exact shape that produced the
/// quadratic instantiation), and the operator factories are generic so each
/// stage's inline callback requires contextual typing from the previous stage.
const RXJS_PRELUDE: &str = r#"
interface Observable<T> {
    pipe<A>(op1: OperatorFunction<T, A>): Observable<A>;
    pipe<A, B>(op1: OperatorFunction<T, A>, op2: OperatorFunction<A, B>): Observable<B>;
    pipe<A, B, C>(
        op1: OperatorFunction<T, A>,
        op2: OperatorFunction<A, B>,
        op3: OperatorFunction<B, C>,
    ): Observable<C>;
    pipe<A, B, C, D>(
        op1: OperatorFunction<T, A>,
        op2: OperatorFunction<A, B>,
        op3: OperatorFunction<B, C>,
        op4: OperatorFunction<C, D>,
    ): Observable<D>;
    pipe<A, B, C, D, E>(
        op1: OperatorFunction<T, A>,
        op2: OperatorFunction<A, B>,
        op3: OperatorFunction<B, C>,
        op4: OperatorFunction<C, D>,
        op5: OperatorFunction<D, E>,
    ): Observable<E>;
}
type OperatorFunction<T, R> = (source: Observable<T>) => Observable<R>;
declare function of<T>(value: T): Observable<T>;
declare function map<T, R>(project: (value: T) => R): OperatorFunction<T, R>;
declare function filter<T>(predicate: (value: T) => boolean): OperatorFunction<T, T>;
declare function scan<T, R>(accumulator: (acc: R, value: T) => R, seed: R): OperatorFunction<T, R>;
declare function take<T>(count: number): OperatorFunction<T, T>;
"#;

fn check(body: &str) -> Vec<(u32, String)> {
    check_source_code_messages(&format!("{RXJS_PRELUDE}\n{body}"))
}

fn codes(messages: &[(u32, String)]) -> Vec<u32> {
    messages.iter().map(|(code, _)| *code).collect()
}

#[test]
fn long_operator_chain_resolves_without_diagnostics() {
    // The canonical issue #11337 repro. A four-stage chain with a contextual
    // result annotation (which drives the return-context-substitution path that
    // owns the per-argument instantiation loop) must type-check cleanly.
    let messages = check(
        r#"
const result: Observable<number> = of(1).pipe(
    map((x) => x + 1),
    filter((x) => x > 0),
    scan((a, b) => a + b, 0),
    take(10),
);
"#,
    );
    assert!(
        messages.is_empty(),
        "expected clean long operator chain, got {messages:?}",
    );
}

#[test]
fn each_stage_callback_param_is_typed_from_previous_stage() {
    // `map((x) => ...)` receives `x: number` from `of(1)`. If contextual typing
    // of the per-stage callback parameter regressed to `any`, the `.length`
    // access below would be silently accepted. Requiring TS2339 here proves the
    // optimized helper still threads the correct contextual parameter type.
    let messages = check(
        r#"
const bad: Observable<number> = of(1).pipe(
    map((x) => x.length),
);
"#,
    );
    assert!(
        codes(&messages).contains(&2339),
        "expected TS2339 for `.length` on a number stage param, got {messages:?}",
    );
}

#[test]
fn five_stage_operator_chain_resolves_without_diagnostics() {
    // Breadth guard: a five-stage chain exercises the `pipe<A, B, C, D, E>`
    // overload (five parameters), which is where the quadratic instantiation was
    // most pronounced. The whole chain — including the alternating
    // number/string stage transitions — must still type-check cleanly under the
    // linear per-argument instantiation.
    let messages = check(
        r#"
const result: Observable<number> = of(1).pipe(
    map((x) => x + 1),
    map((x) => `${x}`),
    filter((s) => s.length > 0),
    map((s) => s.length),
    scan((a, b) => a + b, 0),
);
"#,
    );
    assert!(
        messages.is_empty(),
        "expected clean five-stage operator chain, got {messages:?}",
    );
}

#[test]
fn stage_type_transformation_flows_through_chain() {
    // `map((x: number) => `${x}`)` turns the stream into `Observable<string>`,
    // so the following `filter` callback parameter must be `string`. Calling a
    // string method there must succeed, while a numeric method must fail —
    // confirming the per-stage contextual type advances across stages.
    let ok = check(
        r#"
const out: Observable<string> = of(1).pipe(
    map((x) => `${x}`),
    filter((s) => s.length > 0),
);
"#,
    );
    assert!(
        ok.is_empty(),
        "expected string-stage chain to type-check, got {ok:?}",
    );

    let bad = check(
        r#"
const out: Observable<string> = of(1).pipe(
    map((x) => `${x}`),
    filter((s) => s.toFixed(2) !== ""),
);
"#,
    );
    assert!(
        codes(&bad).contains(&2339),
        "expected TS2339 for `.toFixed` on a string stage param, got {bad:?}",
    );
}
