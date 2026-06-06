//! Regression tests for recursive conditional / `infer` evaluation over generic
//! applications (issue #11586 — "solver memoization misses recursive cycle keys
//! for alias-heavy utilities").
//!
//! A recursive generic wrapper that unwraps its argument through an `infer`
//! pattern — `type Unbox<T> = T extends Box<infer U> ? Unbox<U> : T`, or the
//! standard-library `Awaited<T>` — re-enters conditional/`infer` evaluation
//! through *fresh* `TypeEvaluator` / `SubtypeChecker` instances at every level.
//! Each fresh instance resets its per-instance cycle, depth, and iteration
//! guards, and the recursion bounces between types whose identity keeps changing
//! (fresh `infer` placeholders / object shapes), so no per-instance guard ever
//! fires. With a literal/object argument this previously ran unbounded and hung
//! the compile; the cross-instance per-query operation budget in
//! `TypeEvaluator::evaluate` now bounds it.
//!
//! These tests pin two things:
//! 1. The common, correctly-terminating shapes still type-check exactly as
//!    `tsc` does (the budget must not perturb finite recursion). They resolve in
//!    far fewer operations than the budget, so the guard is inert for them.
//! 2. The recursive utilities accept renamed binders and varied argument shapes,
//!    so the behaviour follows structure, not spelling.

use crate::test_utils::{check_source_diagnostics, diagnostics_with_code};

fn error_count(source: &str, code: u32) -> usize {
    diagnostics_with_code(&check_source_diagnostics(source), code).len()
}

fn total_errors(source: &str) -> usize {
    check_source_diagnostics(source).len()
}

/// A self-contained `Awaited`-shaped recursive unwrapper (modelling the
/// standard-library `Awaited<T> = T extends PromiseLike<infer U> ? Awaited<U> :
/// T`) over a widened argument chain unwraps to the inner type. Assigning the
/// inner type is fine; assigning an unrelated type is `TS2322`. Uses a
/// user-defined `Thenable` so the test does not depend on the ambient lib build.
#[test]
fn awaited_shaped_chain_widened_unwraps_cleanly() {
    let src = r#"
interface Thenable<T> { then(onfulfilled: (value: T) => void): void; }
type Resolve<T> = T extends Thenable<infer U> ? Resolve<U> : T;
type R = Resolve<Thenable<Thenable<number>>>;
const ok: R = 5;
const bad: R = "no";
"#;
    assert_eq!(
        error_count(src, 2322),
        1,
        "only the string assignment errors"
    );
    assert_eq!(total_errors(src), 1);
}

/// A user-defined recursive unwrapper over an interface with a widened argument
/// resolves to the inner type and does not hang. Renamed binders (`Cell`/`Inner`)
/// exercise the structural, name-agnostic path.
#[test]
fn user_recursive_unwrapper_widened_arg() {
    let src = r#"
interface Cell<Inner> { value: Inner; }
type Open<Wrapped> = Wrapped extends Cell<infer Held> ? Open<Held> : Wrapped;
type R = Open<Cell<Cell<number>>>;
const ok: R = 7;
const bad: R = "no";
"#;
    assert_eq!(error_count(src, 2322), 1);
    assert_eq!(total_errors(src), 1);
}

// NOTE: the previously-hanging shapes (a recursive unwrapper / nested
// `infer`-conditional applied to a *literal* argument, e.g. `Unbox<Box<2>>` or
// `Awaited<Promise<2>>`) are exercised end-to-end by the CLI integration test
// `cross_instance_recursion_terminates` in `crates/tsz-cli/tests`, which runs
// the compiler in a subprocess with a small `TSZ_MAX_EVAL_OPS` budget and a wall
// -clock timeout. They are kept out of the in-process unit tests because at the
// production budget they intentionally spin up to two million operations before
// bailing — fast in release, but too slow for a debug unit test — and the
// per-query budget override is process-global. The `EvalQueryFrame` budget
// mechanism itself is unit-tested in `crates/tsz-solver`.

/// Tail-recursion-eliminated recursive utilities (tuple builders / reversers)
/// must keep working — the per-query budget is far above their cost and the
/// guard never fires for them.
#[test]
fn tail_recursive_tuple_utilities_unaffected() {
    let src = r#"
type Reverse<T extends readonly any[]> =
  T extends readonly [infer H, ...infer R] ? [...Reverse<R>, H] : [];
type R = Reverse<[1, 2, 3, 4, 5]>;
const r: R = [5, 4, 3, 2, 1];

type Counter<N extends number, A extends any[] = []> =
  A['length'] extends N ? A : Counter<N, [...A, 1]>;
type C = Counter<20>['length'];
const c: C = 20;
"#;
    assert_eq!(total_errors(src), 0);
}

/// Structural recursive mapped utilities (deep transforms) must keep working.
#[test]
fn deep_recursive_mapped_utility_unaffected() {
    let src = r#"
type DeepReadonly<T> = T extends object ? { readonly [K in keyof T]: DeepReadonly<T[K]> } : T;
type D = DeepReadonly<{ a: { b: { c: number } } }>;
declare const d: D;
const n: number = d.a.b.c;
"#;
    assert_eq!(total_errors(src), 0);
}
