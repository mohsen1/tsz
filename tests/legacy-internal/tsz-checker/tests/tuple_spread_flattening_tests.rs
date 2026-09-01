//! Regression tests for inlining concrete tuple spreads (`createNormalizedTupleType`).
//!
//! A rest element `...X` whose type resolves to a concrete tuple contributes a
//! statically known run of elements, so `[A, ...[B, C]]` is exactly
//! `[A, B, C]`. Before the fix, a spread whose operand was an **alias**
//! (`...Aliased`), a **pending application** (`...Util<T>`), or a **readonly**
//! tuple (`...readonly [B, C]`) stayed un-inlined, and value-position numeric
//! reads past the fixed prefix fell back to the whole-tuple element union
//! (`head | [tail]`) — surfacing as spurious `TS2339`/`TS2493`/`TS2322`.
//!
//! Per the anti-hardcoding gate, the binder names (`A`/`Head`, `Tail`, etc.)
//! vary across cases so the tests pin the structural rule, not a spelling.

use crate::test_utils::check_source_diagnostics;

/// No spurious property/index/assignability diagnostics for a structurally
/// valid program.
fn no_access_errors(src: &str) -> bool {
    check_source_diagnostics(src)
        .iter()
        .all(|d| !matches!(d.code, 2339 | 2493 | 2322 | 2740))
}

fn codes(src: &str) -> Vec<i32> {
    let mut v: Vec<i32> = check_source_diagnostics(src)
        .iter()
        .map(|d| d.code as i32)
        .collect();
    v.sort_unstable();
    v
}

// ── Alias spread: `[A, ...AliasTuple]` ───────────────────────────────────────

#[test]
fn alias_fixed_tuple_spread_inlines_for_value_access() {
    // `[{ w: 3 }, ...Plain]` is `[{ w: 3 }, { x: 1 }]`; `t[1]` is `{ x: 1 }`.
    assert!(no_access_errors(
        r#"
type Plain = readonly [{ x: 1 }];
type Combined = readonly [{ w: 3 }, ...Plain];
declare const t: Combined;
const head: 3 = t[0].w;
const tail: 1 = t[1].x;
"#
    ));
}

#[test]
fn alias_spread_renamed_binders_behave_identically() {
    // Renaming the alias and members must not change the outcome (§ anti-hardcoding).
    assert!(no_access_errors(
        r#"
type Suffix = readonly [{ beta: 2 }, { gamma: 3 }];
type Whole = readonly [{ alpha: 1 }, ...Suffix];
declare const whole: Whole;
const a: 1 = whole[0].alpha;
const b: 2 = whole[1].beta;
const g: 3 = whole[2].gamma;
"#
    ));
}

// ── readonly spread: `[A, ...readonly [B, C]]` ───────────────────────────────

#[test]
fn readonly_tuple_spread_inlines_for_value_access() {
    assert!(no_access_errors(
        r#"
declare const t: readonly [{ w: 3 }, ...readonly [{ x: 1 }, { y: 2 }]];
const x: 1 = t[1].x;
const y: 2 = t[2].y;
"#
    ));
}

// ── Application spread: `[A, ...Util<T>]` ────────────────────────────────────

#[test]
fn application_tuple_spread_inlines_for_value_access() {
    // `Id<[{ x: 1 }, { y: 2 }]>` reduces to the tuple, then spreads inline.
    assert!(no_access_errors(
        r#"
type Id<X> = X extends unknown ? X : never;
declare const t: readonly [{ w: 3 }, ...Id<readonly [{ x: 1 }, { y: 2 }]>];
const x: 1 = t[1].x;
const y: 2 = t[2].y;
"#
    ));
}

// ── The `head | tail` union regression (recursive list utility) ──────────────

#[test]
fn recursive_tuple_map_flattens_fully() {
    // `Map<[1,2,3]> = [Head, ...Map<Rest>]` must flatten to `[1, 2, 3]`, not a
    // nested `[1, ...[2, ...[3, ...[]]]]`. Reading `m[2]` must yield `3`.
    assert!(no_access_errors(
        r#"
type MapTuple<T> = T extends readonly [infer Head, ...infer Rest]
    ? readonly [Head, ...MapTuple<Rest>]
    : T;
declare const m: MapTuple<readonly [1, 2, 3]>;
const a: 1 = m[0];
const b: 2 = m[1];
const c: 3 = m[2];
"#
    ));
}

#[test]
fn recursive_object_tuple_map_finds_deep_property() {
    // The original witness shape: a recursive conditional that wraps each tuple
    // element. Element 2's `wrapped` property must be reachable.
    assert!(no_access_errors(
        r#"
type Compute<V> = V extends readonly [infer H, ...infer T]
    ? readonly [Compute<H>, ...Compute<T>]
    : V extends object
        ? { [K in keyof V]: Compute<V[K]> }
        : V;
declare const u: Compute<readonly [{ a: 1 }, { b: 2 }, { wrapped: 3 }]>;
const w: 3 = u[2].wrapped;
"#
    ));
}

// ── Negative control: genuine errors must still fire ─────────────────────────

#[test]
fn wrong_element_type_after_flattening_still_errors() {
    // `t[1]` is `{ x: 1 }`; assigning to an incompatible annotation must error.
    assert!(
        !codes(
            r#"
type Plain = readonly [{ x: 1 }];
declare const t: readonly [{ w: 3 }, ...Plain];
const bad: { x: 2 } = t[1];
"#
        )
        .is_empty()
    );
}

// ── Variadic preservation: rest arrays must not be over-spliced ──────────────

