//! Regression guard: TS2344 must be deferred — not eagerly emitted — for a
//! deferred higher-order-function (HOF) application that is indexed by
//! `[number]` and threaded through an `infer X extends unknown[]` argument
//! channel, the shape used pervasively by `hotscript` (`grow` canary row).
//!
//! Structural rule: when a type argument is a generic *application* that still
//! contains free type parameters (here, `this`-relative HOF lambda bodies such
//! as `Invoke<Each<...>, this["second"]>`) and the target parameter constraint
//! is an array/tuple type (`unknown[]`), `tsc` defers the constraint check to
//! instantiation time. The deferred application's base constraint is `unknown`,
//! which is not assignable to `unknown[]`, so eagerly checking it fabricates a
//! spurious `TS2344 … does not satisfy the constraint 'unknown[]'`. tsz must
//! defer it through the generic-application path in
//! `validate_type_args_against_params` (the application-with-type-parameters
//! branch), exactly as `tsc` does.
//!
//! The witness is `hotscript`'s `Tuples.EveryFn`:
//!
//! ```ts
//! interface EveryFn extends Fn {
//!   return: false extends Call<
//!     Tuples.Map<Extract<this["arg0"], Fn>>,
//!     this["arg1"]
//!   >[number]
//!     ? false
//!     : true;
//! }
//! ```
//!
//! `Call`/`Apply`/`PartialApply` all route their argument tuple through
//! `infer args extends unknown[]` and re-apply it via an `unknown[]`-constrained
//! parameter (`Apply<fn, args extends unknown[]>`). A single-file reduction of
//! any one helper evaluates eagerly and stays clean; the deferral only matters
//! once the full HOF graph keeps the application deferred. The two positive
//! cases below reproduce that graph with disjoint binder names (the deferral is
//! structural, never name-driven), and the negative controls prove the deferral
//! is not vacuous: a *concrete* non-array argument still earns its `TS2344`.
//!
//! Issue: tsz-org/tsz#13908.

use tsz_checker::test_utils::check_source_codes;

/// Minimal `--noLib` prelude: the global interfaces the structural-type machinery
/// references plus a local `Extract`, so the HOF graph below is self-contained.
const PRELUDE: &str = r#"
interface Array<T> { length: number; [n: number]: T; }
interface ReadonlyArray<T> { length: number; readonly [n: number]: T; }
interface Boolean {}
interface Function {}
interface CallableFunction {}
interface NewableFunction {}
interface IArguments {}
interface Number {}
interface Object {}
interface RegExp {}
interface String {}
interface Symbol {}
type Extract<T, U> = T extends U ? T : never;
"#;

/// hotscript-shaped deferred HOF graph (binder set "A"): `Lambda`/`Invoke`/
/// `ApplyAll`/`Curry`/`Each`/`All`. The `AllFn` body indexes a deferred
/// `Invoke<Each<...>, this["second"]>` by `[number]`, which is the exact
/// `EveryFn` witness. No `TS2344` may be emitted.
#[test]
fn test_deferred_hof_application_indexed_by_number_defers_unknown_array_constraint() {
    let source = format!(
        "{PRELUDE}{}",
        r#"
declare const rawArgs: unique symbol;
type rawArgs = typeof rawArgs;

interface Lambda {
  [rawArgs]: unknown;
  arguments: this[rawArgs] extends infer a extends unknown[] ? a : never;
  first: this[rawArgs] extends [infer x, ...any] ? x : never;
  second: this[rawArgs] extends [any, infer x, ...any] ? x : never;
}

declare const blank: unique symbol;
type blank = typeof blank;

type Drop<xs, out extends any[] = []> = xs extends [infer h, ...infer t]
  ? [h] extends [blank] ? Drop<t, out> : Drop<t, [...out, h]>
  : out;

type Invoke<lam extends Lambda, p0 = blank, p1 = blank> = (lam & {
  [rawArgs]: Drop<[p0, p1]>;
})["return"];

type ApplyAll<lam extends Lambda, items extends unknown[]> =
  (lam & { [rawArgs]: items })["return"];

interface Curry<lam extends Lambda, held extends unknown[]> extends Lambda {
  return: this["arguments"] extends infer joined extends unknown[]
    ? ApplyAll<lam, [...held, ...joined]>
    : never;
}

interface EachFn extends Lambda {
  return: this["arguments"] extends [infer lam extends Lambda, infer seq extends unknown[]]
    ? { [k in keyof seq]: Invoke<lam, seq[k]> }
    : never;
}
type Each<lam extends Lambda | blank = blank, seq extends readonly any[] | blank = blank> =
  Curry<EachFn, [lam, seq]>;

interface AllFn extends Lambda {
  return: false extends Invoke<Each<Extract<this["first"], Lambda>>, this["second"]>[number]
    ? false
    : true;
}
type All<lam extends Lambda, seq = blank> = Curry<AllFn, [lam, seq]>;

interface IsNum<T> extends Lambda { return: this["first"] extends T ? true : false; }
type Positive = Invoke<All<IsNum<number>>, [1, 2, 3]>;
"#
    );
    let diagnostics = check_source_codes(&source);
    assert!(
        !diagnostics.contains(&2344),
        "deferred HOF application must not emit TS2344; got: {diagnostics:?}"
    );
}

