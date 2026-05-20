//! Tests for recursive conditional type with rest-parameter infer patterns.
//!
//! When a function type `(...args: [A, B]) => R` is pattern-matched against
//! `(first: infer F, ...rest: infer Rest) => infer Ret`, the single rest
//! parameter whose type is a concrete tuple must be expanded to individual
//! params before matching, so `F = A` and `Rest = [B]` — not `F = [A, B]`.
//!
//! Structural rule: a rest parameter with a concrete tuple type
//! `(...args: [A, B, ...])` is structurally equivalent to `(a: A, b: B, ...)`
//! for infer pattern matching.  This is required for recursive Curry-style
//! types, variadic pipe utilities, and any pattern that peels one argument at
//! a time via `(head: infer H, ...tail: infer T)`.
//!
//! Fixes #7003.

use tsz_checker::test_utils::check_source_codes;

// ── 1. Direct repro from the issue ──────────────────────────────────────────

#[test]
fn curry_three_arg_no_ts2345() {
    let codes = check_source_codes(
        r#"
type Curry<F extends (...args: any) => any> = F extends (
  first: infer A,
  ...rest: infer Rest
) => infer R
  ? Rest extends []
    ? (a: A) => R
    : (a: A) => Curry<(...args: Rest) => R>
  : never;

declare function curry<F extends (...args: any) => any>(fn: F): Curry<F>;

const add = (a: number, b: number, c: number) => a + b + c;
const curriedAdd = curry(add);

const step1 = curriedAdd(1);
const step2 = step1(2);
const result = step2(3);
"#,
    );
    assert!(
        !codes.contains(&2345) && !codes.contains(&2349),
        "expected no TS2345/TS2349 for valid curried calls, got: {codes:?}"
    );
}

// ── 2. Two-argument curry ────────────────────────────────────────────────────

#[test]
fn curry_two_arg_no_ts2345() {
    let codes = check_source_codes(
        r#"
type Curry<F extends (...args: any) => any> = F extends (
  first: infer A,
  ...rest: infer Rest
) => infer R
  ? Rest extends []
    ? (a: A) => R
    : (a: A) => Curry<(...args: Rest) => R>
  : never;

declare function curry<F extends (...args: any) => any>(fn: F): Curry<F>;

const multiply = (x: number, y: number) => x * y;
const curriedMultiply = curry(multiply);

const double = curriedMultiply(2);
const result = double(5);
"#,
    );
    assert!(
        !codes.contains(&2345) && !codes.contains(&2349),
        "expected no TS2345/TS2349 for two-arg curried call, got: {codes:?}"
    );
}

// ── 3. Different type-parameter names prove the fix is structural ─────────────

#[test]
fn curry_renamed_type_params_no_ts2345() {
    let codes = check_source_codes(
        r#"
type Pipe<Fn extends (...xs: any) => any> = Fn extends (
  head: infer H,
  ...tail: infer T
) => infer Out
  ? T extends []
    ? (h: H) => Out
    : (h: H) => Pipe<(...xs: T) => Out>
  : never;

declare function pipe<Fn extends (...xs: any) => any>(fn: Fn): Pipe<Fn>;

const f = (a: string, b: number, c: boolean) => `${a}${b}${c}`;
const piped = pipe(f);

const step1 = piped("hello");
const step2 = step1(42);
const result = step2(true);
"#,
    );
    assert!(
        !codes.contains(&2345) && !codes.contains(&2349),
        "expected no TS2345/TS2349 for renamed-param curry, got: {codes:?}"
    );
}

// ── 4. Single-argument function → direct return, no recursion ────────────────

#[test]
fn curry_single_arg_no_ts2345() {
    let codes = check_source_codes(
        r#"
type Curry<F extends (...args: any) => any> = F extends (
  first: infer A,
  ...rest: infer Rest
) => infer R
  ? Rest extends []
    ? (a: A) => R
    : (a: A) => Curry<(...args: Rest) => R>
  : never;

declare function curry<F extends (...args: any) => any>(fn: F): Curry<F>;

const identity = (x: number) => x;
const curried = curry(identity);
const result = curried(42);
"#,
    );
    assert!(
        !codes.contains(&2345) && !codes.contains(&2349),
        "expected no TS2345/TS2349 for single-arg curried call, got: {codes:?}"
    );
}

// ── 5. Wrong argument type must still produce TS2345 ────────────────────────

#[test]
fn curry_wrong_arg_type_produces_ts2345() {
    let codes = check_source_codes(
        r#"
type Curry<F extends (...args: any) => any> = F extends (
  first: infer A,
  ...rest: infer Rest
) => infer R
  ? Rest extends []
    ? (a: A) => R
    : (a: A) => Curry<(...args: Rest) => R>
  : never;

declare function curry<F extends (...args: any) => any>(fn: F): Curry<F>;

const add = (a: number, b: number) => a + b;
const curriedAdd = curry(add);

const step1 = curriedAdd("wrong");
"#,
    );
    assert!(
        codes.contains(&2345),
        "expected TS2345 when passing string to number-expecting curried function, got: {codes:?}"
    );
}

// ── 6. Heterogeneous argument types (number + string) ───────────────────────

#[test]
fn curry_heterogeneous_arg_types_no_ts2345() {
    let codes = check_source_codes(
        r#"
type Curry<F extends (...args: any) => any> = F extends (
  first: infer A,
  ...rest: infer Rest
) => infer R
  ? Rest extends []
    ? (a: A) => R
    : (a: A) => Curry<(...args: Rest) => R>
  : never;

declare function curry<F extends (...args: any) => any>(fn: F): Curry<F>;

const greet = (times: number, name: string) => name.repeat(times);
const curriedGreet = curry(greet);

const repeatThrice = curriedGreet(3);
const result = repeatThrice("hi");
"#,
    );
    assert!(
        !codes.contains(&2345) && !codes.contains(&2349),
        "expected no TS2345/TS2349 for heterogeneous-arg curry, got: {codes:?}"
    );
}
