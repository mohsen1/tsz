//! Regression tests for homomorphic mapped types over a tuple whose rest is an
//! *opaque* element (a deferred application / type parameter, not a concrete
//! array or tuple).
//!
//! Structural rule: when a homomorphic mapped type `{ [K in keyof T]: F<T[K]> }`
//! instantiates over a tuple `[E0, ...R]` whose rest `R` cannot yet resolve to a
//! concrete tuple/array, the mapped rest slot must index the rest element's own
//! type (`F<R[number]>`), not the whole source tuple (`F<T[number]>`). Indexing
//! the whole tuple folds the already-fixed prefix slots into the rest, so a
//! later per-slot reification (`[infer H, ...infer T]` variadic rebuild) re-emits
//! the prefix element and duplicates it — e.g. `[E0, E0, ...]` instead of
//! `[E0, E1, ...]` (issue #14518). `tsc` preserves per-index element identity
//! through these transforms; tsz must too.
//!
//! The owner is the solver's mapped-type instantiation
//! (`instantiate/mapped.rs`, the `OpaqueRest` element binding).

use crate::test_utils::check_source_codes;

fn assert_no_diagnostics(src: &str) {
    let codes = check_source_codes(src);
    assert!(
        codes.is_empty(),
        "expected no diagnostics, got: {codes:?}\nsrc:\n{src}"
    );
}

/// The minimal witness: a homomorphic mapped (`NormalizeBox`) composed over a
/// recursive variadic rebuild (`DeepRO`), forced through a conditional
/// `[infer H, ...infer T]` match (`Probe`). Before the fix the inner mapped
/// produced `[E0, ...F<WholeTuple[number]>]`; the `Probe` residual reified the
/// rest over the whole tuple and re-emitted `E0` into slot 1, so `p[1]` came
/// back as element 0's type and the explicit annotations below failed.
#[test]
fn probe_over_normalize_deepro_preserves_per_slot_identity() {
    assert_no_diagnostics(
        r#"
type NormalizeBox<I> = I extends object ? { [F in keyof I]: NormalizeBox<I[F]> } : I;
type DeepRO<S> =
  S extends readonly [infer A, ...infer B] ? readonly [DeepRO<A>, ...DeepRO<B>]
  : S extends object ? { readonly [N in keyof S]: DeepRO<S[N]> } : S;
type Tup = readonly [{ a: 1 }, { b: 2 }, { wrapped: 3 }];
type Probe<X> = X extends readonly [infer H, ...infer T] ? readonly [H, ...T] : never;

declare const p: Probe<NormalizeBox<DeepRO<Tup>>>;
const e0: { readonly a: 1 } = p[0];
const e1: { readonly b: 2 } = p[1];
const e2: { readonly wrapped: 3 } = p[2];
"#,
    );
}

/// Same structural scenario with every binder renamed (utilities, type
/// parameters, infer variables, tuple member keys). The fix is structural, so
/// the result must not depend on any chosen identifier (anti-hardcoding gate).
#[test]
fn probe_over_renamed_utilities_preserves_per_slot_identity() {
    assert_no_diagnostics(
        r#"
type Wrap<Q> = Q extends object ? { [Key in keyof Q]: Wrap<Q[Key]> } : Q;
type FreezeDeep<Src> =
  Src extends readonly [infer Head2, ...infer Tail2] ? readonly [FreezeDeep<Head2>, ...FreezeDeep<Tail2>]
  : Src extends object ? { readonly [Nm in keyof Src]: FreezeDeep<Src[Nm]> } : Src;
type Triple = readonly [{ first: 10 }, { second: 20 }, { marker: 30 }];
type Split<In> = In extends readonly [infer First, ...infer Rest] ? readonly [First, ...Rest] : never;

declare const probe: Split<Wrap<FreezeDeep<Triple>>>;
const s0: { readonly first: 10 } = probe[0];
const s1: { readonly second: 20 } = probe[1];
const s2: { readonly marker: 30 } = probe[2];
"#,
    );
}

/// A directly-authored homomorphic mapped over a tuple with a *concrete* prefix
/// and a deferred (opaque) rest, indexed per slot. This exercises the same
/// `OpaqueRest` instantiation path without the conditional-rebuild wrapper, and
/// is the positive control that the prefix slot's identity is preserved.
#[test]
fn homomorphic_mapped_over_prefix_plus_opaque_rest_keeps_prefix_slot() {
    assert_no_diagnostics(
        r#"
type Box<T> = { boxed: T };
type Boxify<Tpl extends readonly unknown[]> = { [K in keyof Tpl]: Box<Tpl[K]> };
type DeferRest<S> = S extends readonly [infer A, ...infer B] ? readonly [A, ...B] : S;

type Input = readonly ["lead", 1, 2];
declare const r: Boxify<DeferRest<Input>>;
const r0: Box<"lead"> = r[0];
const r1: Box<1> = r[1];
const r2: Box<2> = r[2];
"#,
    );
}
