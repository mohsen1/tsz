//! Regression tests for target-intersection elaboration member **order**
//! (#16753).
//!
//! Structural rule (`tsc`'s `typeRelatedToEachType`, quoted in the issue): a
//! source related to a target intersection `C1 & C2 & …` is related to each
//! constituent in **written** order, and the **first** failing constituent is
//! elaborated one level deeper — the same order the top-level headline prints.
//!
//! `normalize_intersection` rebuilds a mixed object / non-object intersection
//! with the object member moved to the end (`{ z: 1 } & [string, number]`
//! interns as `[string, number] & { z: 1 }`), so the interned member list no
//! longer matches the written order. The elaboration must recover the written
//! order (from the display alias) rather than iterate the reordered members,
//! otherwise it names the non-object member even when the object comes first.
//!
//! Binder-independent: the rule is structural, so the property/element spellings
//! vary across rows and none drives the decision.

use crate::test_utils::check_source_diagnostics;

/// The nested elaboration line (the second-level `Type '…' is not assignable to
/// type '…'.`) of the single `TS2322` produced by `source`.
fn nested_line(source: &str) -> String {
    let diags = check_source_diagnostics(source);
    let matching: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        matching.len(),
        1,
        "Expected exactly one TS2322. Got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.message_text.clone()))
            .collect::<Vec<_>>()
    );
    matching[0]
        .related_information
        .first()
        .map(|info| info.message_text.clone())
        .unwrap_or_default()
}

/// Issue witness: an object-first intersection with a tuple names the **object**
/// member, matching tsc (`... to type '{ z: 1; }'.`), not the tuple.
#[test]
fn object_first_then_tuple_names_the_object() {
    assert_eq!(
        nested_line("const b: { z: 1 } & [string, ...[number, boolean]] = 1;\n"),
        "Type 'number' is not assignable to type '{ z: 1; }'.",
    );
}

/// A plain (non-spread) tuple behaves the same — the divergence was never
/// spread-specific.
#[test]
fn object_first_then_plain_tuple_names_the_object() {
    assert_eq!(
        nested_line("const c: { z: 1 } & [string, number] = 1;\n"),
        "Type 'number' is not assignable to type '{ z: 1; }'.",
    );
}

/// An object-first intersection with a primitive names the object.
#[test]
fn object_first_then_primitive_names_the_object() {
    assert_eq!(
        nested_line("const d: { a: 0 } & string = 1;\n"),
        "Type 'number' is not assignable to type '{ a: 0; }'.",
    );
}

/// An object-first intersection with an array names the object.
#[test]
fn object_first_then_array_names_the_object() {
    assert_eq!(
        nested_line("const e: { k: 2 } & string[] = 1;\n"),
        "Type 'number' is not assignable to type '{ k: 2; }'.",
    );
}

/// Control — a tuple-first intersection still names the tuple (both compilers
/// already agreed here). The fix must not flip this row.
#[test]
fn tuple_first_then_object_still_names_the_tuple() {
    assert_eq!(
        nested_line("const f: [string, number] & { z: 1 } = 1;\n"),
        "Type 'number' is not assignable to type '[string, number]'.",
    );
}

/// A three-way intersection names the first written constituent the source
/// fails — the leading object.
#[test]
fn three_way_object_first_names_the_first_object() {
    assert_eq!(
        nested_line("const g: { z: 1 } & { w: 2 } & [string, number] = 1;\n"),
        "Type 'number' is not assignable to type '{ z: 1; }'.",
    );
}

/// Control — a two-object intersection is unaffected: still the first object.
#[test]
fn two_object_intersection_names_the_first_object() {
    assert_eq!(
        nested_line("const h: { z: 1 } & { w: 2 } = 1;\n"),
        "Type 'number' is not assignable to type '{ z: 1; }'.",
    );
}
