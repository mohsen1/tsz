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

/// An `Awaited`-shaped distributive conditional over a union whose members still
/// carry a free type parameter must reduce each member, not leave a raw
/// conditional. Modelling the zod `_parseAsync` witness (`Promise.resolve` over
/// `SyncParseReturnType<Output> | AsyncParseReturnType<Output>`): the non-thenable
/// member `Wrapped<Output>` has no `then` property, so the per-member relation
/// `Wrapped<Output> extends Thenable<infer U>` fails even under the permissive
/// (`Output := any`) instantiation. tsc reduces that member to itself; tsz must
/// take the false branch too instead of deferring the whole conditional, so the
/// distributed `Resolve<…>` is assignable back to `Wrapped<Output>`.
///
/// Before the permissive-false-branch fall-through, the generic check
/// `Wrapped<Output>` (an `Application` with no narrower constraint than itself)
/// short-circuited to a deferred conditional, leaving `Resolve<Wrapped<Output>>`
/// raw and emitting a spurious `TS2322` on the positive assignment.
#[test]
fn awaited_shaped_distribution_reduces_free_param_union_member() {
    let src = r#"
interface Thenable<T> { then(onfulfilled: (value: T) => void): void; }
type Resolve<T> = T extends Thenable<infer U> ? Resolve<U> : T;
type Wrapped<P> = { tag: "wrapped"; payload: P };
function f<Output>(): void {
  type In = Wrapped<Output> | Thenable<Wrapped<Output>>;
  const resolved = null as any as Resolve<In>;
  // Resolve<In> distributes: Wrapped<Output> (no `then`) reduces to itself and
  // Thenable<Wrapped<Output>> unwraps to Wrapped<Output>, so the whole thing is
  // Wrapped<Output> and this assignment is fine.
  const ok: Wrapped<Output> = resolved;
  // An unrelated shape is still rejected (TS2322), proving the reduction did not
  // collapse to `any`/`unknown`.
  const bad: { unrelated_marker: 1 } = resolved;
}
"#;
    // Only the deliberately-unrelated `bad` assignment errors; the positive
    // `ok` assignment must NOT (no spurious raw-conditional TS2322). tsc
    // drills the single-missing-property failure to TS2741, so accept either
    // the drilled form or the plain TS2322.
    assert_eq!(
        error_count(src, 2322) + error_count(src, 2741),
        1,
        "only the unrelated `bad` assignment errors; got: {:?}",
        check_source_diagnostics(src)
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
    assert_eq!(total_errors(src), 1);
}

/// Renamed binders and a different non-thenable carrier exercise the same
/// structural path (the reduction follows shape, not spelling).
#[test]
fn awaited_shaped_distribution_reduces_free_param_union_member_renamed() {
    let src = r#"
interface Awaitable<Value> { then(cb: (value: Value) => void): void; }
type Settle<Input> = Input extends Awaitable<infer Held> ? Settle<Held> : Input;
type Cell<Slot> = { kind: 0; slot: Slot };
function g<Elem>(): void {
  type Mixed = Cell<Elem> | Awaitable<Cell<Elem>>;
  const settled = null as any as Settle<Mixed>;
  const ok: Cell<Elem> = settled;
  const bad: { mismatch: true } = settled;
}
"#;
    // tsc drills the single-missing-property failure to TS2741; accept either
    // the drilled form or the plain TS2322.
    assert_eq!(error_count(src, 2322) + error_count(src, 2741), 1);
    assert_eq!(total_errors(src), 1);
}
