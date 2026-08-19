//! Regression tests for #14791: a mapped type whose key-remapping `as` clause
//! collapses every literal source key to the *bare* `string`/`number` primitive
//! must lower to a string/number index signature, not to named properties with
//! a degenerate value type.
//!
//! Structural rule: when `{ [K in keyof S as string]: S[K] }` (or `as number`)
//! produces a bare-primitive remapped key, tsc synthesizes an index signature
//! `{ [x: string]: V }` whose value `V` unions every contributing source key's
//! value type. tsz now does the same in the solver mapped-type evaluator, so
//! object-literal assignment routes through index-signature assignability
//! (no spurious `TS2322`) and excess-property checking is suppressed
//! (no spurious `TS2353`).
//!
//! Binder names are varied across cases so the fix cannot depend on any
//! identifier, alias, or property literal.

use tsz_checker::test_utils::check_source_codes;

fn check(source: &str) -> Vec<u32> {
    check_source_codes(source)
}

fn count(codes: &[u32], code: u32) -> usize {
    codes.iter().filter(|&&c| c == code).count()
}

/// `as string` over named-key source: object literal with arbitrary keys must
/// be accepted through the synthesized string index signature.
#[test]
fn as_string_remap_synthesizes_string_index_no_false_positive() {
    let codes = check(
        r#"
type ToStringIndex<S> = { [K in keyof S as string]: S[K] };
type R = ToStringIndex<{ a: number; b: string }>;
const r: R = { x: 1, y: "hello" };
export {};
"#,
    );
    assert_eq!(
        count(&codes, 2322),
        0,
        "no TS2322: value assigns through the string index signature: {codes:?}"
    );
    assert_eq!(
        count(&codes, 2353),
        0,
        "no TS2353: an index-signature target suppresses excess-property checks: {codes:?}"
    );
}

/// `as number` over a renamed binder/source: produces a numeric index signature.
#[test]
fn as_number_remap_synthesizes_number_index_no_false_positive() {
    let codes = check(
        r#"
type ToNumIndex<Src> = { [Prop in keyof Src as number]: Src[Prop] };
type Out = ToNumIndex<{ first: number; second: number }>;
const out: Out = { 0: 1, 7: 42 };
export {};
"#,
    );
    assert_eq!(
        count(&codes, 2322),
        0,
        "no TS2322 for numeric index: {codes:?}"
    );
    assert_eq!(
        count(&codes, 2353),
        0,
        "no TS2353 for numeric index: {codes:?}"
    );
}

/// The synthesized string index value type is the union of source value types,
/// so a wrongly-typed entry still errors (the fix must not become a blanket
/// `any` index). `boolean` is not assignable to `number | string`.
#[test]
fn as_string_remap_index_value_is_union_and_still_rejects_bad_value() {
    let codes = check(
        r#"
type StrIndex<T> = { [P in keyof T as string]: T[P] };
type Rec = StrIndex<{ a: number; b: string }>;
const bad: Rec = { whatever: true };
export {};
"#,
    );
    assert_eq!(
        count(&codes, 2322),
        1,
        "exactly one TS2322: `true` is not in the index value `number | string`: {codes:?}"
    );
}

/// Mixed remap: some keys stay literal (conditionally remapped to themselves),
/// some collapse to `string`. The literal `keep` property's value type
/// (`number`) must itself satisfy the synthesized string index signature's
/// value type (`string`, contributed only by the `drop` arm) — it does not,
/// so `tsc` reports the mapped type's own property/index-signature conflict
/// as a `TS2322` on the *assignment*, oracle-verified against
/// `/opt/node22/bin/tsc` (6.0.2, `--strict`):
/// `Property 'keep' is incompatible with index signature. Type 'number' is
/// not assignable to type 'string'.` This is not the false positive #14791
/// guards against — it is a genuine incompatibility between the mapped
/// type's literal member and its own synthesized index signature.
#[test]
fn mixed_literal_and_primitive_remap() {
    let codes = check(
        r#"
type Mix<S> = { [K in keyof S as K extends "keep" ? K : string]: S[K] };
type M = Mix<{ keep: number; drop: string }>;
const m: M = { keep: 5, anything: "x" };
export {};
"#,
    );
    assert_eq!(
        count(&codes, 2322),
        1,
        "keep's value type (number) is incompatible with the synthesized \
         string index signature's value type (string): {codes:?}"
    );
}

/// Conditional `as` that always widens to `string` is still a bare-primitive
/// remap and must synthesize the string index signature.
#[test]
fn conditional_as_widening_to_string() {
    let codes = check(
        r#"
type Widen<Source> = { [Key in keyof Source as Key extends never ? never : string]: Source[Key] };
type W = Widen<{ alpha: number }>;
const w: W = { anyName: 1 };
export {};
"#,
    );
    assert_eq!(
        count(&codes, 2322),
        0,
        "conditional widen assigns: {codes:?}"
    );
    assert_eq!(
        count(&codes, 2353),
        0,
        "conditional widen no excess: {codes:?}"
    );
}

/// `keyof` of the remapped result must include the index key space
/// (`string | number` for a string index), so it is NOT assignable to `string`.
/// This guards the opposite-direction unsoundness called out in #14791.
#[test]
fn keyof_remapped_string_index_is_not_assignable_to_string() {
    let codes = check(
        r#"
type ToIdx<S> = { [K in keyof S as string]: S[K] };
type Keys = keyof ToIdx<{ a: number }>;
declare const k: Keys;
const s: string = k;
export {};
"#,
    );
    assert_eq!(
        count(&codes, 2322),
        1,
        "keyof is `string | number`, not assignable to `string`: {codes:?}"
    );
}

/// Scoping control: `as symbol` is a non-property-name primitive, NOT a
/// string/number index. The fix must stay scoped to `string`/`number` and must
/// not synthesize a permissive string index from a `symbol` remap. If it did,
/// an arbitrary string-keyed object literal would be wrongly accepted; instead
/// it must still be rejected (proving the symbol path is untouched).
#[test]
fn as_symbol_remap_not_lowered_to_string_index() {
    let codes = check(
        r#"
type ToSym<S> = { [K in keyof S as symbol]: S[K] };
type Sy = ToSym<{ a: number }>;
const sy: Sy = { arbitrary: 1 };
export {};
"#,
    );
    assert!(
        count(&codes, 2322) + count(&codes, 2353) >= 1,
        "`as symbol` must NOT synthesize a string index that accepts arbitrary keys: {codes:?}"
    );
}
