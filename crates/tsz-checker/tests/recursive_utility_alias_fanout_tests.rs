//! Regression tests for issue #10826: recursive utility expansion must not
//! over-instantiate when the same structural type is reached through different
//! alias chains (alias fan-out).
//!
//! Structural rule: when `Application(DefId, [args])` is evaluated and the
//! args contain `Lazy` type-alias references, the solver must normalize the
//! args transitively to their canonical structural bodies before consulting
//! the `application_eval_cache`. This ensures that `DeepObject<Anchor, 0>`
//! and `DeepObject<AnchorAlias, 0>` (where `AnchorAlias = Anchor`) share a
//! single cache entry rather than triggering separate re-evaluations.
//!
//! Adjacent cases: direct alias, one-indirection alias, two-indirection alias,
//! concrete struct (no alias), primitive arg, node-only recursive struct,
//! N=0 (base case), N=1 (mapped over object-only keys), TS2589 absence.

use tsz_checker::test_utils::check_source_codes;

/// `BuildTuple<0>` has `A = [] (default)`, `A['length'] = 0 extends 0` → true →
/// returns `A = []`. No recursion, no depth issues.
#[test]
fn build_tuple_zero_terminates_to_empty_tuple() {
    let codes = check_source_codes(
        r#"
type BuildTuple<N, A extends any[] = []> =
  A['length'] extends N ? A : BuildTuple<N, [...A, any]>;
type BT0 = BuildTuple<0>;
declare const x: BT0;
const y: [] = x;
"#,
    );
    assert!(
        codes.is_empty(),
        "BuildTuple<0> should produce [] with no diagnostics, got: {codes:?}"
    );
}

/// `BuildTuple<1>` expands once (`0 ≠ 1`), recurses with `A=[any]`,
/// `1 extends 1` → true → `[any]`. Bounded, no TS2589.
#[test]
fn build_tuple_one_terminates_to_single_element_tuple() {
    let codes = check_source_codes(
        r#"
type BuildTuple<N, A extends any[] = []> =
  A['length'] extends N ? A : BuildTuple<N, [...A, any]>;
type BT1 = BuildTuple<1>;
"#,
    );
    assert!(
        !codes.contains(&2589),
        "BuildTuple<1> must not produce TS2589, got: {codes:?}"
    );
}

/// `DeepObject<T, 0>` — the conditional `BuildTuple<0> extends []` is true
/// (base case), so the result is `T` directly. No mapped recursion occurs.
#[test]
fn deep_object_n_zero_reduces_to_t_no_error() {
    let codes = check_source_codes(
        r#"
type BuildTuple<N, A extends any[] = []> =
  A['length'] extends N ? A : BuildTuple<N, [...A, any]>;
type DeepObject<T, N extends number> =
  BuildTuple<N> extends [] ? T : { [K in keyof T]: DeepObject<T[K], N> };
type Anchor = { value: string; nested?: Anchor };
type Fixed = DeepObject<Anchor, 0>;
declare const anchor: Anchor;
const fixed: Fixed = anchor;
"#,
    );
    assert!(
        !codes.contains(&2589),
        "DeepObject<Anchor, 0> must not produce TS2589. Got: {codes:?}"
    );
    assert!(
        !codes.contains(&2322),
        "DeepObject<Anchor, 0> = Anchor so assignment must be valid. Got: {codes:?}"
    );
}

/// Alias fan-out (single indirection): `AnchorAlias = Anchor`. The transitive
/// Lazy normalization ensures both `DeepObject<Anchor, 0>` and
/// `DeepObject<AnchorAlias, 0>` share the same cache entry and structural
/// result, making the assignment valid.
#[test]
fn deep_object_alias_fanout_single_indirection_shares_result() {
    let codes = check_source_codes(
        r#"
type BuildTuple<N, A extends any[] = []> =
  A['length'] extends N ? A : BuildTuple<N, [...A, any]>;
type DeepObject<T, N extends number> =
  BuildTuple<N> extends [] ? T : { [K in keyof T]: DeepObject<T[K], N> };
type Anchor = { value: string; nested?: Anchor };
type AnchorAlias = Anchor;
type Fixed = DeepObject<Anchor, 0>;
type FixedAlias = DeepObject<AnchorAlias, 0>;
declare const f: Fixed;
const fa: FixedAlias = f;
"#,
    );
    assert!(
        !codes.contains(&2589),
        "DeepObject<AnchorAlias, 0> must not produce TS2589. Got: {codes:?}"
    );
    assert!(
        !codes.contains(&2322),
        "DeepObject<Anchor, 0> and DeepObject<AnchorAlias, 0> must be assignable. Got: {codes:?}"
    );
}