/// Same structural graph as above with a disjoint binder set ("B":
/// `Thunk`/`Run`/`Spread`/`Bind`/`OverEach`/`Whole`). The deferral is a
/// structural property of the application/constraint shapes, never of the
/// chosen identifiers, so this must also emit no `TS2344`.
#[test]
fn test_deferred_hof_application_renamed_binders_still_defers() {
    let source = format!(
        "{PRELUDE}{}",
        r#"
declare const packed: unique symbol;
type packed = typeof packed;

interface Thunk {
  [packed]: unknown;
  inputs: this[packed] extends infer a extends unknown[] ? a : never;
  head: this[packed] extends [infer x, ...any] ? x : never;
  next: this[packed] extends [any, infer x, ...any] ? x : never;
}

declare const hole: unique symbol;
type hole = typeof hole;

type Strip<xs, acc extends any[] = []> = xs extends [infer h, ...infer t]
  ? [h] extends [hole] ? Strip<t, acc> : Strip<t, [...acc, h]>
  : acc;

type Run<th extends Thunk, q0 = hole, q1 = hole> = (th & {
  [packed]: Strip<[q0, q1]>;
})["return"];

type Spread<th extends Thunk, cells extends unknown[]> =
  (th & { [packed]: cells })["return"];

interface Bind<th extends Thunk, saved extends unknown[]> extends Thunk {
  return: this["inputs"] extends infer merged extends unknown[]
    ? Spread<th, [...saved, ...merged]>
    : never;
}

interface OverEachFn extends Thunk {
  return: this["inputs"] extends [infer th extends Thunk, infer row extends unknown[]]
    ? { [k in keyof row]: Run<th, row[k]> }
    : never;
}
type OverEach<th extends Thunk | hole = hole, row extends readonly any[] | hole = hole> =
  Bind<OverEachFn, [th, row]>;

interface WholeFn extends Thunk {
  return: false extends Run<OverEach<Extract<this["head"], Thunk>>, this["next"]>[number]
    ? false
    : true;
}
type Whole<th extends Thunk, row = hole> = Bind<WholeFn, [th, row]>;

interface IsStr<T> extends Thunk { return: this["head"] extends T ? true : false; }
type Result = Run<Whole<IsStr<string>>, ["a", "b"]>;
"#
    );
    let diagnostics = check_source_codes(&source);
    assert!(
        !diagnostics.contains(&2344),
        "renamed-binder deferred HOF application must not emit TS2344; got: {diagnostics:?}"
    );
}

/// Negative control: a *concrete* non-array argument passed to an
/// `unknown[]`-constrained parameter must still earn its `TS2344`. This proves
/// the deferral above is structural (deferred-application-only), not a blanket
/// suppression of array-constraint checks.
#[test]
fn test_concrete_non_array_into_unknown_array_constraint_still_emits_ts2344() {
    let source = format!(
        "{PRELUDE}{}",
        r#"
type Vec<X extends unknown[]> = { items: X };
type Bad = Vec<string>;
"#
    );
    let diagnostics = check_source_codes(&source);
    assert!(
        diagnostics.contains(&2344),
        "concrete non-array argument must emit TS2344; got: {diagnostics:?}"
    );
}

/// Negative control: a concrete array argument satisfies `unknown[]` and must
/// not emit `TS2344` (guards against over-correcting into a false negative).
#[test]
fn test_concrete_array_into_unknown_array_constraint_is_accepted() {
    let source = format!(
        "{PRELUDE}{}",
        r#"
type Vec<X extends unknown[]> = { items: X };
type Ok = Vec<number[]>;
"#
    );
    let diagnostics = check_source_codes(&source);
    assert!(
        !diagnostics.contains(&2344),
        "concrete array argument must not emit TS2344; got: {diagnostics:?}"
    );
}
