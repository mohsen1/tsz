//! Regression tests for indexed access over an intersection inside
//! mapped-type instantiation (issue #15676).
//!
//! When a mapped type `{ [P in K]: T[P] }` instantiates with `T` bound to an
//! intersection of *named* (lazy) object types, the instantiation-time eager
//! evaluation runs without a resolver. Each intersection member's access came
//! back as its own deferred `member[P]`, and the distribution loop intersected
//! them: `(A & B)[P]` became `A[P] & B[P]`. A later resolver-backed pass then
//! evaluated each piece independently, so a key missing from one member
//! resolved to `undefined` alone and collapsed the whole property type to
//! `never` (false TS2322).
//!
//! tsc resolves `(A & B)[P]` against the merged property set — constituents
//! lacking the key are skipped, never intersected in as `undefined`. tsz does
//! this through the solver's intersection index-access distribution, which now
//! defers the WHOLE access when any member is an unresolved semantic ref.
//!
//! Binder names are varied across cases so the fix cannot rely on any
//! user-chosen identifier.

use crate::test_utils::check_source_strict_codes as check_strict;

const TS2322: u32 = 2322;
const TS2344: u32 = 2344;
const TS2741: u32 = 2741;

fn assert_clean(source: &str, label: &str) {
    let codes = check_strict(source);
    assert!(
        codes.is_empty(),
        "{label}: expected no diagnostics (tsc accepts), got codes: {codes:?}"
    );
}

