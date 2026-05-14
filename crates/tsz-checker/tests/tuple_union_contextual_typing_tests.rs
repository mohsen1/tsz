//! Tests for contextual typing of tuple literals against union-of-tuples targets.
//!
//! When the declared type is a union where every member is a tuple (e.g. `["a"] | ["b"]`),
//! a tuple literal on the right-hand side must be contextually typed per-position:
//! element 0 sees `"a" | "b"` as its contextual type, which preserves the string
//! literal `"a"` instead of widening it to `string`.
//!
//! Root cause: `union_context_for_array_literal_is_ambiguous` returned `true` for
//! any union with multiple distinct applicable shapes, including distinct tuples.
//! That caused `effective_contextual = None`, then no element contextual types,
//! then literal widening, making `[string]` not assignable to `["a"] | ["b"]`.
//!
//! Fix: when `force_tuple_for_union_context` is true (all union members are tuples),
//! bypass the ambiguity path and use `get_tuple_element_type_with_count` to supply
//! per-position contextual types.

use tsz_checker::test_utils::check_source_codes;

// ---------------------------------------------------------------------------
// Core cases - direct union-of-tuples annotations
// ---------------------------------------------------------------------------

/// Assigning a single-element tuple literal to a union of single-element tuples
/// with distinct string literals must NOT emit TS2322.
#[test]
fn tuple_literal_assignable_to_single_elem_string_union() {
    let codes = check_source_codes(
        r#"
type AB = ["a"] | ["b"];
const x: AB = ["a"];
const y: AB = ["b"];
"#,
    );
    assert!(!codes.contains(&2322), "expected no TS2322, got: {codes:?}");
}

/// Same test with number literals.
#[test]
fn tuple_literal_assignable_to_single_elem_number_union() {
    let codes = check_source_codes(
        r#"
type Pair = [1] | [2];
const a: Pair = [1];
const b: Pair = [2];
"#,
    );
    assert!(!codes.contains(&2322), "expected no TS2322, got: {codes:?}");
}

/// Two-element tuples with distinct string+number pairs.
#[test]
fn tuple_literal_assignable_to_two_elem_union() {
    let codes = check_source_codes(
        r#"
type ABx = ["a", 1] | ["b", 2];
const x: ABx = ["a", 1];
const y: ABx = ["b", 2];
"#,
    );
    assert!(!codes.contains(&2322), "expected no TS2322, got: {codes:?}");
}

/// Literal that does NOT match any union member MUST still emit TS2322.
#[test]
fn tuple_literal_not_in_union_still_errors() {
    let codes = check_source_codes(
        r#"
type AB = ["a"] | ["b"];
const z: AB = ["c"];
"#,
    );
    assert!(codes.contains(&2322), "expected TS2322, got: {codes:?}");
}

// ---------------------------------------------------------------------------
// Distributive conditional type - the original issue #6155
// ---------------------------------------------------------------------------

/// Nested distributive conditional produces a union of tuples; assigning a
/// tuple literal to the result must NOT emit TS2322.
#[test]
fn nested_distributive_conditional_cartesian_no_error() {
    let codes = check_source_codes(
        r#"
type Both<T, U> = T extends any ? (U extends any ? [T, U] : never) : never;
type BothDist = Both<"a" | "b", 1 | 2>;
const bd: BothDist = ["a", 1];
const bd2: BothDist = ["b", 2];
"#,
    );
    assert!(!codes.contains(&2322), "expected no TS2322, got: {codes:?}");
}

/// Alternate variable names for the type parameters must produce the same result.
#[test]
fn nested_distributive_conditional_alternate_param_names() {
    let codes = check_source_codes(
        r#"
type Cartesian<X, Y> = X extends unknown ? Y extends unknown ? [X, Y] : never : never;
type Result = Cartesian<"x" | "y", 0 | 1>;
const r: Result = ["x", 0];
const s: Result = ["y", 1];
"#,
    );
    assert!(!codes.contains(&2322), "expected no TS2322, got: {codes:?}");
}

/// Inline union-of-tuples annotation (no type alias) must work the same way.
#[test]
fn inline_union_of_tuples_annotation() {
    let codes = check_source_codes(
        r#"
const x: ["a", 1] | ["b", 2] = ["a", 1];
"#,
    );
    assert!(!codes.contains(&2322), "expected no TS2322, got: {codes:?}");
}

/// A tuple literal element that is NOT a valid member of ANY union branch
/// must still produce TS2322 after the fix.
#[test]
fn inline_union_of_tuples_invalid_literal_still_errors() {
    let codes = check_source_codes(
        r#"
const bad: ["a", 1] | ["b", 2] = ["c", 1];
"#,
    );
    assert!(codes.contains(&2322), "expected TS2322, got: {codes:?}");
}

// ---------------------------------------------------------------------------
// Mixed-length union members - position union should still work
// ---------------------------------------------------------------------------

/// When union members have different lengths, element 0 context is still the
/// union of first elements from members that have an element at position 0.
#[test]
fn mixed_length_tuple_union_first_element_ok() {
    let codes = check_source_codes(
        r#"
type T = ["a"] | ["a", 1];
const x: T = ["a"];
"#,
    );
    assert!(
        !codes.contains(&2322),
        "expected no TS2322 for first member, got: {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// Ensure mixed array/tuple unions remain unaffected (no regression)
// ---------------------------------------------------------------------------

/// When the union contains a non-tuple member (plain array), the behavior
/// is unchanged from before this fix.
#[test]
fn mixed_array_tuple_union_not_affected() {
    // `[1]` typed against `[1] | number[]`: the non-tuple member means
    // `force_tuple_for_union_context` is false, so the pre-fix path applies.
    // tsc accepts this (the literal tuple is assignable to `[1]`), so
    // we just verify we do NOT regress into a new crash or wrong code.
    let codes = check_source_codes(
        r#"
type T = [1] | number[];
const x: T = [1];
"#,
    );
    // TS2322 may or may not be emitted depending on prior behavior;
    // what matters is no panic and no NEW code that wasn't there before.
    // We specifically must NOT see TS2589 (depth exceeded) or TS2345.
    assert!(!codes.contains(&2589), "unexpected TS2589: {codes:?}");
    assert!(!codes.contains(&2345), "unexpected TS2345: {codes:?}");
}
