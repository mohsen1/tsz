//! Tests for parser improvements to reduce TS1005 and TS2300 false positives

use crate::parser::ParserState;

#[test]
fn test_arrow_function_with_line_break_no_false_positive() {
    // Arrow function where => is missing but there's a line break
    // Should be more permissive to avoid false positives
    let source = r#"
const fn = (a: number, b: string)
=> a + b;
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    // Should not have cascading TS1005 errors
    let ts1005_count = parser.diagnostics.iter().filter(|d| d.code == 1005).count();
    assert!(
        ts1005_count <= 1,
        "Expected at most 1 TS1005 error, got {}",
        ts1005_count
    );
}

#[test]
fn test_parameters_with_line_break_no_comma() {
    // Function parameters without comma but with line break
    // Should be more permissive to avoid false positives
    let source = r#"
function foo(
    a: number
    b: string
) {
    return a + b;
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    // Should not emit TS1005 for missing comma when there's a line break
    let ts1005_count = parser.diagnostics.iter().filter(|d| d.code == 1005).count();
    assert!(
        ts1005_count <= 1,
        "Expected at most 1 TS1005 error, got {}",
        ts1005_count
    );
}

#[test]
fn test_interface_merging_no_duplicate() {
    // Interface merging should not emit TS2300
    let source = r#"
interface Foo {
    a: number;
}
interface Foo {
    b: string;
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    // Should not emit TS2300 for interface merging
    let ts2300_count = parser.diagnostics.iter().filter(|d| d.code == 2300).count();
    assert_eq!(
        ts2300_count, 0,
        "Expected no TS2300 errors for interface merging, got {}",
        ts2300_count
    );
}

#[test]
fn test_function_overloads_no_duplicate() {
    // Function overloads should not emit TS2300
    let source = r#"
function foo(x: number): void;
function foo(x: string): void;
function foo(x: number | string): void {
    console.log(x);
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    // Should not emit TS2300 for function overloads
    let ts2300_count = parser.diagnostics.iter().filter(|d| d.code == 2300).count();
    assert_eq!(
        ts2300_count, 0,
        "Expected no TS2300 errors for function overloads, got {}",
        ts2300_count
    );
}

#[test]
fn test_namespace_function_merging_no_duplicate() {
    // Namespace + function merging should not emit TS2300
    let source = r#"
namespace Utils {
    export function helper(): void {
        console.log("helper");
    }
}
function Utils() {
    console.log("constructor");
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    // Should not emit TS2300 for namespace + function merging
    let ts2300_count = parser.diagnostics.iter().filter(|d| d.code == 2300).count();
    assert_eq!(
        ts2300_count, 0,
        "Expected no TS2300 errors for namespace+function merging, got {}",
        ts2300_count
    );
}

#[test]
fn test_asi_after_return() {
    // ASI (automatic semicolon insertion) should work after return
    let source = r#"
function foo() {
    return
    42;
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    // Should not emit TS1005 for missing semicolon after return with line break
    let ts1005_count = parser.diagnostics.iter().filter(|d| d.code == 1005).count();
    assert_eq!(
        ts1005_count, 0,
        "Expected no TS1005 errors for ASI after return, got {}",
        ts1005_count
    );
}

#[test]
fn test_trailing_comma_in_object_literal() {
    // Trailing commas should be allowed in object literals
    let source = r#"
const obj = {
    a: 1,
    b: 2,
};
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    // Should not emit any errors for trailing comma
    assert!(
        parser.diagnostics.is_empty(),
        "Expected no errors for trailing comma in object literal, got {:?}",
        parser.diagnostics
    );
}

#[test]
fn test_trailing_comma_in_array_literal() {
    // Trailing commas should be allowed in array literals
    let source = r#"
const arr = [
    1,
    2,
    3,
];
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    // Should not emit any errors for trailing comma
    assert!(
        parser.diagnostics.is_empty(),
        "Expected no errors for trailing comma in array literal, got {:?}",
        parser.diagnostics
    );
}

#[test]
fn test_trailing_comma_in_parameters() {
    // Trailing commas should be allowed in function parameters
    let source = r#"
function foo(
    a: number,
    b: string,
) {
    return a + b;
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    // Should not emit any errors for trailing comma in parameters
    assert!(
        parser.diagnostics.is_empty(),
        "Expected no errors for trailing comma in parameters, got {:?}",
        parser.diagnostics
    );
}
