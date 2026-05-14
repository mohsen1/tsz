//! Tests for TS2352 intersection type assertion overlap.
//!
//! Structural rule: when asserting `S as T` where T is an intersection
//! `T1 & T2 & ... & Tn`, tsc's `comparableRelation` via `eachTypeRelatedToType`
//! requires S to be comparable to every member Ti. When S itself is an
//! intersection `S1 & S2 & ... & Sn`, the intersection carries all properties
//! from all members, so comparability against any member Si suffices.
//!
//! These tests cover both the solver-level `types_are_comparable_for_assertion`
//! and the checker-level `object_properties_are_comparable` path.

use crate::test_utils::check_source_strict_codes as check_strict;

// ---------------------------------------------------------------------------
// Valid assertions: intersection TARGET — source overlaps with each member
// ---------------------------------------------------------------------------

/// Asserting an object literal to an intersection of two compatible interfaces
/// must NOT emit TS2352.
#[test]
fn object_to_two_member_intersection_no_ts2352() {
    for (n1, n2, target) in [
        ("HasName", "HasId", "HasName & HasId"),
        ("A", "B", "A & B"),
        ("Part1", "Part2", "Part1 & Part2"),
    ] {
        let source = format!(
            r#"
interface {n1} {{ name: string }}
interface {n2} {{ id: number }}
type Combined = {target};
declare let obj: {{ name: string; id: number; extra: boolean }};
let ok = obj as Combined;
"#
        );
        let codes = check_strict(&source);
        let ts2352: Vec<&u32> = codes.iter().filter(|c| **c == 2352).collect();
        assert!(
            ts2352.is_empty(),
            "[{n1} & {n2}] no TS2352 expected — source overlaps with both members. \
             Got: {codes:?}"
        );
    }
}

/// Asserting to an inline intersection type must also work correctly.
#[test]
fn object_to_inline_intersection_no_ts2352() {
    let source = r#"
declare let src: { name: string; id: number };
let ok = src as ({ name: string } & { id: number });
"#;
    let codes = check_strict(source);
    let ts2352: Vec<&u32> = codes.iter().filter(|c| **c == 2352).collect();
    assert!(
        ts2352.is_empty(),
        "no TS2352 expected — source has both `name` and `id`. Got: {codes:?}"
    );
}

/// Three-member intersection: source overlaps all three members → no TS2352.
#[test]
fn object_to_three_member_intersection_no_ts2352() {
    let source = r#"
interface X { x: number }
interface Y { y: string }
interface Z { z: boolean }
type XYZ = X & Y & Z;
declare let xyz: { x: number; y: string; z: boolean };
let ok = xyz as XYZ;
"#;
    let codes = check_strict(source);
    let ts2352: Vec<&u32> = codes.iter().filter(|c| **c == 2352).collect();
    assert!(
        ts2352.is_empty(),
        "no TS2352 expected — source has all three properties. Got: {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// Valid assertions: intersection SOURCE — any member overlaps target
// ---------------------------------------------------------------------------

/// When the source itself is an intersection, it carries all members' properties.
/// Asserting to a type that overlaps with any member must NOT emit TS2352.
#[test]
fn intersection_source_to_compatible_target_no_ts2352() {
    for (a, b) in [("HasName", "HasId"), ("Part1", "Part2"), ("A", "B")] {
        let source = format!(
            r#"
interface {a} {{ name: string }}
interface {b} {{ id: number }}
type Both = {a} & {b};
declare let both: Both;
let ok1 = both as {{ name: string }};
let ok2 = both as {{ id: number }};
"#
        );
        let codes = check_strict(&source);
        let ts2352: Vec<&u32> = codes.iter().filter(|c| **c == 2352).collect();
        assert!(
            ts2352.is_empty(),
            "[{a} & {b}] no TS2352 expected — intersection source overlaps with each target. \
             Got: {codes:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// Invalid assertions: must emit TS2352
// ---------------------------------------------------------------------------

/// Asserting `number` to an object intersection must emit TS2352 —
/// a primitive has no properties to overlap with any object member.
#[test]
fn primitive_to_object_intersection_emits_ts2352() {
    let source = r#"
declare let n: number;
let bad = n as ({ a: string } & { b: number });
"#;
    let codes = check_strict(source);
    let ts2352: Vec<&u32> = codes.iter().filter(|c| **c == 2352).collect();
    assert!(
        !ts2352.is_empty(),
        "TS2352 expected — `number` has no overlap with `{{a: string}} & {{b: number}}`. \
         Got: {codes:?}"
    );
}

/// `string` to an object intersection must also emit TS2352.
#[test]
fn string_primitive_to_object_intersection_emits_ts2352() {
    let source = r#"
declare let s: string;
let bad = s as ({ x: number } & { y: string });
"#;
    let codes = check_strict(source);
    let ts2352: Vec<&u32> = codes.iter().filter(|c| **c == 2352).collect();
    assert!(
        !ts2352.is_empty(),
        "TS2352 expected — `string` has no overlap with `{{x: number}} & {{y: string}}`. \
         Got: {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// Generic intersection targets
// ---------------------------------------------------------------------------

/// Asserting to an intersection of same generic interface with different type
/// args that widen to a compatible type must NOT emit TS2352.
#[test]
fn generic_box_intersection_compatible_no_ts2352() {
    for p in ["T", "U", "V"] {
        let source = format!(
            r#"
interface Box<{p}> {{ value: {p} }}
type MultiBox = Box<string> & Box<number>;
declare let anyBox: {{ value: string | number }};
let ok = anyBox as MultiBox;
"#
        );
        let codes = check_strict(&source);
        let ts2352: Vec<&u32> = codes.iter().filter(|c| **c == 2352).collect();
        assert!(
            ts2352.is_empty(),
            "[Box<{p}>] no TS2352 expected — source is compatible with both Box members. \
             Got: {codes:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// Sanity: non-intersection baseline
// ---------------------------------------------------------------------------

/// Asserting an object to a plain (non-intersection) interface keeps working.
#[test]
fn object_to_plain_interface_no_ts2352() {
    let source = r#"
interface Named { name: string }
declare let obj: { name: string; extra: number };
let ok = obj as Named;
"#;
    let codes = check_strict(source);
    let ts2352: Vec<&u32> = codes.iter().filter(|c| **c == 2352).collect();
    assert!(
        ts2352.is_empty(),
        "no TS2352 expected — object overlaps with Named. Got: {codes:?}"
    );
}
