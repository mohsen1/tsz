//! Tests for TS2353: Object literal may only specify known properties,
//! and '{prop}' does not exist in type '{Type}'.
//!
//! These tests cover:
//! - Discriminated union excess property checking (narrowed member)
//! - Type alias name display in error messages

use crate::CheckerState;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn get_diagnostics(source: &str) -> Vec<(u32, String)> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

// --- Discriminated union excess property checking ---

#[test]
fn discriminated_union_reports_excess_property_on_narrowed_member() {
    // When a fresh object literal with a discriminant is assigned to a
    // discriminated union, tsc narrows to the matching member and reports
    // excess properties against that member (TS2353), not a generic TS2322.
    let source = r#"
type Square = { kind: "sq", size: number }
type Rectangle = { kind: "rt", x: number, y: number }
type Shape = Square | Rectangle
let s: Shape = { kind: "sq", x: 12 }
"#;
    let diags = get_diagnostics(source);
    // Should emit TS2353, not TS2322
    assert!(
        diags.iter().any(|d| d.0 == 2353),
        "Expected TS2353, got: {diags:?}"
    );
    assert!(
        !diags.iter().any(|d| d.0 == 2322),
        "Should NOT emit TS2322 when TS2353 fires: {diags:?}"
    );
}

#[test]
fn discriminated_union_excess_reports_first_property_by_source_position() {
    // tsc reports the first excess property in source order.
    let source = r#"
type Square = { kind: "sq", size: number }
type Rectangle = { kind: "rt", x: number, y: number }
type Shape = Square | Rectangle
let s: Shape = { kind: "sq", x: 12, y: 13 }
"#;
    let diags = get_diagnostics(source);
    let ts2353 = diags.iter().find(|d| d.0 == 2353);
    assert!(ts2353.is_some(), "Expected TS2353, got: {diags:?}");
    // 'x' appears before 'y' in the source, so 'x' should be reported
    let msg = &ts2353.unwrap().1;
    assert!(
        msg.contains("'x'"),
        "Expected excess property 'x' (first in source), got: {msg}"
    );
}

#[test]
fn discriminated_union_excess_message_uses_type_alias_name() {
    // The error message should reference the type alias name (e.g., "Square")
    // instead of the structural type "{ size: number; kind: \"sq\" }".
    let source = r#"
type Square = { kind: "sq", size: number }
type Rectangle = { kind: "rt", x: number, y: number }
type Shape = Square | Rectangle
let s: Shape = { kind: "sq", x: 12 }
"#;
    let diags = get_diagnostics(source);
    let ts2353 = diags.iter().find(|d| d.0 == 2353);
    assert!(ts2353.is_some(), "Expected TS2353, got: {diags:?}");
    let msg = &ts2353.unwrap().1;
    assert!(
        msg.contains("'Square'"),
        "Expected type alias name 'Square' in message, got: {msg}"
    );
}

#[test]
fn discriminated_union_with_missing_required_and_excess_reports_ts2353() {
    // When a fresh object has a discriminant matching one member but is missing
    // a required property AND has an excess property, tsc reports TS2353 (excess)
    // against the narrowed member. The missing property is a secondary concern.
    let source = r#"
type Square = { kind: "sq", size: number }
type Rectangle = { kind: "rt", x: number, y: number }
type Shape = Square | Rectangle
let s: Shape = { kind: "sq", x: 12, y: 13 }
"#;
    let diags = get_diagnostics(source);
    assert!(
        diags.iter().any(|d| d.0 == 2353),
        "Expected TS2353 for excess 'x' on narrowed Square, got: {diags:?}"
    );
    // Exactly one TS2353 error (for the first excess property)
    let ts2353_count = diags.iter().filter(|d| d.0 == 2353).count();
    assert_eq!(
        ts2353_count, 1,
        "Expected exactly 1 TS2353 error, got {ts2353_count}"
    );
}

#[test]
fn non_discriminated_union_does_not_use_discriminant_narrowing() {
    // When the union has no unit-type discriminant, we shouldn't
    // use discriminant narrowing. This should fall through to normal checking.
    let source = r#"
type A = { x: number, y: number }
type B = { x: number, z: string }
type AB = A | B
let v: AB = { x: 1, w: true }
"#;
    // w is excess in both A and B, so some error should fire
    let diags = get_diagnostics(source);
    let has_any_error = !diags.is_empty();
    assert!(has_any_error, "Expected some error for excess property 'w'");
}

// --- Type alias name display in diagnostics ---

#[test]
fn type_alias_name_displayed_in_ts2322_message() {
    // Type alias names should appear in TS2322 messages.
    // Before the fix, this would show the structural type instead.
    let source = r#"
type Point = { x: number, y: number }
let p: Point = { x: 1, z: 3 }
"#;
    let diags = get_diagnostics(source);
    // We expect an error referencing 'Point'
    let has_point_name = diags.iter().any(|d| d.1.contains("'Point'"));
    assert!(
        has_point_name,
        "Expected type alias 'Point' in error message, got: {diags:?}"
    );
}

#[test]
fn interface_name_still_displayed_correctly() {
    // Interfaces already displayed their names correctly; ensure no regression.
    let source = r#"
interface Foo { a: number }
let f: Foo = { a: 1, b: 2 }
"#;
    let diags = get_diagnostics(source);
    let has_foo_name = diags.iter().any(|d| d.1.contains("'Foo'"));
    assert!(
        has_foo_name,
        "Expected interface name 'Foo' in error message, got: {diags:?}"
    );
}
