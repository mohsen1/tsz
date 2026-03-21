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

use tsz_checker::context::CheckerOptions;
use tsz_checker::diagnostics::Diagnostic;

fn check(source: &str) -> Vec<Diagnostic> {
    let mut parser =
        tsz_parser::parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = tsz_binder::BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = tsz_solver::TypeInterner::new();
    let options = CheckerOptions::default();

    let mut checker = tsz_checker::state::CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        options,
    );
    checker.ctx.report_unresolved_imports = true;
    checker.check_source_file(root);
    checker.ctx.diagnostics.clone()
}

// =========================================================================
// TS2693: Type used as value (routes through boundary)
// =========================================================================

#[test]
fn ts2693_type_alias_used_as_value() {
    let diags = check(
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
    let diags = check(
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
    let diags = check(
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
    let diags = check(
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
    let diags = check(
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
    let diags = check(
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
    let diags = check(
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
    let diags = check(
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
    let diags = check(
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
    let diags = check(
        r#"
namespace MyNs {
    export type MyType = string;
    export type MyOtherType = number;
}
let x: MyNs.MyTyp;
"#,
    );
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
    let diags = check(
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
    let diags = check(
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
// Phase 2: report_wrong_meaning routing tests
// =========================================================================

#[test]
fn phase2_type_alias_in_value_routes_through_report_wrong_meaning() {
    let diags = check(
        r#"
type StringAlias = string;
let v = StringAlias;
"#,
    );
    let ts2693_count = diags.iter().filter(|d| d.code == 2693).count();
    assert!(
        ts2693_count == 1,
        "Expected exactly 1 TS2693, got {ts2693_count}: {diags:?}"
    );
}

#[test]
fn phase2_namespace_as_value_routes_through_report_wrong_meaning() {
    let diags = check(
        r#"
namespace PureTypeNs {
    export interface I {}
}
let v = PureTypeNs;
"#,
    );
    let ts2708_count = diags.iter().filter(|d| d.code == 2708).count();
    assert!(
        ts2708_count == 1,
        "Expected exactly 1 TS2708, got {ts2708_count}: {diags:?}"
    );
}

#[test]
fn phase2_value_only_in_type_routes_through_boundary_in_type_literal() {
    let diags = check(
        r#"
function myFunc() { return 1; }
type T = { x: myFunc };
"#,
    );
    assert!(
        diags.iter().any(|d| d.code == 2749),
        "Expected TS2749 for function used as type in type literal, got: {diags:?}"
    );
}

#[test]
fn phase2_value_only_in_qualified_type_routes_through_boundary() {
    let diags = check(
        r#"
namespace NS {
    export const val = 42;
}
let x: NS.val;
"#,
    );
    assert!(
        diags.iter().any(|d| d.code == 2749 || d.code == 2694),
        "Expected TS2749 or TS2694 for value-only qualified name, got: {diags:?}"
    );
}

// =========================================================================
// Phase 2: Type-position suggestion collection through boundary
// =========================================================================

#[test]
fn phase2_type_position_not_found_goes_through_boundary() {
    let diags = check(
        r#"
let x: UnknownTypeName;
"#,
    );
    assert!(
        diags.iter().any(|d| d.code == 2304 || d.code == 2552),
        "Expected TS2304/TS2552 for unknown type name, got: {diags:?}"
    );
}

// =========================================================================
// Phase 2: No duplicate diagnostics after migration
// =========================================================================

#[test]
fn phase2_no_double_diagnostic_for_interface_as_value() {
    let diags = check(
        r#"
interface IFoo { x: number; }
let v = IFoo;
"#,
    );
    let ts2693_count = diags.iter().filter(|d| d.code == 2693).count();
    assert!(
        ts2693_count <= 1,
        "Should emit at most 1 TS2693, got {ts2693_count}: {diags:?}"
    );
}

#[test]
fn phase2_no_double_diagnostic_for_namespace_as_value() {
    let diags = check(
        r#"
namespace NS {
    export type T = string;
}
let v = NS;
"#,
    );
    let ts2708_count = diags.iter().filter(|d| d.code == 2708).count();
    assert!(
        ts2708_count <= 1,
        "Should emit at most 1 TS2708, got {ts2708_count}: {diags:?}"
    );
}

#[test]
fn phase2_no_double_diagnostic_for_value_as_type() {
    let diags = check(
        r#"
const myVal = 42;
let x: myVal;
"#,
    );
    let ts2749_count = diags.iter().filter(|d| d.code == 2749).count();
    assert!(
        ts2749_count <= 1,
        "Should emit at most 1 TS2749, got {ts2749_count}: {diags:?}"
    );
}

// =========================================================================
// Phase 2: boundary_known_value_no_diagnostic
// =========================================================================

#[test]
fn boundary_known_value_no_diagnostic() {
    let diags = check(
        r#"
const knownValue = 42;
function test() {
    return knownValue;
}
"#,
    );
    let relevant: Vec<_> = diags
        .iter()
        .filter(|d| d.code == 2304 || d.code == 2693 || d.code == 2749)
        .collect();
    assert!(
        relevant.is_empty(),
        "Expected no name resolution errors for known value, got: {relevant:?}"
    );
}

#[test]
fn boundary_namespace_member_access_works() {
    let diags = check(
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
    let diags = check(
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