#[test]
fn rest_array_spread_is_preserved() {
    // `[A, ...number[]]` stays variadic; numeric reads still resolve to the
    // element type and length stays `number`, not a literal.
    assert!(no_access_errors(
        r#"
declare const t: readonly [string, ...number[]];
const head: string = t[0];
const rest: number = t[5];
"#
    ));
}

#[test]
fn variadic_tail_tuple_spread_keeps_single_rest() {
    // `[A, ...[B, ...C[]]]` flattens to `[A, B, ...C[]]` — fixed prefix readable,
    // trailing rest preserved.
    assert!(no_access_errors(
        r#"
type Tail = readonly [boolean, ...number[]];
declare const t: readonly [string, ...Tail];
const a: string = t[0];
const b: boolean = t[1];
const c: number = t[7];
"#
    ));
}

// ── Homomorphic mapped over a tuple, through recursive utility composition ───
//
// `{ [K in keyof T]: F<T[K]> }` is homomorphic over `T` even when the per-key
// value passes through another utility `F`, so the tuple structure (and each
// element's per-index identity) must be preserved. Before the fix the mapped
// collapsed the tuple to an array (`F<T[number]>[]`) because the homomorphic
// source could not be recovered through the `F<…>` application wrapper, and the
// variadic rebuild folded every spread index onto a single `source[index]`.

/// The four-utility composition from the bug report: a homomorphic mapped
/// (`NormalizeBox`/`AliasCompute`) sits over a variadic-rebuilt tuple
/// (`DeepRO`) reached through a key-remap mapped (`PickStr`). `tuple[2]` must be
/// the third element, so `.wrapped` resolves.
#[test]
fn recursive_utility_composition_preserves_tuple_index_identity() {
    assert!(no_access_errors(
        r#"
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
type UtilityPipeline<X> = AliasCompute<NormalizeBox<DeepRO<PickStr<X>>>>;
type Seed = { readonly tuple: readonly [{ a: 1 }, { b: 2 }, { wrapped: 3 }] };
declare const m: UtilityPipeline<Seed>;
const probe: 3 = m.tuple[2].wrapped;
"#
    ));
}

/// Reduced witness with renamed binders: a homomorphic map (`Wrap`) over an
/// object whose property is a variadic-rebuilt tuple (`Freeze`), read per index
/// in value position. Every literal index resolves to its own element.
#[test]
fn homomorphic_map_over_rebuilt_tuple_each_index_resolves() {
    assert!(no_access_errors(
        r#"
type Wrap<Box> = Box extends object ? { [Slot in keyof Box]: Wrap<Box[Slot]> } : Box;
type Freeze<Src> = Src extends readonly [infer Lead, ...infer Trail]
  ? readonly [Freeze<Lead>, ...Freeze<Trail>]
  : Src extends object ? { readonly [Pos in keyof Src]: Freeze<Src[Pos]> } : Src;
type Holder = { readonly cells: readonly [{ p: 1 }, { q: 2 }, { r: 3 }] };
declare const h: Wrap<Freeze<Holder>>;
const c0: 1 = h.cells[0].p;
const c1: 2 = h.cells[1].q;
const c2: 3 = h.cells[2].r;
"#
    ));
}

/// Negative control: a `number` index into the same composed tuple must still
/// yield the element *union*, not a single element — so reading a property that
/// only exists on one element is rejected.
#[test]
fn composed_tuple_number_index_stays_element_union() {
    let found = codes(
        r#"
type Wrap<Box> = Box extends object ? { [Slot in keyof Box]: Wrap<Box[Slot]> } : Box;
type Freeze<Src> = Src extends readonly [infer Lead, ...infer Trail]
  ? readonly [Freeze<Lead>, ...Freeze<Trail>]
  : Src extends object ? { readonly [Pos in keyof Src]: Freeze<Src[Pos]> } : Src;
type Holder = { readonly cells: readonly [{ p: 1 }, { q: 2 }, { r: 3 }] };
declare const h: Wrap<Freeze<Holder>>;
declare const n: number;
const u = h.cells[n];
const onlyP: { readonly p: 1 } = u;
"#,
    );
    assert!(
        found.contains(&2322),
        "number index must stay the element union (TS2322 expected): {found:?}"
    );
}

/// Length preservation: the composed receiver's tuple property keeps exactly
/// three elements (it is not widened to an array), so reading the whole tuple
/// observes a fixed-length tuple rather than `(elt)[]`.
#[test]
fn composed_tuple_keeps_fixed_length_three() {
    // A genuine array would assign to `readonly unknown[]` but never expose a
    // length-3 tuple shape. Assigning the tuple to a 2-tuple target must fail
    // (lengths differ), proving the third element is present and distinct.
    let found = codes(
        r#"
type Wrap<Box> = Box extends object ? { [Slot in keyof Box]: Wrap<Box[Slot]> } : Box;
type Freeze<Src> = Src extends readonly [infer Lead, ...infer Trail]
  ? readonly [Freeze<Lead>, ...Freeze<Trail>]
  : Src extends object ? { readonly [Pos in keyof Src]: Freeze<Src[Pos]> } : Src;
type Holder = { readonly cells: readonly [{ p: 1 }, { q: 2 }, { r: 3 }] };
declare const h: Wrap<Freeze<Holder>>;
const twoOnly: readonly [{ p: 1 }, { q: 2 }] = h.cells;
"#,
    );
    assert!(
        found.contains(&2322),
        "length-3 tuple must not assign to a 2-tuple target (TS2322 expected): {found:?}"
    );
}
