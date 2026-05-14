//! Tests for TS2411: Property type not assignable to index signature type
//!
//! Verifies that getter/setter accessors are checked against index signatures.

use crate::test_utils::{
    check_source, check_source_code_messages as get_diagnostics, diagnostic_code_messages,
};
use tsz_checker::context::CheckerOptions;

fn has_error_with_code(source: &str, code: u32) -> bool {
    get_diagnostics(source).iter().any(|d| d.0 == code)
}

#[test]
fn local_symbol_property_access_computed_name_is_string_keyed() {
    let source = r#"
const Symbol = { tag: "name" } as const;

interface Bag {
    [key: string]: number;
    [Symbol.tag]: string;
}
"#;
    let diagnostics = get_diagnostics(source);
    assert!(
        diagnostics.iter().any(|(code, message)| {
            *code == 2411 && message.contains("[Symbol.tag]") && message.contains("number")
        }),
        "expected TS2411 because local Symbol.tag is a string-keyed computed property, got: {diagnostics:?}"
    );
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

#[test]
fn test_type_literal_union_function_property_vs_index_signature() {
    let source = r#"
function test(arg: string | number, whatever: any) {
  if (typeof arg === "string") {
    const o: { [k: string]: () => typeof arg; x: (() => boolean) | (() => void) } = whatever;
  }
}
"#;
    let diags = get_diagnostics(source);
    assert!(
        diags.iter().any(|d| d.0 == 2411
            && d.1.contains("Property 'x' of type")
            && d.1.contains("index type '() => string | number'")),
        "Should emit TS2411 for union function property not assignable to index, got: {diags:?}"
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

// =========================================================================
// Issue #2871: a local object named `Symbol` must not be treated as the lib
// global `Symbol` when classifying `[Symbol.tag]` as a symbol-keyed property.
// With a `[s: symbol]: number` index signature present, the buggy
// classification routes the `[Symbol.tag]: string` member into the symbol
// index check and emits TS2411 ("string not assignable to 'symbol' index
// type 'number'"). After the fix the local `Symbol` is recognized as a
// shadow, the member is not symbol-keyed, and that diagnostic must not fire.
// =========================================================================

#[test]
fn ts2411_shadowed_symbol_computed_property_is_not_symbol_keyed() {
    let source = r#"
const Symbol = { tag: "name" } as const;

interface Bag {
    [s: symbol]: number;
    [Symbol.tag]: string;
}
"#;
    let ts2411_against_symbol = get_diagnostics(source)
        .into_iter()
        .filter(|d| d.0 == 2411 && d.1.contains("'symbol'"))
        .count();
    assert_eq!(
        ts2411_against_symbol, 0,
        "Expected no symbol-index TS2411 when local Symbol shadows the global, got: {ts2411_against_symbol}"
    );
}

// =========================================================================
// Optional properties in interfaces must be checked as `T | undefined`
// against the index signature (TS2411, issue #6746).
//
// tsc rule: an optional property `prop?: T` has effective type `T | undefined`
// for index-signature compatibility because the property can be absent.
// If `T | undefined` is not assignable to the index value type, TS2411 fires.
// =========================================================================

#[test]
fn ts2411_interface_optional_property_vs_string_index() {
    // `optional?: string` is effectively `string | undefined`.
    // `string | undefined` is not assignable to `string` index value → TS2411.
    let source = r#"
interface WithSpecific {
    [key: string]: string;
    required: string;
    optional?: string;
}
export {};
"#;
    assert!(
        has_error_with_code(source, 2411),
        "Expected TS2411: optional property `string | undefined` not assignable to string index"
    );
}

#[test]
fn ts2411_interface_required_property_no_false_positive() {
    // Required `required: string` IS assignable to `string` index → no TS2411.
    let source = r#"
interface WithSpecific {
    [key: string]: string;
    required: string;
}
export {};
"#;
    assert!(
        !has_error_with_code(source, 2411),
        "Required property should not trigger TS2411 vs matching string index"
    );
}

#[test]
fn ts2411_interface_optional_property_index_includes_undefined() {
    // When the index value type already includes `undefined`, the optional
    // property's `T | undefined` IS assignable → no TS2411.
    let source = r#"
interface WithUndefined {
    [key: string]: string | undefined;
    optional?: string;
}
export {};
"#;
    assert!(
        !has_error_with_code(source, 2411),
        "Optional property should not trigger TS2411 when index type already includes undefined"
    );
}

#[test]
fn ts2411_interface_optional_property_exact_optional_no_undefined_widening() {
    let source = r#"
interface ExactOptional {
    [key: string]: string;
    optional?: string;
}
export {};
"#;
    let diagnostics = diagnostic_code_messages(check_source(
        source,
        "test.ts",
        CheckerOptions {
            exact_optional_property_types: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
    ));
    assert!(
        !diagnostics.iter().any(|d| d.0 == 2411),
        "Exact optional property types should check the present value type without adding undefined, got: {diagnostics:?}"
    );
}

#[test]
fn ts2411_interface_optional_number_index() {
    // Optional numeric property vs number index signature.
    let source = r#"
interface NumericIndex {
    [idx: number]: string;
    0?: string;
}
export {};
"#;
    assert!(
        has_error_with_code(source, 2411),
        "Expected TS2411 for optional numeric property vs number index"
    );
}

#[test]
fn ts2411_class_optional_property_vs_string_index() {
    // Class optional property must also include `undefined` for the check.
    let source = r#"
class MyClass {
    [key: string]: string;
    optional?: string;
}
"#;
    assert!(
        has_error_with_code(source, 2411),
        "Expected TS2411 for class optional property vs string index"
    );
}

#[test]
fn ts2411_interface_optional_property_renamed_key_var() {
    // Rename the index-signature iteration variable to prove no hardcoding.
    let source = r#"
interface Renamed {
    [x: string]: string;
    prop?: string;
}
export {};
"#;
    assert!(
        has_error_with_code(source, 2411),
        "Expected TS2411 regardless of the index-signature parameter name"
    );
}
