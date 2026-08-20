//! Regression tests for excess-property (TS2353) and property-access (TS2339)
//! checking against intersections whose members are *generic* mapped-type alias
//! applications with a non-identity `as` clause over a `Lazy(DefId)` source.
//!
//! Structural rule: when a homomorphic mapped type with a key-remapping `as`
//! clause (rename or filter) is instantiated with a concrete source declared as
//! a type alias/interface (i.e. `Lazy(DefId)`) and intersected with another
//! object, the mapped member's evaluated properties are *known* properties of
//! the intersection. tsc preserves the source `readonly`/optional modifiers and
//! the remapped key set through the mapped + intersection operations.
//!
//! tsz before fix: the checker's generic-mapped fallback enumerated key names
//! through the solver's environment-free finite-name queries, which cannot
//! resolve `Lazy(DefId)` references. For a non-identity `as` clause that returned
//! no names, so the member was wrongly reported as lacking every key — emitting a
//! spurious TS2353. The fix evaluates the concrete instantiation through the
//! checker's environment (which owns `DefId -> TypeId`) and answers membership
//! from the real object shape, only falling back to the syntactic heuristic for
//! genuinely generic receivers.
//!
//! Binder names are varied across cases so the fix is structural, not spelling
//! specific.

use crate::test_utils::check_source_diagnostics;

/// Filtering `as K extends KS ? K : never` alias intersected with another object:
/// assigning all kept keys plus the intersection key must be clean.
#[test]
fn filter_as_clause_alias_intersection_assignment_is_clean() {
    let diags = check_source_diagnostics(
        r#"
type Source = { readonly a?: number; b: string; c?: boolean };
type Pick2<T, KS extends keyof T> = { [K in keyof T as K extends KS ? K : never]: T[K] };
type R = Pick2<Source, "a" | "b"> & { d: number };
const y: R = { a: 1, b: "z", d: 1 };
const y2: R = { b: "z", d: 1 };
"#,
    );
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    assert!(
        codes.is_empty(),
        "filtered mapped-alias intersection must accept kept + intersection keys; got: {codes:?}"
    );
}

/// Renamed `as` clause (template literal) alias intersected with another object.
/// Renamed binder names (`Box`/`Elem`/`Prefix`) prove the rule is structural.
#[test]
fn rename_as_clause_alias_intersection_assignment_is_clean() {
    let diags = check_source_diagnostics(
        r#"
type Box = { readonly a?: number; b: string; c?: boolean };
type Rename<Elem, Prefix extends string> = { [K in keyof Elem as `${Prefix}_${string & K}`]: Elem[K] };
type R = Rename<Box, "x"> & { d: number };
const y: R = { x_a: 1, x_b: "z", x_c: true, d: 1 };
"#,
    );
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    assert!(
        codes.is_empty(),
        "renamed mapped-alias intersection must accept renamed + intersection keys; got: {codes:?}"
    );
}

/// Single-parameter filtering alias (no second type parameter) still recognizes
/// its evaluated keys inside an intersection.
#[test]
fn single_param_filter_alias_intersection_is_clean() {
    let diags = check_source_diagnostics(
        r#"
type Source = { readonly a?: number; b: string; c?: boolean };
type KeepAB<T> = { [K in keyof T as K extends "a" | "b" ? K : never]: T[K] };
type R = KeepAB<Source> & { d: number };
const y: R = { a: 1, b: "z", d: 1 };
"#,
    );
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    assert!(
        codes.is_empty(),
        "single-param filter mapped-alias intersection must be clean; got: {codes:?}"
    );
}

/// A key genuinely filtered out by the `as` clause is still excess (TS2353):
/// the fix must not over-accept. `c` is dropped by the filter.
#[test]
fn filtered_out_key_is_still_excess() {
    let diags = check_source_diagnostics(
        r#"
type Source = { readonly a?: number; b: string; c?: boolean };
type KeepAB<T> = { [K in keyof T as K extends "a" | "b" ? K : never]: T[K] };
type R = KeepAB<Source> & { d: number };
const y: R = { b: "z", c: true, d: 1 };
"#,
    );
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2353),
        "a key removed by the `as` filter must still be reported as excess; got: {codes:?}"
    );
}

/// Property access (TS2339) on a renamed mapped-alias receiver: renamed keys
/// exist; the original (pre-rename) key does not.
#[test]
fn rename_as_clause_property_access_renames_keys() {
    let diags = check_source_diagnostics(
        r#"
type Source = { readonly a?: number; b: string };
type Rename<T> = { [K in keyof T as `x_${string & K}`]: T[K] };
type R = Rename<Source>;
declare const r: R;
r.x_a;
r.x_b;
r.a;
"#,
    );
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    assert!(
        codes == vec![2339],
        "only the pre-rename key `a` must be TS2339 on a renamed mapped receiver; got: {codes:?}"
    );
}

/// The `readonly` modifier survives a filtering `as` clause + intersection:
/// writing to the preserved-readonly property is TS2540.
#[test]
fn readonly_modifier_preserved_through_filter_as_and_intersection() {
    let diags = check_source_diagnostics(
        r#"
type Source = { readonly a?: number; b: string; c?: boolean };
type Pick2<T, KS extends keyof T> = { [K in keyof T as K extends KS ? K : never]: T[K] };
type R = Pick2<Source, "a" | "b"> & { d: number };
declare const r: R;
r.a = 1;
r.d = 2;
"#,
    );
    let codes: Vec<u32> = diags.iter().map(|d| d.code).collect();
    assert!(
        codes == vec![2540],
        "readonly source modifier must survive filter `as` + intersection (only `a` write is TS2540); got: {codes:?}"
    );
}
