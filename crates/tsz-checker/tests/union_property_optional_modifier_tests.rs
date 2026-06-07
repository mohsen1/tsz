//! Regression tests for issue #12174 (mapped key operations lose optional/
//! readonly modifier intent across project rows).
//!
//! Structural rule: `tsc`'s `createUnionOrIntersectionProperty` aggregates a
//! union member's modifiers with `optionalFlag |= prop.flags & Optional` and
//! marks the property readonly when it is readonly in any constituent — so a
//! union property is OPTIONAL/READONLY when ANY member is, while an intersection
//! property is only when EVERY declaring member is. tsz previously merged the
//! union's optionality with ALL-member semantics, so a non-distributing
//! homomorphic mapped type (`Pick<A | B, K>`) over a heterogeneous union — whose
//! members are not subtype-related, so the union is never collapsed by subtype
//! reduction — silently dropped the optional intent. These tests use a
//! locally-defined `Pick`-style alias (the test lib has no utility types) over
//! inline composite sources, plus direct union property access, to pin both
//! directions and the negative case.

use tsz_checker::test_utils::{check_source_diagnostics, diagnostic_count, has_diagnostic_code};
use tsz_common::diagnostics::Diagnostic;

/// Diagnostics with the bare-lib noise (2318 "no global `Object`", 2304 "cannot
/// find name") removed, leaving only the behavior under test.
fn relevant(source: &str) -> Vec<Diagnostic> {
    check_source_diagnostics(source)
        .into_iter()
        .filter(|d| !matches!(d.code, 2318 | 2304))
        .collect()
}

// ---------------------------------------------------------------------------
// Optional intent survives a `Pick`-style mapped type over a union
// ---------------------------------------------------------------------------

/// `a` is optional in one constituent, so the union property — and therefore a
/// homomorphic `{ [Q in K]: T[Q] }` picked over it — must keep it optional. The
/// members carry distinct discriminants so the union is not collapsed by subtype
/// reduction, which is what surfaced the bug.
#[test]
fn picked_property_over_union_keeps_optional_from_any_member() {
    let diags = relevant(
        r#"
        type MyPick<T, K extends keyof T> = { [Q in K]: T[Q] };
        type Picked = MyPick<{ a: number; tag: 1 } | { a?: number; tag: 2 }, "a">;

        const empty: Picked = {};                     // a optional -> ok
        declare const value: Picked;
        const widened: number | undefined = value.a;  // ok
    "#,
    );
    assert!(
        diags.is_empty(),
        "expected optional `a` to survive Pick over heterogeneous union, got: {diags:?}"
    );
}

/// Reading the preserved-optional property without allowing `undefined` must
/// surface the dropped-`undefined` as TS2322 (the witness from the issue).
#[test]
fn picked_property_over_union_read_includes_undefined() {
    let diags = relevant(
        r#"
        type MyPick<T, K extends keyof T> = { [Q in K]: T[Q] };
        type OnlyValue = MyPick<{ value: number; kind: "a" } | { value?: number; kind: "b" }, "value">;

        declare const field: OnlyValue;
        const exact: number = field.value; // number | undefined -> TS2322
    "#,
    );
    assert!(
        has_diagnostic_code(&diags, 2322),
        "expected TS2322 from reading union-optional property, got: {diags:?}"
    );
}

/// Negative control: when no constituent makes the property optional, the picked
/// property stays required and an empty object is rejected (TS2741). Guards
/// against over-correcting to "always optional".
#[test]
fn picked_property_over_union_required_when_no_member_optional() {
    let diags = relevant(
        r#"
        type MyPick<T, K extends keyof T> = { [Q in K]: T[Q] };
        type IdOnly = MyPick<{ id: number; side: "l" } | { id: number; side: "r" }, "id">;

        const missing: IdOnly = {}; // id required -> TS2741
    "#,
    );
    assert!(
        has_diagnostic_code(&diags, 2741),
        "expected TS2741 for required union property, got: {diags:?}"
    );
}

/// `readonly` in one constituent makes the union property readonly, and that
/// survives the picked mapped type.
#[test]
fn picked_property_over_union_keeps_readonly_from_any_member() {
    let diags = relevant(
        r#"
        type MyPick<T, K extends keyof T> = { [Q in K]: T[Q] };
        type Named = MyPick<{ name: string; v: 1 } | { readonly name: string; v: 2 }, "name">;

        declare const named: Named;
        named.name = "next"; // readonly -> TS2540
    "#,
    );
    assert_eq!(
        diagnostic_count(&diags, 2540),
        1,
        "expected one TS2540 for readonly union property, got: {diags:?}"
    );
}

// ---------------------------------------------------------------------------
// Intersection sources: optional only when every declaring member is optional
// ---------------------------------------------------------------------------

/// `a` is optional in the only constituent that declares it, so the picked
/// property over the intersection is optional.
#[test]
fn picked_property_over_intersection_keeps_optional_from_declaring_member() {
    let diags = relevant(
        r#"
        type MyPick<T, K extends keyof T> = { [Q in K]: T[Q] };
        type Picked = MyPick<{ a?: number; t: 1 } & { z: boolean }, "a">;

        const empty: Picked = {}; // a optional -> ok
    "#,
    );
    assert!(
        diags.is_empty(),
        "expected optional `a` to survive Pick over intersection, got: {diags:?}"
    );
}

// ---------------------------------------------------------------------------
// Direct union property access exercises the shared collection path
// ---------------------------------------------------------------------------

/// Plain property access on a heterogeneous union keeps the optional read type
/// `number | undefined`, exercising the union property collection directly.
#[test]
fn direct_union_property_access_includes_undefined() {
    let diags = relevant(
        r#"
        declare const node: { weight?: number; n: "o" } | { weight: number; n: "r" };
        const w: number = node.weight; // number | undefined -> TS2322
    "#,
    );
    assert!(
        has_diagnostic_code(&diags, 2322),
        "expected TS2322 for direct union property access, got: {diags:?}"
    );
}
