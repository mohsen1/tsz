//! Object-literal → union-target elaboration selects the member `tsc`'s
//! `getBestMatchingType` selects.
//!
//! Structural rule: when an object literal is assigned to a union and fails,
//! `tsc` picks the member to elaborate against with `getBestMatchingType`,
//! which tries `findMatchingDiscriminantType` *before* `findMostOverlappyType`.
//! So a written unit discriminant (`kind: "a"`) selects the member whose
//! discriminant it matches — even when every member shares that same key and a
//! pure key-overlap heuristic would tie and fall to the *last* member. And when
//! the source shares no property key with any member, `tsc` selects no member
//! at all (its `findMostOverlappyType` needs a unit-typed key intersection), so
//! the failure is the bare union line with no nested missing-property drill.
//!
//! Owner: `SubtypeChecker::select_union_target_best_member`
//! (`crates/tsz-solver/src/relations/subtype/explain_union_discriminant.rs`).
//!
//! These tests vary the discriminant key name, the member/property names, and
//! the written discriminant value so a fix keyed to a particular spelling would
//! not satisfy them; they assert *which* property is named missing (i.e. which
//! member was selected), not the exact rendered source type.

use tsz_checker::test_utils::check_source_diagnostics;
use tsz_common::diagnostics::Diagnostic;

/// Every `Property '<name>' is missing ... but required in type ...` message on
/// a TS2322 diagnostic in `source`.
fn missing_property_elaborations(source: &str) -> Vec<String> {
    check_source_diagnostics(source)
        .iter()
        .filter(|diagnostic: &&Diagnostic| diagnostic.code == 2322)
        .flat_map(|diagnostic| diagnostic.related_information.iter())
        .map(|info| info.message_text.clone())
        .filter(|text| text.contains("is missing") && text.contains("but required in type"))
        .collect()
}

fn names_missing_property(source: &str, property: &str) -> bool {
    let needle = format!("Property '{property}' is missing");
    missing_property_elaborations(source)
        .iter()
        .any(|text| text.contains(&needle))
}

// ---------------------------------------------------------------------------
// Discriminant selects the matching member, not the last-overlapping one.
// Both members share only the discriminant key, so pure key-overlap ties and
// (breaking to the last member) would name the wrong missing property.
// ---------------------------------------------------------------------------

#[test]
fn first_member_discriminant_selects_first_member() {
    let source = r#"
        type Shape = { kind: "a"; alpha: number } | { kind: "b"; beta: string };
        const value: Shape = { kind: "a" };
        export {};
    "#;
    // Discriminant `kind: "a"` -> first member -> `alpha` is the missing prop.
    assert!(
        names_missing_property(source, "alpha"),
        "expected the `kind: \"a\"` member's `alpha` to be reported missing, got {:#?}",
        missing_property_elaborations(source)
    );
    assert!(
        !names_missing_property(source, "beta"),
        "must not elaborate the non-matching `kind: \"b\"` member; got {:#?}",
        missing_property_elaborations(source)
    );
}

#[test]
fn last_member_discriminant_selects_last_member() {
    let source = r#"
        type Shape = { tag: "one"; first: number } | { tag: "two"; second: string };
        const value: Shape = { tag: "two" };
        export {};
    "#;
    assert!(
        names_missing_property(source, "second"),
        "expected the `tag: \"two\"` member's `second` to be reported missing, got {:#?}",
        missing_property_elaborations(source)
    );
    assert!(!names_missing_property(source, "first"));
}

#[test]
fn middle_member_discriminant_over_three_members() {
    // Three same-keyed members: overlap ties three ways and would pick the
    // *last* (`third`); the discriminant must pick the middle one.
    let source = r#"
        type Shape =
            | { variant: "x"; ex: number }
            | { variant: "y"; why: string }
            | { variant: "z"; zed: boolean };
        const value: Shape = { variant: "y" };
        export {};
    "#;
    assert!(
        names_missing_property(source, "why"),
        "expected the `variant: \"y\"` member's `why` to be reported missing, got {:#?}",
        missing_property_elaborations(source)
    );
    assert!(!names_missing_property(source, "ex"));
    assert!(!names_missing_property(source, "zed"));
}

#[test]
fn discriminant_name_is_not_hardcoded() {
    // A discriminant key with an unusual name, to rule out a `kind`/`tag`
    // spelling fast-path.
    let source = r#"
        type Node = { __disc: "leaf"; payload: number } | { __disc: "branch"; children: string };
        const node: Node = { __disc: "branch" };
        export {};
    "#;
    assert!(
        names_missing_property(source, "children"),
        "expected the `__disc: \"branch\"` member's `children` missing, got {:#?}",
        missing_property_elaborations(source)
    );
    assert!(!names_missing_property(source, "payload"));
}

// ---------------------------------------------------------------------------
// No discriminant: the key-overlap heuristic still applies (ties to the last
// member), matching `findMostOverlappyType`.
// ---------------------------------------------------------------------------

#[test]
fn no_discriminant_falls_back_to_key_overlap() {
    // `common` is typed `string` (not a unit), so it is not a discriminant;
    // both members share `common`, overlap ties, and tsc breaks to the last
    // member -> `beta` is reported missing.
    let source = r#"
        type Shape = { alpha: number; common: string } | { beta: number; common: string };
        const value: Shape = { common: "present" };
        export {};
    "#;
    assert!(
        names_missing_property(source, "beta"),
        "expected the last overlapping member's `beta` missing, got {:#?}",
        missing_property_elaborations(source)
    );
    assert!(!names_missing_property(source, "alpha"));
}

// ---------------------------------------------------------------------------
// Zero key overlap: tsc selects no member and emits only the bare union line.
// ---------------------------------------------------------------------------

#[test]
fn zero_overlap_source_has_no_nested_elaboration() {
    let source = r#"
        type Shape = { alpha: number } | { beta: string };
        const value: Shape = {};
        export {};
    "#;
    let elaborations = missing_property_elaborations(source);
    assert!(
        elaborations.is_empty(),
        "a source sharing no key with any member must not drill into a member; got {elaborations:#?}"
    );
    // The bare TS2322 union line must still fire.
    assert!(
        check_source_diagnostics(source)
            .iter()
            .any(|diagnostic| diagnostic.code == 2322),
        "expected the bare TS2322 union-mismatch line"
    );
}

#[test]
fn disjoint_keyed_source_has_no_nested_elaboration() {
    // Source carries a key, but it overlaps no member -> still no drill.
    let source = r#"
        type Shape = { alpha: number } | { beta: string };
        const value: Shape = { gamma: 1 };
        export {};
    "#;
    assert!(
        missing_property_elaborations(source).is_empty(),
        "a source whose only key overlaps no member must not drill; got {:#?}",
        missing_property_elaborations(source)
    );
}
