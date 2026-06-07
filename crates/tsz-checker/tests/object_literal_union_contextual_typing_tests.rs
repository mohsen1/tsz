//! Tests for contextual typing of object literals against union-of-objects targets.
//!
//! When the declared type is a union whose members share a property at different
//! literal-vs-widened precision (e.g. `{ k: 2 } | { k: number; j: boolean }`), a
//! fresh object literal on the right-hand side must be contextually typed
//! per-property: property `k` sees `2 | number` as its contextual type, which
//! preserves the number literal `2` instead of widening it to `number`. Without
//! literal preservation the inferred `{ k: number }` matches neither arm (the
//! literal arm by value, the wider arm by arity) and tsz emitted a spurious
//! TS2322.
//!
//! Root cause: `contextual_object_literal_property_type` already builds the
//! per-property contextual type with `union_preserve_members` (`2 | number`),
//! mirroring tsc's `getTypeOfPropertyOfContextualType` under
//! `UnionReduction.None`. But `prefer_more_specific_contextual_property_type`
//! then collapsed that union back to its widened member `number` via the
//! "union contains the candidate" rule, dropping the literal arm.
//!
//! Fix: only collapse a union to a contained member when the member is a
//! *strict* subset. When the union reduces to exactly that member as a set
//! (`2 | number` denotes the same values as `number`), the un-reduced union is
//! the literal-preserving form and is kept.
//!
//! This is the object-literal counterpart of
//! `tuple_union_contextual_typing_tests.rs`.

use tsz_checker::test_utils::check_source_codes;

// ---------------------------------------------------------------------------
// Core cases - differing-arity object unions, literal arm preserved
// ---------------------------------------------------------------------------

/// Assigning `{ k: 2 }` to `{ k: 2 } | { k: number; j: boolean }` must NOT emit
/// TS2322: the fresh `2` is preserved by the `2 | number` contextual type and
/// matches the literal arm.
#[test]
fn object_literal_assignable_to_number_literal_arm() {
    let codes = check_source_codes(
        r#"
const a: { k: number; j: boolean } | { k: 2 } = { k: 2 };
const b: { k: 2 } | { k: number; j: boolean } = { k: 2 };
"#,
    );
    assert!(!codes.contains(&2322), "expected no TS2322, got: {codes:?}");
}

/// Same shape with string literals.
#[test]
fn object_literal_assignable_to_string_literal_arm() {
    let codes = check_source_codes(
        r#"
const a: { a: string; b: number } | { a: "x" } = { a: "x" };
const b: { a: "x" } | { a: string; b: number } = { a: "x" };
"#,
    );
    assert!(!codes.contains(&2322), "expected no TS2322, got: {codes:?}");
}

/// Same shape with bigint literals.
#[test]
fn object_literal_assignable_to_bigint_literal_arm() {
    let codes = check_source_codes(
        r#"
const a: { n: bigint; extra: boolean } | { n: 2n } = { n: 2n };
"#,
    );
    assert!(!codes.contains(&2322), "expected no TS2322, got: {codes:?}");
}

/// Renamed binders (different property/alias names) must behave identically —
/// the rule is structural, not keyed on any identifier.
#[test]
fn object_literal_union_renamed_binders() {
    let codes = check_source_codes(
        r#"
type Wide = { value: number; flag: boolean };
type Narrow = { value: 7 };
const a: Wide | Narrow = { value: 7 };
const b: Narrow | Wide = { value: 7 };
"#,
    );
    assert!(!codes.contains(&2322), "expected no TS2322, got: {codes:?}");
}

// ---------------------------------------------------------------------------
// Through a type alias (the original large-ts-repo witness shape)
// ---------------------------------------------------------------------------

#[test]
fn object_literal_union_through_alias() {
    let codes = check_source_codes(
        r#"
type Lit = { k: 2 };
const a: Lit | { k: number; j: boolean } = { k: 2 };
"#,
    );
    assert!(!codes.contains(&2322), "expected no TS2322, got: {codes:?}");
}

// ---------------------------------------------------------------------------
// Nested object literals
// ---------------------------------------------------------------------------

#[test]
fn nested_object_literal_union_preserves_literal() {
    let codes = check_source_codes(
        r#"
const a: { o: { k: 2 } } | { o: { k: number; j: boolean } } = { o: { k: 2 } };
"#,
    );
    assert!(!codes.contains(&2322), "expected no TS2322, got: {codes:?}");
}

// ---------------------------------------------------------------------------
// Discriminated unions (same-key, same-arity) must remain unaffected
// ---------------------------------------------------------------------------

#[test]
fn discriminated_union_object_literal_ok() {
    let codes = check_source_codes(
        r#"
type Shape = { tag: "a"; v: number } | { tag: "b"; v: string };
const a: Shape = { tag: "b", v: "hi" };
const b: Shape = { tag: "a", v: 3 };
"#,
    );
    assert!(!codes.contains(&2322), "expected no TS2322, got: {codes:?}");
}

// ---------------------------------------------------------------------------
// Negative cases - genuine mismatches must STILL error
// ---------------------------------------------------------------------------

/// A literal that matches no arm by value or arity must still emit TS2322.
#[test]
fn object_literal_not_in_union_still_errors() {
    let codes = check_source_codes(
        r#"
const z: { k: 2 } | { m: string } = { k: 3 };
"#,
    );
    assert!(codes.contains(&2322), "expected TS2322, got: {codes:?}");
}

/// A string literal outside both arms' literal sets must still error.
#[test]
fn object_literal_wrong_string_literal_still_errors() {
    let codes = check_source_codes(
        r#"
const z: { a: "x" } | { a: "y"; b: number } = { a: "z" };
"#,
    );
    assert!(codes.contains(&2322), "expected TS2322, got: {codes:?}");
}

/// The literal arm is matched, but a missing required property in the only
/// arm whose discriminant fits must still error (no accidental over-acceptance).
#[test]
fn object_literal_missing_required_property_still_errors() {
    let codes = check_source_codes(
        r#"
const z: { k: 2; extra: string } | { k: number; j: boolean } = { k: 2 };
"#,
    );
    assert!(codes.contains(&2322), "expected TS2322, got: {codes:?}");
}

// ---------------------------------------------------------------------------
// Genuinely-wider union member must still be preferred (no over-fix)
// ---------------------------------------------------------------------------

/// When the union does NOT reduce to a single contained member — the two arms
/// carry disjoint primitive property types (`string` vs `number`) — the
/// per-property contextual type is a real `string | number` and a literal
/// argument is still accepted on the matching side.
#[test]
fn object_literal_disjoint_primitive_union_ok() {
    let codes = check_source_codes(
        r#"
const a: { v: string } | { v: number } = { v: "hello" };
const b: { v: string } | { v: number } = { v: 42 };
"#,
    );
    assert!(!codes.contains(&2322), "expected no TS2322, got: {codes:?}");
}
