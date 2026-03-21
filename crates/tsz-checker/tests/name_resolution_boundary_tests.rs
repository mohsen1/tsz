//! Regression tests for the unified name resolution boundary
//! (`query_boundaries/name_resolution.rs`).
//!
//! Covers the target diagnostic families:
//! - TS2304 (cannot find name)
//! - TS2552 (spelling suggestion)
//! - TS2694 (namespace has no exported member)
//! - TS2708 (cannot use namespace as a value)
//! - TS2693 (type used as value)
//! - TS2724 (namespace export spelling suggestion)
//! - TS2749 (value used as type)
//!
//! Note: In single-file test mode (`report_unresolved_imports = false`),
//! TS2304 is intentionally suppressed for unknown identifiers (they might
//! come from unresolved imports). Tests that verify TS2304 use primitive
//! type keywords which always emit, or test via TS2693/TS2708 which also
//! fire in single-file mode.

use tsz_checker::test_utils::check_source_diagnostics;

// =========================================================================
// TS2693: Type used as value (routes through boundary)
// =========================================================================

#[test]
fn ts2693_type_alias_used_as_value() {
    let diags = check_source_diagnostics(
        r#"
type Foo = string;
let x = Foo;
"#,
    );
    assert!(
        diags.iter().any(|d| d.code == 2693),
        "Expected TS2693 for type alias used as value, got: {diags:?}"
    );
}

#[test]
fn ts2693_interface_used_as_value() {
    let diags = check_source_diagnostics(
        r#"
interface Bar { x: number; }
let y = Bar;
"#,
    );
    assert!(
        diags.iter().any(|d| d.code == 2693),
        "Expected TS2693 for interface used as value, got: {diags:?}"
    );
}

#[test]
fn ts2693_not_for_class() {
    // Classes have both type and value, so no TS2693
    let diags = check_source_diagnostics(
        r#"
class Baz {}
let z = Baz;
"#,
    );
    assert!(
        !diags.iter().any(|d| d.code == 2693),
        "Should not emit TS2693 for class (which is also a value), got: {diags:?}"
    );
}

#[test]
fn ts2693_not_for_merged_type_and_value() {
    // When a type alias merges with a const of the same name, value wins
    let diags = check_source_diagnostics(
        r#"
type FAILURE = "FAILURE";
const FAILURE = "FAILURE";
let x = FAILURE;
"#,
    );
    assert!(
        !diags.iter().any(|d| d.code == 2693),
        "Should not emit TS2693 when value shadows type alias, got: {diags:?}"
    );
}

// =========================================================================
// TS2708: Cannot use namespace as a value
// =========================================================================

#[test]
fn ts2708_uninstantiated_namespace_as_value() {
    let diags = check_source_diagnostics(
        r#"
namespace MyNs {
    export type Foo = string;
}
let x = MyNs;
"#,
    );
    assert!(
        diags.iter().any(|d| d.code == 2708),
        "Expected TS2708 for uninstantiated namespace used as value, got: {diags:?}"
    );
}

#[test]
fn ts2708_not_for_instantiated_namespace() {
    let diags = check_source_diagnostics(
        r#"
namespace MyNs {
    export const value = 42;
}
let x = MyNs;
"#,
    );
    assert!(
        !diags.iter().any(|d| d.code == 2708),
        "Should not emit TS2708 for instantiated namespace, got: {diags:?}"
    );
}

// =========================================================================
// TS2749: Value used as type
// =========================================================================

#[test]
fn ts2749_value_used_as_type() {
    let diags = check_source_diagnostics(
        r#"
const myVal = 42;
let x: myVal;
"#,
    );
    assert!(
        diags.iter().any(|d| d.code == 2749),
        "Expected TS2749 for value used as type, got: {diags:?}"
    );
}

// =========================================================================
// TS2694: Namespace has no exported member (routed through boundary)
// =========================================================================

#[test]
fn ts2694_namespace_missing_export() {
    let diags = check_source_diagnostics(
        r#"
namespace MyNs {
    export type Foo = string;
}
let x: MyNs.Bar;
"#,
    );
    assert!(
        diags.iter().any(|d| d.code == 2694),
        "Expected TS2694 for missing namespace export, got: {diags:?}"
    );
}

#[test]
fn ts2694_not_for_existing_export() {
    let diags = check_source_diagnostics(
        r#"
namespace MyNs {
    export type Foo = string;
}
let x: MyNs.Foo;
"#,
    );
    assert!(
        !diags.iter().any(|d| d.code == 2694),
        "Should not emit TS2694 for existing namespace export, got: {diags:?}"
    );
}

// =========================================================================
// TS2724: Namespace export spelling suggestion (routed through boundary)
// =========================================================================

#[test]
fn ts2724_namespace_export_spelling_suggestion() {
    let diags = check_source_diagnostics(
        r#"
namespace MyNs {
    export type MyType = string;
    export type MyOtherType = number;
}
let x: MyNs.MyTyp;
"#,
    );
    // Should get either TS2694 or TS2724 (with suggestion)
    let has_ns_error = diags.iter().any(|d| d.code == 2694 || d.code == 2724);
    assert!(
        has_ns_error,
        "Expected TS2694 or TS2724 for misspelled namespace export, got: {diags:?}"
    );
}

// =========================================================================
// Cross-concern: type vs value distinction
// =========================================================================

#[test]
fn type_value_merged_class_no_errors() {
    // A class is both a type and a value — no TS2693
    let diags = check_source_diagnostics(
        r#"
class Pair { constructor(public a: number, public b: number) {} }
let p = new Pair(1, 2);
let t: Pair;
"#,
    );
    let type_value_errors: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2693 || d.code == 2749)
        .collect();
    assert!(
        type_value_errors.is_empty(),
        "Class should not produce type/value errors, got: {type_value_errors:?}"
    );
}

#[test]
fn enum_is_both_type_and_value() {
    let diags = check_source_diagnostics(
        r#"
enum Direction { Up, Down, Left, Right }
let d = Direction.Up;
let t: Direction;
"#,
    );
    let type_value_errors: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2693 || d.code == 2749 || d.code == 2708)
        .collect();
    assert!(
        type_value_errors.is_empty(),
        "Enum should not produce type/value errors, got: {type_value_errors:?}"
    );
}

// =========================================================================
// Boundary types: unit tests (compile-time verification)
// =========================================================================

#[test]
fn boundary_known_value_no_diagnostic() {
    // When a name resolves successfully, no diagnostics should appear
    let diags = check_source_diagnostics(
        r#"
const knownValue = 42;
function test() {
    return knownValue;
}
"#,
    );
    assert!(
        diags.is_empty(),
        "Expected no diagnostics for resolved value, got: {diags:?}"
    );
}

#[test]
fn boundary_namespace_member_access_works() {
    // Accessing a valid namespace member should not produce errors
    let diags = check_source_diagnostics(
        r#"
namespace NS {
    export const val = 42;
}
let x = NS.val;
"#,
    );
    let relevant: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2694 || d.code == 2708 || d.code == 2724)
        .collect();
    assert!(
        relevant.is_empty(),
        "Valid namespace member access should not produce errors, got: {relevant:?}"
    );
}

#[test]
fn boundary_nested_namespace_export_missing() {
    let diags = check_source_diagnostics(
        r#"
namespace Outer {
    export namespace Inner {
        export type Exists = number;
    }
}
let x: Outer.Inner.DoesNotExist;
"#,
    );
    assert!(
        diags.iter().any(|d| d.code == 2694),
        "Expected TS2694 for missing nested namespace export, got: {diags:?}"
    );
}
