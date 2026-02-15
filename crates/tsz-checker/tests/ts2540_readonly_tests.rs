//! Tests for TS2540 readonly property assignment errors
//!
//! Verifies that assigning to readonly properties emits TS2540.

use crate::CheckerState;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn has_error_with_code(source: &str, code: u32) -> bool {
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

    checker.ctx.diagnostics.iter().any(|d| d.code == code)
}

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
        .filter(|d| d.code != 2318) // Filter global type errors
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

// =========================================================================
// Class readonly property tests
// =========================================================================

#[test]
fn test_readonly_class_property_assignment() {
    let source = r#"
class C {
    readonly x: number = 1;
}
const c = new C();
c.x = 10;
"#;
    assert!(
        has_error_with_code(source, 2540),
        "Should emit TS2540 for assigning to readonly class property"
    );
}

#[test]
fn test_non_readonly_class_property_assignment_ok() {
    let source = r#"
class C {
    y: number = 2;
}
const c = new C();
c.y = 20;
"#;
    assert!(
        !has_error_with_code(source, 2540),
        "Should NOT emit TS2540 for non-readonly property"
    );
}

#[test]
fn test_readonly_class_mixed_properties() {
    // Class with both readonly and mutable properties
    let source = r#"
class C {
    readonly ro: string = "hello";
    mut_prop: string = "world";
}
const c = new C();
c.ro = "new";
c.mut_prop = "ok";
"#;
    let diags = get_diagnostics(source);
    let ts2540_count = diags.iter().filter(|(code, _)| *code == 2540).count();
    assert_eq!(
        ts2540_count, 1,
        "Should emit exactly 1 TS2540 (for ro), got: {diags:?}"
    );
}

// =========================================================================
// Interface readonly property tests
// =========================================================================

#[test]
fn test_readonly_interface_property() {
    let source = r#"
interface I {
    readonly x: number;
}
declare const obj: I;
obj.x = 10;
"#;
    assert!(
        has_error_with_code(source, 2540),
        "Should emit TS2540 for assigning to readonly interface property"
    );
}

#[test]
fn test_non_readonly_interface_property_ok() {
    let source = r#"
interface I {
    x: number;
}
declare const obj: I;
obj.x = 10;
"#;
    assert!(
        !has_error_with_code(source, 2540),
        "Should NOT emit TS2540 for mutable interface property"
    );
}

// =========================================================================
// Const variable tests
// =========================================================================

#[test]
fn test_const_variable_assignment() {
    // TS2588: Cannot assign to 'x' because it is a constant
    let source = r#"
const x = 10;
x = 20;
"#;
    assert!(
        has_error_with_code(source, 2588),
        "Should emit TS2588 for assigning to const variable"
    );
}

// =========================================================================
// Namespace const export tests
// =========================================================================

#[test]
fn test_namespace_const_export_readonly() {
    let source = r#"
namespace M {
    export const x = 0;
}
M.x = 1;
"#;
    assert!(
        has_error_with_code(source, 2540),
        "Should emit TS2540 for assigning to namespace const export"
    );
}

// =========================================================================
// Interface mixed readonly tests
// =========================================================================

#[test]
fn test_readonly_interface_mixed_properties() {
    // Interface with both readonly and mutable properties
    let source = r#"
interface I {
    readonly ro: string;
    mut_prop: string;
}
declare const obj: I;
obj.ro = "new";
obj.mut_prop = "ok";
"#;
    let diags = get_diagnostics(source);
    let ts2540_count = diags.iter().filter(|(code, _)| *code == 2540).count();
    assert_eq!(
        ts2540_count, 1,
        "Should emit exactly 1 TS2540 (for ro), got: {diags:?}"
    );
}

#[test]
fn test_readonly_interface_multiple_readonly_props() {
    // Interface with multiple readonly properties
    let source = r#"
interface I {
    readonly a: number;
    readonly b: string;
    c: boolean;
}
declare const obj: I;
obj.a = 1;
obj.b = "x";
obj.c = true;
"#;
    let diags = get_diagnostics(source);
    let ts2540_count = diags.iter().filter(|(code, _)| *code == 2540).count();
    assert_eq!(
        ts2540_count, 2,
        "Should emit 2 TS2540 errors (for a and b), got: {diags:?}"
    );
}

// =========================================================================
// Namespace let export should be mutable
// =========================================================================

#[test]
fn test_namespace_let_export_mutable() {
    let source = r#"
namespace M {
    export let x = 0;
}
M.x = 1;
"#;
    assert!(
        !has_error_with_code(source, 2540),
        "Should NOT emit TS2540 for namespace let export"
    );
}

// =========================================================================
// Element access readonly tests
// =========================================================================

#[test]
fn test_readonly_interface_element_access() {
    // obj["x"] should also be caught as readonly
    let source = r#"
interface I {
    readonly x: number;
}
declare const obj: I;
obj["x"] = 10;
"#;
    assert!(
        has_error_with_code(source, 2540),
        "Should emit TS2540 for element access to readonly interface property"
    );
}

#[test]
fn test_readonly_class_compound_assignment() {
    // Compound assignments (+=, -=, etc.) should also be caught
    let source = r#"
class C {
    readonly x: number = 1;
}
const c = new C();
c.x += 10;
"#;
    assert!(
        has_error_with_code(source, 2540),
        "Should emit TS2540 for compound assignment to readonly class property"
    );
}

#[test]
fn test_readonly_class_increment() {
    // Increment/decrement should also be caught
    let source = r#"
class C {
    readonly x: number = 1;
}
const c = new C();
c.x++;
"#;
    assert!(
        has_error_with_code(source, 2540),
        "Should emit TS2540 for increment on readonly class property"
    );
}
