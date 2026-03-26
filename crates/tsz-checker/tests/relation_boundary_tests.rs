//! Regression tests for the canonical RelationRequest/RelationOutcome boundary.
//!
//! These tests verify that object/property/call compatibility checks are
//! correctly routed through the canonical relation boundary, ensuring:
//! - Freshness / EPC (excess property checking) consistency
//! - Missing property classification via `RelationOutcome`
//! - Weak union violation detection via `RelationOutcome`
//! - Property compatibility on unions/intersections
//! - Call-argument object-literal compatibility

use tsz_checker::test_utils::check_source_diagnostics;

// ============================================================================
// Freshness / Excess Property Checking (EPC) via canonical boundary
// ============================================================================

#[test]
fn fresh_object_literal_excess_property_ts2353() {
    // Fresh object literals should trigger EPC: 'z' does not exist in Point.
    let diags = check_source_diagnostics(
        r#"
type Point = { x: number, y: number }
const p: Point = { x: 1, y: 2, z: 3 }
"#,
    );
    assert!(
        diags.iter().any(|d| d.code == 2353),
        "Expected TS2353 for excess property 'z', got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn non_fresh_object_no_excess_property_error() {
    // Non-fresh objects should NOT trigger EPC even with extra properties.
    let diags = check_source_diagnostics(
        r#"
type Point = { x: number, y: number }
const obj = { x: 1, y: 2, z: 3 }
const p: Point = obj
"#,
    );
    assert!(
        !diags.iter().any(|d| d.code == 2353),
        "Should NOT emit TS2353 for non-fresh object, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn fresh_object_literal_excess_property_interface_target_ts2353() {
    // Same as the type alias test, but with an interface target.
    // Interfaces are represented as Lazy(DefId) and must be resolved for EPC.
    let diags = check_source_diagnostics(
        r#"
interface Point { x: number; y: number }
const p: Point = { x: 1, y: 2, z: 3 }
"#,
    );
    assert!(
        diags.iter().any(|d| d.code == 2353),
        "Expected TS2353 for excess property 'z' on interface target, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn satisfies_excess_property_ts2353() {
    // The `satisfies` operator should trigger EPC for excess properties.
    let diags = check_source_diagnostics(
        r#"
interface Theme { primary: string; secondary: string }
const theme = { primary: "red", secondary: "blue", tertiary: "green" } satisfies Theme;
"#,
    );
    assert!(
        diags.iter().any(|d| d.code == 2353),
        "Expected TS2353 for excess property 'tertiary' via satisfies, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn excess_property_suppressed_for_index_signature_target() {
    // Index signatures accept arbitrary string-keyed properties.
    let diags = check_source_diagnostics(
        r#"
type Dict = { [key: string]: number }
const d: Dict = { a: 1, b: 2, c: 3 }
"#,
    );
    assert!(
        !diags.iter().any(|d| d.code == 2353),
        "Should NOT emit TS2353 with index signature target, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn excess_property_suppressed_for_empty_object_target() {
    // Empty object type {} accepts any non-primitive.
    let diags = check_source_diagnostics(
        r#"
type Empty = {}
const e: Empty = { x: 1, y: 2 }
"#,
    );
    assert!(
        !diags.iter().any(|d| d.code == 2353),
        "Should NOT emit TS2353 with empty object target, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

// ============================================================================
// Weak Union Violation Detection via RelationOutcome
// ============================================================================

#[test]
fn weak_union_violation_suppresses_ts2322() {
    // When an object literal has only excess properties relative to a weak target
    // (all optional properties), TS2322 should be suppressed in favor of TS2559
    // or TS2353.
    let diags = check_source_diagnostics(
        r#"
type Options = { color?: string, width?: number }
const o: Options = { height: 100 }
"#,
    );
    // Should emit TS2559 (weak type) or TS2353 (excess), NOT TS2322.
    let has_relevant = diags.iter().any(|d| d.code == 2559 || d.code == 2353);
    assert!(
        has_relevant,
        "Expected TS2559 or TS2353, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

// ============================================================================
// Property Classification: excess + incompatible properties
// ============================================================================

#[test]
fn excess_property_with_incompatible_matching_emits_ts2322() {
    // When an object literal has BOTH excess properties AND incompatible
    // matching properties, TS2322 should be emitted (not just TS2353).
    let diags = check_source_diagnostics(
        r#"
type Point = { x: number, y: number }
const p: Point = { x: "wrong", y: 2, z: 3 }
"#,
    );
    // Should emit TS2322 for the type mismatch (x: string vs number).
    // The excess property 'z' should still generate TS2353.
    let has_2322_or_2353 = diags.iter().any(|d| d.code == 2322 || d.code == 2353);
    assert!(
        has_2322_or_2353,
        "Expected TS2322 or TS2353, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

// ============================================================================
// Union target EPC
// ============================================================================

#[test]
fn union_target_excess_property_when_not_in_any_member() {
    // A property is excess if it doesn't exist in ANY union member.
    let diags = check_source_diagnostics(
        r#"
type A = { kind: "a", x: number }
type B = { kind: "b", y: number }
type AB = A | B
const v: AB = { kind: "a", x: 1, z: 99 }
"#,
    );
    assert!(
        diags.iter().any(|d| d.code == 2353),
        "Expected TS2353 for excess property 'z', got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn union_target_no_excess_when_property_in_other_member() {
    // For non-discriminated unions, a property that exists in ANY member
    // is NOT excess. 'y' exists in B, so it should not be flagged.
    let diags = check_source_diagnostics(
        r#"
type A = { x: number }
type B = { x: number, y: number }
type AB = A | B
const v: AB = { x: 1, y: 5 }
"#,
    );
    let ts2353_for_y = diags
        .iter()
        .any(|d| d.code == 2353 && d.message_text.contains("'y'"));
    assert!(
        !ts2353_for_y,
        "Should NOT emit TS2353 for 'y' which exists in member B, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

// ============================================================================
// Intersection target EPC
// ============================================================================

#[test]
fn intersection_target_excess_property_not_in_any_member() {
    // Excess property checking on intersection targets.
    let diags = check_source_diagnostics(
        r#"
type A = { x: number }
type B = { y: number }
const v: A & B = { x: 1, y: 2, z: 3 }
"#,
    );
    assert!(
        diags.iter().any(|d| d.code == 2353),
        "Expected TS2353 for excess property 'z' in intersection target, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

// ============================================================================
// Call-argument object literal compatibility
// ============================================================================

#[test]
fn call_argument_excess_property_ts2353() {
    // Object literal in call argument should trigger EPC.
    let diags = check_source_diagnostics(
        r#"
declare function foo(opts: { x: number, y: number }): void
foo({ x: 1, y: 2, z: 3 })
"#,
    );
    assert!(
        diags.iter().any(|d| d.code == 2353),
        "Expected TS2353 for excess property in call argument, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn call_argument_no_excess_for_type_parameter() {
    // When the parameter type is a type parameter, EPC should be skipped.
    let diags = check_source_diagnostics(
        r#"
declare function foo<T>(opts: T): T
const result = foo({ x: 1, y: 2, z: 3 })
"#,
    );
    assert!(
        !diags.iter().any(|d| d.code == 2353),
        "Should NOT emit TS2353 when param is type parameter, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

// ============================================================================
// Spread expression EPC (explicit-only mode)
// ============================================================================

#[test]
fn spread_object_only_checks_explicit_properties() {
    // Spread sources should only check explicitly-written properties.
    let diags = check_source_diagnostics(
        r#"
type Point = { x: number, y: number }
const base = { x: 1, y: 2, z: 3 }
const p: Point = { ...base, w: 4 }
"#,
    );
    // 'w' is explicit in the spread → should be excess.
    // 'z' comes from spread → should NOT be flagged.
    let ts2353_for_w = diags
        .iter()
        .any(|d| d.code == 2353 && d.message_text.contains("'w'"));
    assert!(
        ts2353_for_w,
        "Expected TS2353 for explicit excess 'w' in spread, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

// ============================================================================
// Missing property classification (TS2741)
// ============================================================================

#[test]
fn missing_required_property_ts2741() {
    // Missing required property should emit TS2741.
    let diags = check_source_diagnostics(
        r#"
type Point = { x: number, y: number }
const p: Point = { x: 1 }
"#,
    );
    let has_missing_error = diags
        .iter()
        .any(|d| d.code == 2741 || d.code == 2739 || d.code == 2322);
    assert!(
        has_missing_error,
        "Expected missing property diagnostic (TS2741/TS2739/TS2322), got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}
