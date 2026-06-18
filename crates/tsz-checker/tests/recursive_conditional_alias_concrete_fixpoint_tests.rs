//! Regression tests for issue #13508 (root cause B): a recursive
//! conditional-type alias instantiated with *fully concrete* arguments must
//! converge to a single shared fixpoint rather than being re-evaluated per
//! sibling use site.
//!
//! Structural rule: when `Application(DefId, [args])` is evaluated and the
//! arguments carry no free type parameters, and the evaluation produces a
//! fully-resolved result (no free type parameter and no `error` sentinel),
//! that result is a complete, ambient-stack-independent function of
//! `(DefId, args, no_unchecked)` — the per-`(symbol, type-arg tuple)`
//! instantiation fixpoint `tsc` shares via its `resolvingType` memo. tsz now
//! shares it through the cross-evaluator `application_eval_cache` even when the
//! per-application `limit_epoch` gate (which guards against persisting a
//! depth-*truncated* result) would otherwise withhold the write. A truncated
//! result always carries an `error` sentinel, so the `error`-free predicate is
//! what separates a genuine fixpoint from a stack-context artifact.
//!
//! These assert correctness and TS2589-absence (the recursion converges); the
//! CPU-bound non-termination collapse on the `typebox` / `remeda` canary rows
//! is owned by the ready-review `project-compile-guard` (per repo policy the
//! broad project suites are not run locally).
//!
//! Adjacent cases: typebox `Static<…>`-shaped distributive conditional over a
//! concrete schema, the same schema reached through alias indirection and at
//! many sibling sites (fan-out sharing), a remeda `FilteredArray<…>`-shaped
//! distributive conditional, deep nesting, the generic (non-concrete) form
//! left untouched, and a renamed-binder variant so the path is structural and
//! not identifier-driven.

use tsz_checker::test_utils::check_source_codes;

/// typebox `Static<TSchema>`-shaped recursive distributive conditional applied
/// to a fully concrete schema. The result is a concrete object type, so it is
/// a sharable fixpoint; the assignment to its hand-written expansion must hold
/// with no TS2589.
#[test]
fn typebox_static_concrete_schema_converges_to_expansion() {
    let codes = check_source_codes(
        r#"
type Static<T> =
  T extends { kind: "object"; props: infer P } ? { [K in keyof P]: Static<P[K]> } :
  T extends { kind: "array"; items: infer I } ? Static<I>[] :
  T extends { kind: "string" } ? string :
  T extends { kind: "number" } ? number :
  unknown;
type Schema = {
  kind: "object";
  props: {
    a: { kind: "string" };
    b: { kind: "array"; items: { kind: "number" } };
    c: { kind: "object"; props: { d: { kind: "string" } } };
  };
};
type R = Static<Schema>;
declare const r: R;
const ok: { a: string; b: number[]; c: { d: string } } = r;
"#,
    );
    assert!(
        !codes.contains(&2589),
        "concrete Static<Schema> must converge (no TS2589). Got: {codes:?}"
    );
    assert!(
        !codes.contains(&2322),
        "Static<Schema> must equal its hand-written expansion. Got: {codes:?}"
    );
}

/// Fan-out sharing: the same concrete `Static<Schema>` reached at several
/// sibling sites — directly and through alias indirection — must resolve to
/// one shared fixpoint and remain mutually assignable.
#[test]
fn typebox_static_concrete_fanout_shares_fixpoint() {
    let codes = check_source_codes(
        r#"
type Static<T> =
  T extends { kind: "object"; props: infer P } ? { [K in keyof P]: Static<P[K]> } :
  T extends { kind: "array"; items: infer I } ? Static<I>[] :
  T extends { kind: "string" } ? string :
  T extends { kind: "number" } ? number :
  unknown;
type Schema = { kind: "object"; props: { a: { kind: "string" }; b: { kind: "number" } } };
type SchemaAlias = Schema;
type SchemaDouble = SchemaAlias;
type R0 = Static<Schema>;
type R1 = Static<SchemaAlias>;
type R2 = Static<SchemaDouble>;
declare const r0: R0;
const r1: R1 = r0;
const r2: R2 = r0;
"#,
    );
    assert!(
        !codes.contains(&2589),
        "fan-out concrete Static must not produce TS2589. Got: {codes:?}"
    );
    assert!(
        !codes.contains(&2322),
        "all alias spellings of Static<Schema> must be mutually assignable. Got: {codes:?}"
    );
}