/// Two-level alias chain: `C = B = Anchor`. The transitive normalization
/// must follow the full chain so `DeepObject<C, 0>` hits the same cache
/// entry as `DeepObject<Anchor, 0>`.
#[test]
fn deep_object_alias_fanout_two_indirections_shares_result() {
    let codes = check_source_codes(
        r#"
type BuildTuple<N, A extends any[] = []> =
  A['length'] extends N ? A : BuildTuple<N, [...A, any]>;
type DeepObject<T, N extends number> =
  BuildTuple<N> extends [] ? T : { [K in keyof T]: DeepObject<T[K], N> };
type Anchor = { value: string; nested?: Anchor };
type AnchorAlias = Anchor;
type AnchorDouble = AnchorAlias;
type Fixed = DeepObject<Anchor, 0>;
type FixedDouble = DeepObject<AnchorDouble, 0>;
declare const f: Fixed;
const fd: FixedDouble = f;
"#,
    );
    assert!(
        !codes.contains(&2589),
        "DeepObject<AnchorDouble, 0> must not produce TS2589. Got: {codes:?}"
    );
    assert!(
        !codes.contains(&2322),
        "Two-level alias chain must produce compatible result. Got: {codes:?}"
    );
}

/// Concrete struct arg — no alias involved.
/// `DeepObject<{x: number}, 0>` = `{x: number}`. Confirms the fix does
/// not regress the plain-struct path.
#[test]
fn deep_object_concrete_struct_n_zero_no_error() {
    let codes = check_source_codes(
        r#"
type BuildTuple<N, A extends any[] = []> =
  A['length'] extends N ? A : BuildTuple<N, [...A, any]>;
type DeepObject<T, N extends number> =
  BuildTuple<N> extends [] ? T : { [K in keyof T]: DeepObject<T[K], N> };
type Fixed = DeepObject<{ x: number; y: string }, 0>;
declare const obj: { x: number; y: string };
const fixed: Fixed = obj;
"#,
    );
    assert!(
        codes.is_empty(),
        "DeepObject<{{x:number;y:string}}, 0> must have no errors. Got: {codes:?}"
    );
}

/// Primitive arg — `DeepObject<string, 0>` = `string`. No mapped recursion.
#[test]
fn deep_object_primitive_arg_n_zero_no_error() {
    let codes = check_source_codes(
        r#"
type BuildTuple<N, A extends any[] = []> =
  A['length'] extends N ? A : BuildTuple<N, [...A, any]>;
type DeepObject<T, N extends number> =
  BuildTuple<N> extends [] ? T : { [K in keyof T]: DeepObject<T[K], N> };
type FS = DeepObject<string, 0>;
declare const s: string;
const fs: FS = s;
"#,
    );
    assert!(
        codes.is_empty(),
        "DeepObject<string, 0> must produce string with no errors. Got: {codes:?}"
    );
}

/// Triple alias fan-out: three independent aliases of `Anchor` all resolve to
/// the same `DeepObject<Anchor, 0>` cache entry. Confirms the transitive
/// normalization scales beyond two aliases without introducing separate
/// evaluations or false TS2322 errors.
#[test]
fn deep_object_triple_alias_fanout_all_assignable() {
    let codes = check_source_codes(
        r#"
type BuildTuple<N, A extends any[] = []> =
  A['length'] extends N ? A : BuildTuple<N, [...A, any]>;
type DeepObject<T, N extends number> =
  BuildTuple<N> extends [] ? T : { [K in keyof T]: DeepObject<T[K], N> };
type Anchor = { value: string; nested?: Anchor };
type A1 = Anchor;
type A2 = Anchor;
type D  = DeepObject<Anchor, 0>;
type D1 = DeepObject<A1, 0>;
type D2 = DeepObject<A2, 0>;
declare const d: D;
const d1: D1 = d;
const d2: D2 = d;
"#,
    );
    assert!(
        !codes.contains(&2589),
        "Triple alias fan-out must not produce TS2589. Got: {codes:?}"
    );
    assert!(
        !codes.contains(&2322),
        "All three DeepObject aliases must be mutually assignable. Got: {codes:?}"
    );
}

/// Alias in nested position: `DeepObject<{ inner: AnchorAlias }, 0>` must
/// equal `DeepObject<{ inner: Anchor }, 0>` structurally. This exercises
/// transitive normalization when the alias appears as a field type rather
/// than the top-level arg.
#[test]
fn deep_object_alias_in_nested_field_no_error() {
    let codes = check_source_codes(
        r#"
type BuildTuple<N, A extends any[] = []> =
  A['length'] extends N ? A : BuildTuple<N, [...A, any]>;
type DeepObject<T, N extends number> =
  BuildTuple<N> extends [] ? T : { [K in keyof T]: DeepObject<T[K], N> };
type Anchor = { value: string; nested?: Anchor };
type AnchorAlias = Anchor;
type WrapDirect = DeepObject<{ inner: Anchor }, 0>;
type WrapAlias  = DeepObject<{ inner: AnchorAlias }, 0>;
declare const wd: WrapDirect;
const wa: WrapAlias = wd;
"#,
    );
    assert!(
        !codes.contains(&2589),
        "Alias in nested field must not produce TS2589. Got: {codes:?}"
    );
    assert!(
        !codes.contains(&2322),
        "WrapDirect and WrapAlias must be assignable. Got: {codes:?}"
    );
}

