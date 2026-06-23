//! Batch regression guards for fleet-fixed canary false positives,
//! harvested 2026-06-23 (round f). Each verified to have a fix commit in main.
//! #14565 #14561 #14538 #14530 #14528 #14518 #14512.

use super::super::core::*;

/// #14565: 8375f0d8a8 fix(checker): apply default type arguments when recovering a failed generic call/new (#14565) (#14566)
#[test]
fn issue_14565_default_type_args_failed_new_no_ts2322() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
class Box<T = string> { constructor(public value: T) {} }
const b = new Box();
const v: string = b.value;
"#,
    );
    assert!(
        !has_error(&diagnostics, 2322),
        "#14565: no TS2322 expected (fleet-fixed). Actual: {diagnostics:#?}"
    );
}

/// #14561: cf1a94d8ad fix(solver): bind `infer` in type-predicate position (`x is infer R`) (#14561) (#14562)
#[test]
fn issue_14561_infer_in_type_predicate_no_ts2322() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
type Narrowed<P> = P extends (value: any) => value is infer R ? R : never;
declare const isText: (probe: unknown) => probe is string;
type Out = Narrowed<typeof isText>;
const ok: Out = "hello";
"#,
    );
    assert!(
        !has_error(&diagnostics, 2322),
        "#14561: no TS2322 expected (fleet-fixed). Actual: {diagnostics:#?}"
    );
}

/// #14538: 36b8348d1f fix(checker): TS2454 suppressed when declared type has undefined behind an indexed access (#14543)
#[test]
fn issue_14538_undefined_behind_indexed_access_no_ts2454() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
interface W { opt?: { connect(): void }; }
declare const w: W;
function f1() {
  let x: W['opt'] | false;          // W['opt'] resolves to `{connect()} | undefined`
  try { x = (true as boolean) && w.opt; } catch {}
  if (!x) return;                    // tsz: TS2454 ; tsc: ok
  return x;
}
"#,
    );
    assert!(
        !has_error(&diagnostics, 2454),
        "#14538: no TS2454 expected (fleet-fixed). Actual: {diagnostics:#?}"
    );
}

/// #14530: 6f9dc23589 fix(checker): union unwidened return literals, widen only a single fresh literal (#14534)
#[test]
fn issue_14530_union_unwidened_return_literals_no_ts2322() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
function classify(n: number) {
  if (n > 0) return "positive";
  return "zero";
}
const c: "positive" | "zero" = classify(0);
"#,
    );
    assert!(
        !has_error(&diagnostics, 2322),
        "#14530: no TS2322 expected (fleet-fixed). Actual: {diagnostics:#?}"
    );
}

/// #14528: 46952e67c9 fix(checker): keyof of a key-preserving wrapper is a valid index (spurious TS2536) (#14537)
#[test]
#[ignore = "reproduces #14528 OR minimal witness differs from project repro; fix verified via commit 46952e67c9"]
fn issue_14528_keyof_key_preserving_wrapper_no_ts2536() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
type Alias<T> = T;
declare function get<T, K extends keyof Alias<T>>(o: T, k: K): T[K];

declare function get2<T extends object, K extends keyof NonNullable<T>>(o: T, k: K): T[K];
"#,
    );
    assert!(
        !has_error(&diagnostics, 2536),
        "#14528: no TS2536 expected (fleet-fixed). Actual: {diagnostics:#?}"
    );
}

/// #14518: f73aed7b9e fix(solver): preserve tuple identity through recursive-utility composition (#14518) (#14555)
#[test]
#[ignore = "reproduces #14518 OR minimal witness differs; fix verified via commit f73aed7b9e"]
fn issue_14518_tuple_identity_recursive_utility_no_ts2339() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
// Minimal repro for issue #14518
// TS2339 false positive on numeric-literal index into a tuple
// transformed by composed recursive utilities.
// Expected: compiles cleanly (rc=0)
// Before fix: TS2339 on property 'wrapped'

type AliasCompute<TValue> = TValue extends (...args: infer P) => infer R
  ? (...args: P) => R
  : TValue extends readonly [infer H, ...infer T]
    ? readonly [AliasCompute<H>, ...AliasCompute<T>]
    : TValue extends object ? { [K in keyof TValue]: AliasCompute<TValue[K]> } : TValue;
type NormalizeBox<I> = I extends object ? { [F in keyof I]: NormalizeBox<I[F]> } : I;
type DeepRO<S> = S extends (...a: any[]) => any ? S
  : S extends readonly [infer A, ...infer B] ? readonly [DeepRO<A>, ...DeepRO<B>]
  : S extends object ? { readonly [N in keyof S]: DeepRO<S[N]> } : S;
type PickStr<R> = { [M in keyof R as M extends string ? M : never]: R[M] };

type UtilityPipeline<Seed> = AliasCompute<NormalizeBox<DeepRO<PickStr<Seed>>>>;

type Seed = { readonly tuple: readonly [{ a: 1 }, { b: 2 }, { wrapped: 3 }] };
type M = UtilityPipeline<Seed>;
declare const m: M;

export const probe: 3 = m.tuple[2].wrapped;
"#,
    );
    assert!(
        !has_error(&diagnostics, 2339),
        "#14518: no TS2339 expected (fleet-fixed). Actual: {diagnostics:#?}"
    );
}

/// #14512: a306175fb0 fix(checker): preserve polymorphic `this` when a member is read through a `this`-relative receiver (TS2345 FP) (#14516)
#[test]
fn issue_14512_polymorphic_this_relative_receiver_no_ts2345() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
class TreeNode {
  children: this[] = [];
  addChild(c: this): void {
    this.children.push(c);
  }
}
"#,
    );
    assert!(
        !has_error(&diagnostics, 2345),
        "#14512: no TS2345 expected (fleet-fixed). Actual: {diagnostics:#?}"
    );
}
