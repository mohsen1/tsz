//! Tests for TS2456 circular-type-alias detection through a `keyof` operand.
//!
//! `keyof T` forces `T`'s apparent type, so a self-reference reached through
//! `keyof` re-enters the alias mid-resolution and is circular — `tsc` reports
//! TS2456 for `type A = keyof A`, `keyof A[]`, `keyof Array<A>`,
//! `keyof (x | A)`, and `keyof A["k"]`, and for alias hops that route back
//! through such a `keyof`. The eagerness is confined to `keyof`: a deferred
//! array element without `keyof` (`type N = N[]`, `readonly N[]`) and an
//! object-literal property under `keyof` (`type A = keyof { a: A }`) stay
//! deferred and are not circular. These tests lock that behavior to match
//! `tsc` 6.0.2, including renamed binders so no name-literal drives the logic.

use tsz_checker::test_utils::check_source_codes as get_error_codes;

fn assert_ts2456(src: &str) {
    let codes = get_error_codes(src);
    assert!(
        codes.contains(&2456),
        "Expected TS2456 (circularly references itself) for:\n{src}\ngot: {codes:?}"
    );
}

fn assert_no_ts2456(src: &str) {
    let codes = get_error_codes(src);
    assert!(
        !codes.contains(&2456),
        "Expected no TS2456 for:\n{src}\ngot: {codes:?}"
    );
}

#[test]
fn direct_keyof_self_is_circular() {
    assert_ts2456("type A = keyof A;");
}

#[test]
fn renamed_binder_keyof_self_is_circular() {
    // No dependence on a particular alias name.
    assert_ts2456("type Whatever = keyof Whatever;");
    assert_ts2456("type SomethingElse = keyof SomethingElse;");
}

#[test]
fn nested_keyof_self_is_circular() {
    assert_ts2456("type A = keyof keyof A;");
}

#[test]
fn keyof_self_array_is_circular() {
    // `keyof` forces the array's apparent type, resolving the element `A`.
    assert_ts2456("type A = keyof A[];");
}

#[test]
fn keyof_generic_ref_to_self_is_circular() {
    assert_ts2456("type A = keyof Array<A>;");
}

#[test]
fn keyof_union_with_self_is_circular() {
    assert_ts2456("type A = keyof (string | A);");
}

#[test]
fn keyof_indexed_self_is_circular() {
    assert_ts2456("type A = keyof A[\"x\"];");
}

#[test]
fn keyof_self_via_alias_hop_is_circular() {
    // `E` reaches itself only through `F`'s `keyof E`.
    assert_ts2456("type E = keyof F; type F = E;");
    assert_ts2456("type First = keyof Second; type Second = First;");
}

#[test]
fn keyof_over_object_property_self_is_not_circular() {
    // `keyof { a: A }` resolves to `"a"` without resolving the property `A`,
    // so the object-property position stays deferred.
    assert_no_ts2456("type A = keyof { a: A };");
    assert_no_ts2456("type Ring = keyof { next: Ring; value: number };");
}

#[test]
fn plain_self_array_without_keyof_is_not_circular() {
    // Bare `N[]`/`readonly N[]` keep the element deferred — no `keyof` to force it.
    assert_no_ts2456("type N = N[];");
    assert_no_ts2456("type O = readonly O[];");
}

#[test]
fn keyof_over_unrelated_type_is_not_circular() {
    assert_no_ts2456("type Keys = keyof { a: number; b: string };");
    assert_no_ts2456("interface Shape { x: number; y: number } type K = keyof Shape;");
}

#[test]
fn generic_keyof_mapped_recursion_is_not_circular() {
    // A homomorphic mapped type over `keyof T` is legitimate deferred recursion.
    assert_no_ts2456("type Deep<T> = { [K in keyof T]: Deep<T[K]> };");
    assert_no_ts2456("type PropKeys<T> = { [K in keyof T]: K }[keyof T];");
}
