//! Tests for TS2411: Property type not assignable to index signature type
//!
//! Verifies that getter/setter accessors are checked against index signatures.

use crate::test_utils::check_source_code_messages;

fn get_diagnostics(source: &str) -> Vec<(u32, String)> {
    check_source_code_messages(source)
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

#[test]
fn test_ts2411_method_overload_displays_merged_signatures() {
    // When an interface method has multiple overload signatures, the TS2411
    // message must render the property's type as `{ (): any; (): any; }`
    // (matching tsc) instead of just the first signature's `() => any`.
    // Regression test for interfaceMemberValidation.ts.
    let source = r#"
interface foo {
    bar(): any;
    bar(): any;
    [s: string]: number;
}
"#;
    let diags = get_diagnostics(source);
    let ts2411 = diags
        .iter()
        .find(|d| d.0 == 2411)
        .expect("expected TS2411 for `bar` overloads vs string index");
    assert!(
        ts2411.1.contains("{ (): any; (): any; }"),
        "TS2411 must render merged overload type as `{{ (): any; (): any; }}`, got: {}",
        ts2411.1
    );
}
