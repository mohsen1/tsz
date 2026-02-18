//! Tests for TS2411: Property type not assignable to index signature type
//!
//! Verifies that getter/setter accessors are checked against index signatures.

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

fn has_error_with_code(source: &str, code: u32) -> bool {
    get_diagnostics(source).iter().any(|d| d.0 == code)
}

// =========================================================================
// Getter without type annotation vs string index signature
// =========================================================================

#[test]
fn test_getter_no_annotation_string_index_class() {
    // Getter returns boolean, string index requires string
    let source = r#"
class Foo {
    [key: string]: string;
    get bar() { return true; }
}
"#;
    assert!(
        has_error_with_code(source, 2411),
        "Should emit TS2411 for getter returning boolean vs string index"
    );
}

#[test]
fn test_getter_no_annotation_string_index_interface() {
    let source = r#"
interface Foo {
    [key: string]: string;
    get bar(): boolean;
}
"#;
    // Interface getters always have type annotation in syntax, so this uses the annotation path
    assert!(
        has_error_with_code(source, 2411),
        "Should emit TS2411 for interface getter with mismatched return type"
    );
}

// =========================================================================
// Getter without type annotation vs number index signature
// =========================================================================

#[test]
fn test_getter_no_annotation_number_index() {
    let source = r#"
class Foo {
    [key: number]: string;
    get 0() { return 42; }
}
"#;
    assert!(
        has_error_with_code(source, 2411),
        "Should emit TS2411 for numeric getter returning number vs number index string"
    );
}

// =========================================================================
// Getter with explicit type annotation (should still work)
// =========================================================================

#[test]
fn test_getter_with_annotation_mismatch() {
    let source = r#"
class Foo {
    [key: string]: string;
    get bar(): number { return 42; }
}
"#;
    assert!(
        has_error_with_code(source, 2411),
        "Should emit TS2411 for getter with explicit return type mismatch"
    );
}

#[test]
fn test_getter_with_annotation_compatible() {
    let source = r#"
class Foo {
    [key: string]: string;
    get bar(): string { return "hello"; }
}
"#;
    assert!(
        !has_error_with_code(source, 2411),
        "Should NOT emit TS2411 when getter return type matches index signature"
    );
}

// =========================================================================
// Getter without annotation, compatible return type
// =========================================================================

#[test]
fn test_getter_no_annotation_compatible() {
    let source = r#"
class Foo {
    [key: string]: string;
    get bar() { return "hello"; }
}
"#;
    assert!(
        !has_error_with_code(source, 2411),
        "Should NOT emit TS2411 when inferred getter return type matches index signature"
    );
}

// =========================================================================
// Setter parameter type vs index signature
// =========================================================================

#[test]
fn test_setter_with_annotation_mismatch() {
    let source = r#"
class Foo {
    [key: string]: string;
    set bar(val: number) {}
}
"#;
    assert!(
        has_error_with_code(source, 2411),
        "Should emit TS2411 for setter with mismatched parameter type"
    );
}

#[test]
fn test_setter_with_annotation_compatible() {
    let source = r#"
class Foo {
    [key: string]: string;
    set bar(val: string) {}
}
"#;
    assert!(
        !has_error_with_code(source, 2411),
        "Should NOT emit TS2411 when setter parameter type matches index signature"
    );
}

// =========================================================================
// Method signature vs index signature (interface)
// =========================================================================

#[test]
fn test_method_signature_vs_index_signature() {
    // Method bar():any has function type () => any, which is not assignable to number
    let source = r#"
interface Foo {
    bar(): any;
    [s: string]: number;
}
"#;
    let diags = get_diagnostics(source);
    assert!(
        diags.iter().any(|d| d.0 == 2411),
        "Should emit TS2411 for method signature type not assignable to index, got: {diags:?}"
    );
}

#[test]
fn test_method_declaration_vs_index_signature() {
    // Class method bar():any has function type () => any, not assignable to number
    let source = r#"
class Foo {
    bar(): any { return 1; }
    [s: string]: number;
}
"#;
    let diags = get_diagnostics(source);
    assert!(
        diags.iter().any(|d| d.0 == 2411),
        "Should emit TS2411 for class method type not assignable to index, got: {diags:?}"
    );
}

// =========================================================================
// Type literal (object type) members vs index signature
// =========================================================================

#[test]
fn test_type_literal_property_vs_index_signature() {
    let source = r#"
interface I { k: any; }
var x: { z: I; [s: string]: { x: any; y: any; } };
"#;
    let diags = get_diagnostics(source);
    assert!(
        diags.iter().any(|d| d.0 == 2411),
        "Should emit TS2411 for type literal property not assignable to index, got: {diags:?}"
    );
}

// Note: Inherited member vs index signature is tested via conformance tests
// (e.g. inheritedMembersAndIndexSignaturesFromDifferentBases.ts) since it
// requires full lib type resolution that unit tests don't provide.
