//! Tests for TS2456 circular-type-alias detection through an indexed access
//! into a self-referencing mapped type (`{ [P in K]: X }[Key]`).
//!
//! Indexing a mapped type at a literal key forces eager resolution of that
//! one property, the same "eager position" `keyof` already gets — unlike an
//! ordinary deferred object or mapped-type property access. Oracle-verified
//! against `typescript@7.0.2`: `type Rec = { [P in "x"]: Rec }` (no indexing)
//! is accepted as legitimate deferred recursion (a linked-list shape), but
//! `type Rec = { [P in "x"]: Rec }["x"]` is `TS2456`. These tests lock that
//! boundary, including renamed binders and adjacent non-circular shapes, so
//! no name-literal drives the logic.

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
fn indexed_mapped_type_self_access_is_circular() {
    assert_ts2456(r#"type Self = { [P in "x"]: Self }["x"]; declare const s: Self;"#);
}

#[test]
fn renamed_binder_indexed_mapped_type_self_access_is_circular() {
    // No dependence on a particular alias name.
    assert_ts2456(r#"type Whatever = { [K in "a"]: Whatever }["a"];"#);
    assert_ts2456(r#"type SomethingElse = { [Q in "z"]: SomethingElse }["z"];"#);
}

#[test]
fn indexed_mapped_type_self_access_with_number_literal_key_is_circular() {
    assert_ts2456("type Bar = { [K in 1]: Bar }[1];");
}

#[test]
fn indexed_mapped_type_self_access_with_multi_key_constraint_is_circular() {
    assert_ts2456(r#"type Rec = { [P in "x" | "y"]: Rec }["x"]; declare const r: Rec;"#);
}

#[test]
fn indexed_mapped_type_self_access_via_keyof_full_domain_is_circular() {
    assert_ts2456(r#"type All = { [K in "a"]: All }[keyof { [K in "a"]: All }];"#);
}

#[test]
fn bare_mapped_type_self_reference_without_indexing_is_not_circular() {
    // Legitimate deferred recursion (a linked-list shape) — no top-level
    // indexed access to force eager resolution.
    assert_no_ts2456(r#"type Rec = { [P in "x"]: Rec };"#);
}

#[test]
fn plain_object_property_self_reference_is_not_circular() {
    assert_no_ts2456("type Rec = { next: Rec };");
}

#[test]
fn indexed_mapped_type_non_self_value_is_not_circular() {
    assert_no_ts2456(r#"type Baz = { [K in "a"]: number }["a"]; declare const b: Baz;"#);
}
