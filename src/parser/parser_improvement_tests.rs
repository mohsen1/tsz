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
    let ts1005_count = parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code == 1005)
        .count();
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
    let ts1005_count = parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code == 1005)
        .count();
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
    let ts2300_count = parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code == 2300)
        .count();
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
    let ts2300_count = parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code == 2300)
        .count();
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
    let ts2300_count = parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code == 2300)
        .count();
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
    let ts1005_count = parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code == 1005)
        .count();
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
        parser.get_diagnostics().is_empty(),
        "Expected no errors for trailing comma in object literal, got {:?}",
        parser.get_diagnostics()
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
        parser.get_diagnostics().is_empty(),
        "Expected no errors for trailing comma in array literal, got {:?}",
        parser.get_diagnostics()
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
        parser.get_diagnostics().is_empty(),
        "Expected no errors for trailing comma in parameters, got {:?}",
        parser.get_diagnostics()
    );
}

// =============================================================================
// Primitive Type Keywords Tests
// =============================================================================

#[test]
fn test_void_return_type() {
    // void return type should be parsed correctly without TS1110/TS1109 errors
    let source = r#"
declare function fn(arg0: boolean): void;
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    // Should not emit any parser errors for void return type
    assert!(
        parser.get_diagnostics().is_empty(),
        "Expected no parser errors for void return type, got {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_primitive_type_keywords() {
    // All primitive type keywords should be parsed correctly
    let source = r#"
declare function fn1(): void;
declare function fn2(): string;
declare function fn3(): number;
declare function fn4(): boolean;
declare function fn5(): symbol;
declare function fn6(): bigint;
declare function fn7(): any;
declare function fn8(): unknown;
declare function fn9(): never;
declare function fn10(): null;
declare function fn11(): undefined;
declare function fn12(): object;
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    // Should not emit any parser errors for primitive type keywords
    assert!(
        parser.get_diagnostics().is_empty(),
        "Expected no parser errors for primitive type keywords, got {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_primitive_types_in_type_aliases() {
    // Primitive type keywords should work in type aliases
    let source = r#"
type T1 = void;
type T2 = string;
type T3 = number;
type T4 = boolean;
type T5 = any;
type T6 = unknown;
type T7 = never;
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    // Should not emit any parser errors
    assert!(
        parser.get_diagnostics().is_empty(),
        "Expected no parser errors for primitive types in type aliases, got {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_primitive_types_in_parameters() {
    // Primitive type keywords should work in parameter types
    let source = r#"
declare function fn(a: void, b: string, c: number): boolean;
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    // Should not emit any parser errors
    assert!(
        parser.get_diagnostics().is_empty(),
        "Expected no parser errors for primitive types in parameters, got {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_primitive_types_in_arrow_functions() {
    // Primitive type keywords should work in arrow function types
    let source = r#"
const arrow1: () => void = () => {};
const arrow2: (x: number) => string = (x) => "";
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    // Should not emit any parser errors
    assert!(
        parser.get_diagnostics().is_empty(),
        "Expected no parser errors for primitive types in arrow functions, got {:?}",
        parser.get_diagnostics()
    );
}

// =============================================================================
// Incremental Parsing Tests
// =============================================================================

#[test]
fn test_incremental_parse_from_middle_of_file() {
    // Test parsing from an offset in the middle of a source file
    let source = r#"const a = 1;
const b = 2;
function foo() {
    return a + b;
}
const c = 3;"#;

    // Parse from the start of "function foo()"
    let offset = source.find("function").unwrap() as u32;

    let mut parser = ParserState::new("test.ts".to_string(), String::new());
    let result = parser.parse_source_file_statements_from_offset(
        "test.ts".to_string(),
        source.to_string(),
        offset,
    );

    // Should have parsed the remaining statements (function and const c)
    assert!(
        result.statements.len() >= 2,
        "Expected at least 2 statements from offset, got {}",
        result.statements.len()
    );

    // Should not produce errors for valid code
    assert!(
        parser.get_diagnostics().is_empty(),
        "Expected no errors for incremental parse, got {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_incremental_parse_from_start() {
    // Test incremental parsing from offset 0 (should be equivalent to full parse)
    let source = r#"const x = 42;
let y = "hello";"#;

    let mut parser = ParserState::new("test.ts".to_string(), String::new());
    let result = parser.parse_source_file_statements_from_offset(
        "test.ts".to_string(),
        source.to_string(),
        0,
    );

    // Should have parsed both statements
    assert_eq!(
        result.statements.len(),
        2,
        "Expected 2 statements, got {}",
        result.statements.len()
    );

    // reparse_start should be 0
    assert_eq!(result.reparse_start, 0);

    // Should not produce errors
    assert!(
        parser.get_diagnostics().is_empty(),
        "Expected no errors, got {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_incremental_parse_from_end() {
    // Test incremental parsing from beyond the end of file
    let source = "const x = 1;";

    let mut parser = ParserState::new("test.ts".to_string(), String::new());
    let result = parser.parse_source_file_statements_from_offset(
        "test.ts".to_string(),
        source.to_string(),
        1000, // Beyond EOF
    );

    // Should handle gracefully - clamped to source length
    assert!(
        result.statements.is_empty(),
        "Expected no statements when starting at EOF"
    );
}

#[test]
fn test_incremental_parse_records_reparse_start() {
    // Test that reparse_start is recorded correctly
    let source = "const a = 1;\nconst b = 2;";
    let offset = 13u32; // Start of "const b"

    let mut parser = ParserState::new("test.ts".to_string(), String::new());
    let result = parser.parse_source_file_statements_from_offset(
        "test.ts".to_string(),
        source.to_string(),
        offset,
    );

    // reparse_start should match the offset we provided
    assert_eq!(
        result.reparse_start, offset,
        "Expected reparse_start to be {}, got {}",
        offset, result.reparse_start
    );
}

#[test]
fn test_incremental_parse_with_syntax_error() {
    // Test incremental parsing recovers from syntax errors
    let source = r#"const a = 1;
const b = ;
const c = 3;"#;

    // Parse from start of "const b = ;" (syntax error)
    let offset = source.find("const b").unwrap() as u32;

    let mut parser = ParserState::new("test.ts".to_string(), String::new());
    let result = parser.parse_source_file_statements_from_offset(
        "test.ts".to_string(),
        source.to_string(),
        offset,
    );

    // Should still parse statements (with recovery)
    assert!(
        result.statements.len() >= 1,
        "Expected at least 1 statement after recovery, got {}",
        result.statements.len()
    );

    // Should produce an error for the syntax issue
    assert!(
        !parser.get_diagnostics().is_empty(),
        "Expected at least one diagnostic for syntax error"
    );
}

// =============================================================================
// Expression Statement Recovery Tests
// =============================================================================

#[test]
fn test_incomplete_binary_expression_recovery() {
    // Test recovery from incomplete binary expression: a +
    let source = r#"const result = a +;
const next = 1;"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    // Should produce an error for missing RHS
    let has_error = !parser.get_diagnostics().is_empty();
    assert!(has_error, "Expected error for incomplete binary expression");

    // Parser should recover and continue parsing
    // The error count should be limited (no cascading errors)
    let error_count = parser.get_diagnostics().len();
    assert!(
        error_count <= 2,
        "Expected at most 2 errors for recovery, got {}",
        error_count
    );
}

#[test]
fn test_incomplete_assignment_recovery() {
    // Test recovery from incomplete assignment: x =
    let source = r#"let x =;
let y = 2;"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    // Should produce an error for missing RHS
    assert!(
        !parser.get_diagnostics().is_empty(),
        "Expected error for incomplete assignment"
    );

    // Parser should recover - not too many errors
    let error_count = parser.get_diagnostics().len();
    assert!(
        error_count <= 2,
        "Expected at most 2 errors after recovery, got {}",
        error_count
    );
}

#[test]
fn test_incomplete_conditional_expression_recovery() {
    // Test recovery from incomplete conditional: a ? b :
    let source = r#"const result = a ? b :;
const next = 1;"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    // Should produce error for missing false branch
    assert!(
        !parser.get_diagnostics().is_empty(),
        "Expected error for incomplete conditional"
    );
}

#[test]
fn test_expression_recovery_at_statement_boundary() {
    // Test that parser properly recovers at statement boundaries
    let source = r#"const a = 1 +
const b = 2;"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    // Should have errors but recover for next statement
    assert!(
        !parser.get_diagnostics().is_empty(),
        "Expected error for incomplete expression"
    );
}

#[test]
fn test_expression_recovery_preserves_valid_code() {
    // Test that valid code after error is still parsed correctly
    let source = r#"const bad = ;
function validFunction() {
    return 42;
}"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    // Should have error for bad assignment
    assert!(
        !parser.get_diagnostics().is_empty(),
        "Expected error for invalid assignment"
    );

    // Error count should be limited
    let error_count = parser.get_diagnostics().len();
    assert!(
        error_count <= 2,
        "Expected limited errors with recovery, got {}",
        error_count
    );
}