/// remeda `FilteredArray<T, Cond>`-shaped distributive recursive conditional
/// over a concrete element union. Distribution produces one arm per member; a
/// concrete instantiation must converge and yield the expected filtered array.
#[test]
fn remeda_filtered_array_concrete_union_converges() {
    let codes = check_source_codes(
        r#"
type Filtered<T, Cond> =
  T extends readonly [infer H, ...infer R]
    ? H extends Cond
      ? [H, ...Filtered<R, Cond>]
      : Filtered<R, Cond>
    : [];
type Input = [1, "a", 2, "b", 3];
type OnlyNumbers = Filtered<Input, number>;
declare const nums: OnlyNumbers;
const ok: [1, 2, 3] = nums;
"#,
    );
    assert!(
        !codes.contains(&2589),
        "concrete Filtered<Input, number> must converge (no TS2589). Got: {codes:?}"
    );
    assert!(
        !codes.contains(&2322),
        "Filtered<[1,\"a\",2,\"b\",3], number> must be [1, 2, 3]. Got: {codes:?}"
    );
}

/// Deeper concrete nesting exercises the recursion budget while still
/// terminating; the shared fixpoint must keep it off the TS2589 path.
#[test]
fn typebox_static_deep_concrete_nesting_converges() {
    let codes = check_source_codes(
        r#"
type Static<T> =
  T extends { kind: "object"; props: infer P } ? { [K in keyof P]: Static<P[K]> } :
  T extends { kind: "array"; items: infer I } ? Static<I>[] :
  T extends { kind: "string" } ? string :
  T extends { kind: "number" } ? number :
  unknown;
type Deep = {
  kind: "object";
  props: {
    a: { kind: "object"; props: { b: { kind: "object"; props: { c: { kind: "string" } } } } };
    d: { kind: "array"; items: { kind: "array"; items: { kind: "number" } } };
  };
};
type R = Static<Deep>;
declare const r: R;
const ok: { a: { b: { c: string } }; d: number[][] } = r;
"#,
    );
    assert!(
        !codes.contains(&2589),
        "deep concrete Static must converge (no TS2589). Got: {codes:?}"
    );
    assert!(
        !codes.contains(&2322),
        "deep Static expansion must match. Got: {codes:?}"
    );
}

/// The generic (non-concrete) form is deliberately *not* shared as a global
/// fixpoint (its result depends on the free type parameter), so it must keep
/// behaving exactly as before: a generic wrapper over `Static<T>` instantiated
/// later with a concrete schema still resolves correctly.
#[test]
fn typebox_static_generic_wrapper_still_resolves() {
    let codes = check_source_codes(
        r#"
type Static<T> =
  T extends { kind: "object"; props: infer P } ? { [K in keyof P]: Static<P[K]> } :
  T extends { kind: "string" } ? string :
  T extends { kind: "number" } ? number :
  unknown;
type Wrap<T> = { value: Static<T> };
type Schema = { kind: "object"; props: { a: { kind: "string" } } };
type W = Wrap<Schema>;
declare const w: W;
const ok: { value: { a: string } } = w;
"#,
    );
    assert!(
        !codes.contains(&2589),
        "Wrap<Static<T>> instantiation must not produce TS2589. Got: {codes:?}"
    );
    assert!(
        !codes.contains(&2322),
        "Wrap<Schema>.value must equal {{ a: string }}. Got: {codes:?}"
    );
}

/// Renamed-binder variant: the convergence is structural, not keyed on the
/// alias/type-parameter/property names. Renaming every binder must produce the
/// same clean result.
#[test]
fn typebox_static_renamed_binders_converges() {
    let codes = check_source_codes(
        r#"
type Eval<Node> =
  Node extends { tag: "rec"; fields: infer F } ? { [Key in keyof F]: Eval<F[Key]> } :
  Node extends { tag: "list"; elem: infer E } ? Eval<E>[] :
  Node extends { tag: "text" } ? string :
  Node extends { tag: "num" } ? number :
  unknown;
type Tree = {
  tag: "rec";
  fields: {
    first: { tag: "text" };
    second: { tag: "list"; elem: { tag: "num" } };
  };
};
type Result = Eval<Tree>;
declare const result: Result;
const ok: { first: string; second: number[] } = result;
"#,
    );
    assert!(
        !codes.contains(&2589),
        "renamed-binder concrete Eval<Tree> must converge (no TS2589). Got: {codes:?}"
    );
    assert!(
        !codes.contains(&2322),
        "Eval<Tree> must equal its expansion regardless of binder names. Got: {codes:?}"
    );
}
