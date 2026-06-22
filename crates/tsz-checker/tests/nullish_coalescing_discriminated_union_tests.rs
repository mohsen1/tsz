//! Checker result-type tests for `a ?? b` where the receiver is a
//! discriminated union and both operands are discriminant property accesses.
//!
//! Structural rule:
//!
//! > The right operand of `a ?? b` is checked on the control-flow path where
//! > `a` is *nullish* (tsc's `narrowTypeByOptionality`, not
//! > `narrowTypeByTruthiness`). When `a` is a discriminant property access on a
//! > union `r`, that nullish fact discriminates `r` to the member(s) whose
//! > discriminant property can be `undefined`/`null`, so `b` (another property
//! > of `r`) is narrowed accordingly.
//!
//! Witness: tanstack-router `new-process-route-tree.ts` has
//! `{ fullPath: string; from?: never } | { fullPath?: never; from: string }`
//! and writes `r.fullPath ?? r.from` into a `string`. Before the fix, the `??`
//! right operand stayed `string | undefined` (the union was filtered by
//! *falsiness*, keeping both members), leaking `undefined` into the result and
//! raising a false TS2322. tsc narrows the right operand to `string`.
//!
//! Binder names below deliberately differ from the witness to keep the test
//! free of identifier/file-name coupling.

use crate::test_utils::check_source_strict_codes as check_strict;

/// The reported witness shape: `r.head ?? r.tail` over a two-branch
/// discriminated union whose discriminant is an optional `never` property.
/// The result is `string`, assignable to `string` (no TS2322).
#[test]
fn discriminated_union_coalesce_property_access_yields_non_nullish() {
    let source = r#"
declare const rec:
    | { head: string; tail?: never }
    | { head?: never; tail: string };
const out: string = rec.head ?? rec.tail;
"#;
    let codes = check_strict(source);
    assert!(
        !codes.contains(&2322),
        "`rec.head ?? rec.tail` over a discriminated union must be `string`, got: {codes:?}"
    );
}

/// Element-access form narrows identically to property-access form.
#[test]
fn discriminated_union_coalesce_element_access_yields_non_nullish() {
    let source = r#"
declare const rec:
    | { head: string; tail?: never }
    | { head?: never; tail: string };
const out: string = rec["head"] ?? rec["tail"];
"#;
    let codes = check_strict(source);
    assert!(
        !codes.contains(&2322),
        "`rec[\"head\"] ?? rec[\"tail\"]` must be `string`, got: {codes:?}"
    );
}

/// Two operands suffice to collapse: the right operand `rec.tail` of a single
/// `??` is re-narrowed through the discriminant, matching tsc. A three-branch
/// *chain* (`a ?? b ?? c`) does NOT further collapse in tsc — the intermediate
/// `a ?? b` result is no longer a discriminant access on `rec`, so `?? c`
/// cannot re-narrow `rec`; tsc leaves `string | undefined` there. This test
/// pins that parity: the chain keeps `undefined`, so assigning to `string`
/// raises TS2322 in both tsc and tsz (the fix must not over-eagerly collapse
/// chains).
#[test]
fn discriminated_union_coalesce_chain_matches_tsc_keeps_undefined() {
    let source = r#"
declare const rec:
    | { a: string; b?: never; c?: never }
    | { a?: never; b: string; c?: never }
    | { a?: never; b?: never; c: string };
const out: string = rec.a ?? rec.b ?? rec.c;
"#;
    let codes = check_strict(source);
    assert!(
        codes.contains(&2322),
        "`rec.a ?? rec.b ?? rec.c` stays `string | undefined` in tsc, expected TS2322, got: {codes:?}"
    );
}

/// Negative guard: when the left discriminant value genuinely keeps a
/// non-nullish union member (a `string | number` value, not a falsy-only one),
/// the `??` result must still include that member — the fix must not over-strip.
/// `m.v ?? m.w` is `string | number | boolean`, which assigns to that target
/// (no TS2322) but does NOT assign to `string` (a TS2322 we assert is present).
#[test]
fn discriminated_union_coalesce_keeps_genuine_union_member() {
    let ok = r#"
declare const m:
    | { v: string | number; w?: never }
    | { v?: never; w: boolean };
const out: string | number | boolean = m.v ?? m.w;
"#;
    let codes_ok = check_strict(ok);
    assert!(
        !codes_ok.contains(&2322),
        "`m.v ?? m.w` must be `string | number | boolean` (assignable), got: {codes_ok:?}"
    );

    let bad = r#"
declare const m:
    | { v: string | number; w?: never }
    | { v?: never; w: boolean };
const out: string = m.v ?? m.w;
"#;
    let codes_bad = check_strict(bad);
    assert!(
        codes_bad.contains(&2322),
        "`m.v ?? m.w` keeps `number | boolean`, so assigning to `string` must raise TS2322, got: {codes_bad:?}"
    );
}

/// Negative guard: `||` (logical OR, not `??`) is unchanged. Because `""` is a
/// non-nullish *falsy* `string`, `t.val || t.alt` keeps both members and is
/// `string | undefined`, raising TS2322 against `string` in tsc as well. The
/// fix must only affect `??`, never `||`.
#[test]
fn discriminated_union_logical_or_unchanged() {
    let source = r#"
declare const t:
    | { val: string; alt?: never }
    | { val?: never; alt: string };
const out: string = t.val || t.alt;
"#;
    let codes = check_strict(source);
    assert!(
        codes.contains(&2322),
        "`t.val || t.alt` stays `string | undefined` (matching tsc), expected TS2322, got: {codes:?}"
    );
}
