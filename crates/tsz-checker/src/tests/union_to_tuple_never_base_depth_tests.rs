//! Regression tests for spurious `TS2589` on the convergent `UnionToTuple` /
//! `LastOf` family of recursive aliases (issue #14174).
//!
//! Structural rule: `never` is the bottom type, so a non-distributive
//! `never extends T ? X : Y` always selects the true branch, and any `infer`
//! variable inside the structural pattern `T` has no candidate to infer from the
//! empty `never` source and therefore resolves to its default, `unknown`
//! (matching `tsc`'s `inferTypes`). The `UnionToTuple` family relies on exactly
//! this: its base case is reached when the union is exhausted to `never`, at
//! which point `UnionToIntersection<never>` evaluates to `unknown` and fails the
//! `extends () => infer TLast` test, returning the accumulator. When tsz instead
//! evaluated such an `infer`-from-`never` to `never`, the recursion never reached
//! its base case, grew the accumulator unboundedly, and tripped a spurious
//! TS2589.
//!
//! Anti-hardcoding: the rule is exercised with renamed binders and with the
//! `infer` appearing in covariant (return), contravariant (parameter), array,
//! tuple, and object positions, so the fix cannot be satisfied by matching a
//! specific identifier or a single pattern shape. A genuinely non-terminating
//! companion still reports TS2589.

use crate::context::CheckerOptions;
use crate::test_utils::{check_source_with_libs, load_default_lib_files};

fn strict_codes(source: &str) -> Vec<u32> {
    let libs = load_default_lib_files();
    check_source_with_libs(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
        &libs,
    )
    .into_iter()
    .map(|d| d.code)
    .collect()
}

const TS2589: u32 = 2589;
const TS2322: u32 = 2322;

/// Issue #14174 repro: the accumulator-style `UnionToTuple` helper that excludes
/// the inferred last member each step. `tsc` accepts it (result `[unknown]`); tsz
/// must not emit TS2589.
const UNION_TO_TUPLE: &str = r#"
type UnionToIntersect<TUnion> =
  (TUnion extends any ? (arg: TUnion) => void : never) extends (arg: infer Intersect) => void
    ? Intersect
    : never;
type UnionToTupleHelper<TUnion, TResult extends any[]> =
  UnionToIntersect<TUnion> extends () => infer TLast
    ? UnionToTupleHelper<Exclude<TUnion, TLast>, [TLast, ...TResult]>
    : TResult;
type UnionToTuple<TUnion> = UnionToTupleHelper<TUnion, []>;
"#;

#[test]
fn union_to_tuple_through_keyof_alias_is_not_excessively_deep() {
    let source = format!(
        r#"
{UNION_TO_TUPLE}
type ObjectKeys<T> = UnionToTuple<keyof T>;
declare const s: {{ a: 1; b: 2 }};
type K = ObjectKeys<typeof s>;
declare const k: K;
export {{ k }};
"#
    );
    let codes = strict_codes(&source);
    assert!(
        !codes.contains(&TS2589),
        "convergent UnionToTuple must not emit TS2589. Got: {codes:?}"
    );
}

#[test]
fn union_to_tuple_helper_directly_is_not_excessively_deep() {
    let source = format!(
        r#"
{UNION_TO_TUPLE}
declare const s: {{ a: 1; b: 2 }};
type K = UnionToTupleHelper<keyof typeof s, []>;
declare const k: K;
export {{ k }};
"#
    );
    let codes = strict_codes(&source);
    assert!(
        !codes.contains(&TS2589),
        "convergent UnionToTuple helper must not emit TS2589. Got: {codes:?}"
    );
}

/// Anti-hardcoding: renamed binders (`TUnion`/`TResult` -> `U`/`Acc`,
/// `UnionToTupleHelper` -> `Build`) behave identically.
#[test]
fn renamed_union_to_tuple_is_not_excessively_deep() {
    let source = r#"
type ToIntersection<U> =
  (U extends any ? (a: U) => void : never) extends (a: infer I) => void ? I : never;
type Build<U, Acc extends any[]> =
  ToIntersection<U> extends () => infer L
    ? Build<Exclude<U, L>, [L, ...Acc]>
    : Acc;
type Keys<T> = Build<keyof T, []>;
declare const s: { x: 1; y: 2; z: 3 };
type K = Keys<typeof s>;
declare const k: K;
export { k };
"#;
    let codes = strict_codes(source);
    assert!(
        !codes.contains(&TS2589),
        "renamed convergent UnionToTuple must not emit TS2589. Got: {codes:?}"
    );
}

/// The base case: `never extends (...) => infer X` resolves each `infer` to
/// `unknown` and takes the true branch. Asserting the *value* via an
/// intentional mismatch pins the rule, not just the absence of TS2589.
#[test]
fn infer_from_never_resolves_to_unknown_in_function_pattern() {
    // `A = unknown`; assigning to `number` reveals it as TS2322 with `unknown`.
    let source = r#"
type A = never extends (arg: infer P) => void ? P : "FALSE";
const a: number = null as any as A;
"#;
    let codes = strict_codes(source);
    assert!(
        codes.contains(&TS2322),
        "infer-from-never must yield `unknown` (true branch), failing assignment to number. Got: {codes:?}"
    );
    assert!(
        !codes.contains(&TS2589),
        "infer-from-never must not be excessively deep. Got: {codes:?}"
    );
}

/// Covariant, array, tuple, and object positions all resolve to `unknown`
/// (true branch), matching `tsc`. Each line is independently assignable to
/// `unknown`, so a correct fix produces no diagnostics at all.
#[test]
fn infer_from_never_unknown_across_positions() {
    let source = r#"
type Ret    = never extends () => infer R ? R : "F";
type Cov    = never extends Array<infer E> ? E : "F";
type Tup    = never extends [infer X, infer Y] ? [X, Y] : "F";
type Obj    = never extends { k: infer V } ? V : "F";
const r: unknown = null as any as Ret;
const c: unknown = null as any as Cov;
const t: [unknown, unknown] = null as any as Tup;
const o: unknown = null as any as Obj;
export { r, c, t, o };
"#;
    let codes = strict_codes(source);
    assert!(
        codes.is_empty(),
        "infer-from-never resolves to `unknown` in every position (assignable to unknown). Got: {codes:?}"
    );
}

/// Negative control: a genuinely non-terminating accumulator (the length check
/// can never match because the accumulator grows two elements per step against
/// an odd bound) must still report TS2589 — the `infer`-from-`never` relaxation
/// must not blanket-suppress real divergence. Matches `tsc`, which reports
/// TS2589 for `Never<3>`.
#[test]
fn non_terminating_accumulator_still_reports_ts2589() {
    let source = r#"
type Never<L extends number, T extends any[] = []> =
  T['length'] extends L ? T : Never<L, [...T, any, any]>;
type R = Never<3>;
declare const r: R;
export { r };
"#;
    let codes = strict_codes(source);
    assert!(
        codes.contains(&TS2589),
        "genuinely non-terminating recursion must still report TS2589. Got: {codes:?}"
    );
}
