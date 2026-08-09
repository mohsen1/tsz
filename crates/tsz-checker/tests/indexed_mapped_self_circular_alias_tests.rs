//! Tests for TS2456 circular-type-alias detection through an *eager indexed
//! access* into a mapped or object-literal type.
//!
//! An indexed access `Obj[K]` is an eager projection: it resolves immediately
//! to the selected property's type. So an alias whose body indexes a mapped or
//! object type at a key whose value is the alias itself resolves directly back
//! to the alias — a direct self-cycle `tsc` reports as TS2456:
//!
//! ```ts
//! type Self = { [P in "x"]: Self }["x"]; // TS2456
//! type Self = { x: Self }["x"];          // TS2456 (object-literal twin)
//! ```
//!
//! Before the fix, the mapped-type form was a false negative: its body reduces
//! to the alias's own `Lazy` reference during resolution, so the resolved type
//! is not the bare type reference the direct-cycle check keyed on. The
//! object-literal twin was already caught because its resolved type stays an
//! indexed access whose object materializes to a concrete shape.
//!
//! The eagerness is confined to the indexed access: a structurally deferred
//! value position under the index — a function type, an array element — keeps
//! the self-reference deferred and is not circular, exactly as the un-indexed
//! recursive mapped/object type (`type Self = { [P in "x"]: Self }`) is not.
//! Binder names are varied so no name literal drives the logic.

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
fn indexed_mapped_self_is_circular() {
    // The reported repro: indexing a self-valued mapped type at a literal key.
    assert_ts2456(r#"type Self = { [P in "x"]: Self }["x"];"#);
}

#[test]
fn indexed_mapped_self_matches_object_literal_twin() {
    // Object-literal twin already reported TS2456; the mapped form must match.
    assert_ts2456(r#"type Self = { x: Self }["x"];"#);
    assert_ts2456(r#"type Self = { [P in "x"]: Self }["x"];"#);
}

#[test]
fn renamed_binder_indexed_mapped_self_is_circular() {
    // No dependence on a particular alias or key-literal name.
    assert_ts2456(r#"type Whatever = { [K in "field"]: Whatever }["field"];"#);
    assert_ts2456(r#"type SomethingElse = { [Q in "member"]: SomethingElse }["member"];"#);
}

#[test]
fn parenthesized_indexed_mapped_self_is_circular() {
    // The indexed access may be reached through parentheses.
    assert_ts2456(r#"type Self = ({ [P in "x"]: Self })["x"];"#);
    assert_ts2456(r#"type Self = ({ [P in "x"]: Self }["x"]);"#);
}

#[test]
fn indexed_mapped_union_key_self_is_circular() {
    // A finite union key set still resolves the indexed property to `Self`.
    assert_ts2456(r#"type Self = { [P in "x" | "y"]: Self }["x"];"#);
}

#[test]
fn nested_indexed_mapped_self_is_circular() {
    // Two levels of eager projection still reach the alias.
    assert_ts2456(r#"type Self = { [P in "x"]: { [Q in "y"]: Self } }["x"]["y"];"#);
}

#[test]
fn deferred_value_under_indexed_mapped_is_not_circular() {
    // A function type or array element in the value position defers the
    // self-reference — the indexed access resolves to a wrapper, not the alias.
    assert_no_ts2456(r#"type Self = { [P in "x"]: () => Self }["x"];"#);
    assert_no_ts2456(r#"type Self = { [P in "x"]: Self[] }["x"];"#);
    assert_no_ts2456(r#"type Self = { x: () => Self }["x"];"#);
}

#[test]
fn recursive_mapped_without_index_is_not_circular() {
    // Without the eager index, a recursive mapped/object type is legitimate
    // deferred recursion (the property position defers).
    assert_no_ts2456(r#"type Self = { [P in "x"]: Self };"#);
    assert_no_ts2456(r#"type Ring = { next: Ring; value: number };"#);
}

#[test]
fn indexing_unrelated_mapped_or_object_is_not_circular() {
    // Indexing a type that does not reference the alias is not a cycle.
    assert_no_ts2456(r#"type M = { [P in "x"]: number }; type V = M["x"];"#);
    assert_no_ts2456(r#"type Obj = { a: number; b: string }; type Val = Obj["a"];"#);
    assert_no_ts2456(r#"interface Shape { x: number } type G = Shape["x"];"#);
}