fn assert_has(source: &str, code: u32, label: &str) {
    let codes = check_strict(source);
    assert!(
        codes.contains(&code),
        "{label}: expected TS{code}, got codes: {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// MUST BE CLEAN — tsc accepts all of these
// ---------------------------------------------------------------------------

#[test]
fn union_key_spanning_both_intersection_members() {
    // The primary witness from #15676.
    let source = r#"
type A = { a: 1; c: 3 };
type B = { b: 2; c: 3 };
type Pick2<T, K extends keyof T> = { [P in K]: T[P] };
type P2 = Pick2<A & B, "a" | "b">;
const p2: P2 = { a: 1, b: 2 };
"#;
    assert_clean(source, "union key spanning both members");
}

#[test]
fn single_key_missing_from_one_member() {
    let source = r#"
type Left = { first: string };
type Right = { second: number };
type Grab<Src, Keys extends keyof Src> = { [Prop in Keys]: Src[Prop] };
type OnlyFirst = Grab<Left & Right, "first">;
const value: OnlyFirst = { first: "ok" };
"#;
    assert_clean(source, "single key missing from one member");
}

#[test]
fn key_present_on_both_members() {
    let source = r#"
type M1 = { shared: boolean; only1: string };
type M2 = { shared: boolean; only2: number };
type Sel<O, Ks extends keyof O> = { [Q in Ks]: O[Q] };
type Shared = Sel<M1 & M2, "shared">;
const s: Shared = { shared: true };
"#;
    assert_clean(source, "key present on both members");
}

#[test]
fn keyof_intersection_full_key_set() {
    let source = r#"
type U = { u: "u" };
type V = { v: "v" };
type Copy<T, K extends keyof T> = { [P in K]: T[P] };
type All = Copy<U & V, keyof (U & V)>;
const all: All = { u: "u", v: "v" };
"#;
    assert_clean(source, "keyof intersection full key set");
}

#[test]
fn intersection_behind_alias_wrapper() {
    let source = r#"
type Head = { head: 0 };
type Tail = { tail: 1 };
type Both = Head & Tail;
type Take<T, K extends keyof T> = { [P in K]: T[P] };
type Picked = Take<Both, "head" | "tail">;
const picked: Picked = { head: 0, tail: 1 };
"#;
    assert_clean(source, "intersection behind alias wrapper");
}

#[test]
fn interface_members_instead_of_aliases() {
    let source = r#"
interface Person { name: string }
interface Aged { age: number }
type Fields<T, K extends keyof T> = { [F in K]: T[F] };
type NameOnly = Fields<Person & Aged, "name" | "age">;
const n: NameOnly = { name: "x", age: 3 };
"#;
    assert_clean(source, "interface intersection members");
}

#[test]
fn three_intersection_members() {
    let source = r#"
type X1 = { x1: "x1" };
type X2 = { x2: "x2" };
type X3 = { x3: "x3" };
type Choose<T, K extends keyof T> = { [P in K]: T[P] };
type Two = Choose<X1 & X2 & X3, "x1" | "x3">;
const two: Two = { x1: "x1", x3: "x3" };
"#;
    assert_clean(source, "three intersection members");
}

#[test]
fn mixed_inline_and_named_members() {
    let source = r#"
type Named = { named: true };
type Mix<T, K extends keyof T> = { [P in K]: T[P] };
type Mixed = Mix<{ inline: 1 } & Named, "inline" | "named">;
const m: Mixed = { inline: 1, named: true };
"#;
    assert_clean(source, "mixed inline and named members");
}

#[test]
fn optional_modifier_preserved_over_intersection() {
    let source = r#"
type Opt1 = { alpha?: string };
type Opt2 = { beta: number };
type Loose<T, K extends keyof T> = { [P in K]?: T[P] };
type Part = Loose<Opt1 & Opt2, "alpha" | "beta">;
const p: Part = { beta: 4 };
"#;
    assert_clean(source, "optional modifier over intersection");
}

#[test]
fn readonly_modifier_over_intersection() {
    let source = r#"
type R1 = { one: 1 };
type R2 = { two: 2 };
type Frozen<T, K extends keyof T> = { readonly [P in K]: T[P] };
type F = Frozen<R1 & R2, "one" | "two">;
const f: F = { one: 1, two: 2 };
"#;
    assert_clean(source, "readonly modifier over intersection");
}

#[test]
fn generic_application_member_in_intersection() {
    let source = r#"
type Boxed<V> = { boxed: V };
type Plain = { plain: string };
type Read<T, K extends keyof T> = { [P in K]: T[P] };
type Out = Read<Boxed<number> & Plain, "boxed" | "plain">;
const o: Out = { boxed: 1, plain: "p" };
"#;
    assert_clean(source, "generic application member");
}

#[test]
fn nested_pick_of_pick_over_intersection() {
    let source = r#"
type NA = { na: "na"; nc: "nc" };
type NB = { nb: "nb"; nc: "nc" };
type P<T, K extends keyof T> = { [Q in K]: T[Q] };
type Outer = P<P<NA & NB, "na" | "nb" | "nc">, "na" | "nb">;
const outer: Outer = { na: "na", nb: "nb" };
"#;
    assert_clean(source, "nested pick of pick");
}

#[test]
fn value_read_through_picked_property() {
    let source = r#"
type S1 = { width: number };
type S2 = { height: number };
type Dim<T, K extends keyof T> = { [P in K]: T[P] };
declare const d: Dim<S1 & S2, "width" | "height">;
const w: number = d.width;
const h: number = d.height;
"#;
    assert_clean(source, "value read through picked property");
}

// ---------------------------------------------------------------------------
// MUST STILL ERROR — the fix must not widen away real mismatches
// ---------------------------------------------------------------------------

#[test]
fn wrong_value_type_still_errors() {
    let source = r#"
type WA = { wa: 1 };
type WB = { wb: 2 };
type Grab2<T, K extends keyof T> = { [P in K]: T[P] };
type W = Grab2<WA & WB, "wa" | "wb">;
const w: W = { wa: "wrong", wb: 2 };
"#;
    assert_has(source, TS2322, "wrong value type for picked key");
}

#[test]
fn key_on_neither_member_still_errors() {
    let source = r#"
type KA = { ka: 1 };
type KB = { kb: 2 };
type Grab3<T, K extends keyof T> = { [P in K]: T[P] };
type Bad = Grab3<KA & KB, "missing">;
"#;
    assert_has(source, TS2344, "key on neither member");
}

#[test]
fn missing_required_property_still_errors() {
    let source = r#"
type QA = { qa: 1 };
type QB = { qb: 2 };
type Grab4<T, K extends keyof T> = { [P in K]: T[P] };
type Q = Grab4<QA & QB, "qa" | "qb">;
const q: Q = { qa: 1 };
"#;
    assert_has(source, TS2741, "missing required picked property");
}
