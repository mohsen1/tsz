//! Tuple discriminated unions share the object detector's whole-literal
//! discriminant gate. A nullable element (`string | undefined`) must not
//! masquerade as a discriminant, or the tuple discriminated-union relation
//! distributes a wider source per-constituent across arms and wrongly accepts a
//! strictly wider tuple (a missing-`TS2322` false-negative).
//!
//! Regression for #17643's tuple surface: the object-member detector was fixed
//! (#17650) but the structurally identical tuple-element detector
//! (`type_related_to_discriminated_tuple_type`) kept the "any constituent is
//! unit-like" idiom. Both now route through the shared `whole_type_is_unit_like`
//! gate, so they cannot drift. A couple of object cases guard that the shared gate
//! keeps the object behavior it was extracted from.

use tsz_checker::test_utils::check_source_strict_codes;

fn assert_ts2322(source: &str, context: &str) {
    let codes = check_source_strict_codes(source);
    assert!(
        codes.contains(&2322),
        "{context}: expected TS2322, got {codes:?}"
    );
}

fn assert_no_ts2322(source: &str, context: &str) {
    let codes = check_source_strict_codes(source);
    assert!(
        !codes.contains(&2322),
        "{context}: expected no TS2322, got {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// Tuple: a wider source must be rejected against arms with a nullable element.
// ---------------------------------------------------------------------------

#[test]
fn wider_tuple_source_rejected_against_undefined_element_arms() {
    assert_ts2322(
        r#"
declare const wide: [string | number];
const out: [string | undefined] | [number | undefined] = wide;
"#,
        "tuple source vs union of `[T | undefined]` arms",
    );
}

#[test]
fn wider_tuple_source_rejected_against_null_element_arms() {
    assert_ts2322(
        r#"
declare const wide: [string | number, boolean];
const out: [string | null, boolean] | [number | null, boolean] = wide;
"#,
        "tuple source vs union of `[T | null, ..]` arms",
    );
}

#[test]
fn renamed_binders_tuple_still_rejected() {
    // Same shape, different identifiers — the fix must be structural.
    assert_ts2322(
        r#"
declare const pair: [1 | 2 | 3 | undefined];
const sink: [1 | undefined] | [2 | undefined] = pair;
"#,
        "renamed-binder tuple with an uncovered wide element",
    );
}

// ---------------------------------------------------------------------------
// Tuple: genuine all-unit discriminants must keep distributing (match tsc).
// ---------------------------------------------------------------------------

#[test]
fn genuine_tuple_literal_discriminant_still_distributes() {
    assert_no_ts2322(
        r#"
declare const wide: ["a" | "b", number];
const out: ["a", number] | ["b", number] = wide;
"#,
        "all-unit tuple discriminant distributes across arms",
    );
}

#[test]
fn genuine_tuple_literal_plus_undefined_discriminant_still_distributes() {
    // Every constituent (`1`, `2`, `undefined`) is a unit type, so the position
    // *is* a discriminant and distribution across the arms is sound.
    assert_no_ts2322(
        r#"
declare const wide: [1 | 2 | undefined, number];
const out: [1 | undefined, number] | [2 | undefined, number] = wide;
"#,
        "all-unit `literal | undefined` tuple discriminant still distributes",
    );
}

#[test]
fn tuple_source_that_is_one_arm_stays_assignable() {
    assert_no_ts2322(
        r#"
declare const narrow: [string | undefined];
const out: [string | undefined] | [number | undefined] = narrow;
"#,
        "tuple source equal to one arm",
    );
}

// ---------------------------------------------------------------------------
// Object cases guarding the shared gate `whole_type_is_unit_like` (the object
// detector was rewritten to call it; behavior must be unchanged).
// ---------------------------------------------------------------------------

#[test]
fn object_nullable_member_still_rejected_via_shared_gate() {
    assert_ts2322(
        r#"
declare const wide: { v: string | number };
const out: { v: string | undefined } | { v: number | undefined } = wide;
"#,
        "object arms `T | undefined` (shared gate)",
    );
}

#[test]
fn object_all_unit_discriminant_still_distributes_via_shared_gate() {
    assert_no_ts2322(
        r#"
type Shape = { kind: "circle"; r: number } | { kind: "square"; s: number };
declare const wide: { kind: "circle" | "square"; r: number; s: number };
const out: Shape = wide;
"#,
        "object literal discriminant distributes (shared gate)",
    );
}