/// Wrapper utility: `type Wrap<T> = { inner: DeepObject<T, 0> }`.
/// Ensures the fix composes correctly when `DeepObject` is nested inside
/// another generic wrapper.
#[test]
fn deep_object_nested_in_wrapper_no_error() {
    let codes = check_source_codes(
        r#"
type BuildTuple<N, A extends any[] = []> =
  A['length'] extends N ? A : BuildTuple<N, [...A, any]>;
type DeepObject<T, N extends number> =
  BuildTuple<N> extends [] ? T : { [K in keyof T]: DeepObject<T[K], N> };
type Anchor = { value: string; nested?: Anchor };
type Wrap<T> = { inner: DeepObject<T, 0> };
type WA = Wrap<Anchor>;
declare const w: WA;
const check: { inner: Anchor } = w;
"#,
    );
    assert!(
        !codes.contains(&2589),
        "Wrap<Anchor> nesting must not produce TS2589. Got: {codes:?}"
    );
    assert!(
        !codes.contains(&2322),
        "Wrap<Anchor>.inner = Anchor assignment must be valid. Got: {codes:?}"
    );
}

/// Cross-block sub-application reuse (the `Recursive utility aliases` perf
/// hotspot). Two independent blocks apply the same recursive
/// conditional/mapped utility pipeline to a structurally identical concrete
/// seed, so the shared leaf sub-applications (`DeepReadonly<Leaf>`, …) are
/// re-reached from different alias positions.
///
/// The first-pass `TypeEnvironment` evaluator now participates in the
/// cross-call application-eval/instantiation caches so that shared sub-work is
/// computed once and reused. This test guards the *correctness* invariant of
/// that reuse: the deep property access must still resolve to `string` in every
/// block, exactly as without caching.
#[test]
fn recursive_utility_alias_cross_block_reuse_preserves_results() {
    let codes = check_source_codes(
        r#"
type DeepReadonly<T> = T extends (...args: any[]) => any
    ? T
    : T extends readonly [infer H, ...infer R]
        ? readonly [DeepReadonly<H>, ...DeepReadonly<R>]
        : T extends object
            ? { readonly [K in keyof T]: DeepReadonly<T[K]> }
            : T;
type Pipeline<S> = DeepReadonly<{ [K in keyof S as K extends string ? K : never]: S[K] }>;

interface Leaf { id: string; flags: { labels: readonly ["a", "b"] }; }

type Variant0<S0> = Pipeline<{ item0: S0; nested0: { right: Leaf } }>;
type Materialized0 = Variant0<{ value: Leaf }>;
declare const m0: Materialized0;
const v0: string = m0.nested0.right.flags.labels[0];

type Variant1<S1> = Pipeline<{ item1: S1; nested1: { right: Leaf } }>;
type Materialized1 = Variant1<{ value: Leaf }>;
declare const m1: Materialized1;
const v1: string = m1.nested1.right.flags.labels[0];
"#,
    );
    assert!(
        codes.is_empty(),
        "shared-leaf recursive utility blocks must type-check cleanly with deep \
         access resolving to string; got: {codes:?}"
    );
}

/// Safety gate for limited-resolver caching: a recursive alias whose argument
/// is itself an alias chain (`AnchorAlias = Anchor`) shares the same
/// `(DefId, args)` application-eval entry as the direct form. The first-pass
/// evaluator must not persist an under-resolved result under that
/// resolver-independent key, so both spellings must agree (no spurious TS2322
/// or TS2589).
#[test]
fn recursive_utility_alias_chain_arg_reuse_is_consistent() {
    let codes = check_source_codes(
        r#"
type DeepReadonly<T> = T extends object
    ? { readonly [K in keyof T]: DeepReadonly<T[K]> }
    : T;
type Anchor = { value: string; child: { value: string } };
type AnchorAlias = Anchor;
type Direct = DeepReadonly<Anchor>;
type ViaAlias = DeepReadonly<AnchorAlias>;
declare const d: Direct;
const a: ViaAlias = d;
const b: Direct = a;
"#,
    );
    assert!(
        !codes.contains(&2589) && !codes.contains(&2322),
        "direct and alias-chain DeepReadonly applications must agree; got: {codes:?}"
    );
}
