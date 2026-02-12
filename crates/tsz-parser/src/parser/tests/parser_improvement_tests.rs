//! Tests for parser improvements to reduce TS1005 and TS2300 false positives

use crate::parser::ParserState;

#[test]
fn test_index_signature_with_modifier_emits_ts1071() {
    // Index signature with public modifier should emit TS1071, not TS1184
    // TS1071: '{0}' modifier cannot appear on an index signature.
    // TS1184: Modifiers cannot appear here. (too generic)
    let source = r#"
interface I {
  public [a: string]: number;
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();

    // Should emit TS1071 for modifier on index signature
    let ts1071_count = diagnostics.iter().filter(|d| d.code == 1071).count();
    assert_eq!(
        ts1071_count, 1,
        "Expected 1 TS1071 error for modifier on index signature, got {}",
        ts1071_count
    );

    // Should NOT emit the generic TS1184
    let ts1184_count = diagnostics.iter().filter(|d| d.code == 1184).count();
    assert_eq!(
        ts1184_count, 0,
        "Expected no TS1184 errors (should be TS1071 instead), got {}",
        ts1184_count
    );
}

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
    let offset = source.find("function").expect("pattern should exist in source") as u32;

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
    let offset = source.find("const b").expect("pattern should exist in source") as u32;

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
// Conditional Type ASI Tests
// =============================================================================

#[test]
fn test_interface_extends_property_with_asi() {
    // 'extends' as a property name in interface with ASI (no semicolons)
    // Should NOT parse as conditional type
    let source = r#"
interface JSONSchema4 {
  a?: number
  extends?: string | string[]
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diags = parser.get_diagnostics();
    assert!(
        diags.is_empty(),
        "Expected no parser errors for 'extends' property with ASI, got {:?}",
        diags
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

// =============================================================================
// Import Type Tests
// =============================================================================

#[test]
fn test_typeof_import_with_member_access() {
    // typeof import("...").A.foo should parse without TS1005
    // This is a valid TypeScript syntax for accessing static members
    let source = r#"
export const foo: typeof import("./a").A.foo;
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    // Should not emit TS1005 for member access after import()
    let ts1005_count = parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code == 1005)
        .count();
    assert_eq!(
        ts1005_count, 0,
        "Expected no TS1005 errors for typeof import with member access, got {}",
        ts1005_count
    );

    // Should have no errors at all
    assert!(
        parser.get_diagnostics().is_empty(),
        "Expected no parser errors for typeof import with member access, got {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_typeof_import_with_nested_member_access() {
    // typeof import("...").A.B.C should parse correctly
    let source = r#"
export const foo: typeof import("./module").A.B.C;
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    // Should not emit any errors for nested member access after import()
    assert!(
        parser.get_diagnostics().is_empty(),
        "Expected no parser errors for typeof import with nested member access, got {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_typeof_import_without_member_access() {
    // typeof import("...") without member access should still work
    let source = r#"
export const foo: typeof import("./module");
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    // Should not emit any errors
    assert!(
        parser.get_diagnostics().is_empty(),
        "Expected no parser errors for typeof import without member access, got {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_import_type_without_typeof() {
    // import("...").Type should parse without typeof
    let source = r#"
export const a: import("./test1").T = null as any;
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    // Should not emit parse errors
    let ts1005_count = parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code == 1005)
        .count();
    let ts1109_count = parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code == 1109)
        .count();
    let ts1359_count = parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code == 1359)
        .count();

    assert_eq!(
        ts1005_count, 0,
        "Expected no TS1005 errors for import type, got {}",
        ts1005_count
    );
    assert_eq!(
        ts1109_count, 0,
        "Expected no TS1109 errors for import type, got {}",
        ts1109_count
    );
    assert_eq!(
        ts1359_count, 0,
        "Expected no TS1359 errors for import type, got {}",
        ts1359_count
    );
}

#[test]
fn test_import_type_with_member_access() {
    // import("...").Type.SubType should parse correctly
    let source = r#"
export const a: import("./test1").T.U = null as any;
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    // Should not emit parse errors
    assert!(
        parser.get_diagnostics().iter().all(|d| d.code >= 2000),
        "Expected no parser errors (1xxx) for import type with member access, got {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_import_type_with_generic_arguments() {
    // import("...").Type<T> should parse correctly
    let source = r#"
export const a: import("./test1").T<typeof import("./test2").theme> = null as any;
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    // Should not emit parse errors
    let parse_errors = parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code < 2000)
        .count();
    assert_eq!(
        parse_errors,
        0,
        "Expected no parser errors for import type with generics, got {:?}",
        parser.get_diagnostics()
    );
}

// =============================================================================
// Tuple Type Tests
// =============================================================================

#[test]
fn test_optional_tuple_element() {
    // [T?] should parse correctly without TS1005/TS1110
    let source = r#"
interface Buzz { id: number; }
type T = [Buzz?];
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    // Should not emit TS1005 or TS1110 for optional tuple element
    let ts1005_count = parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code == 1005)
        .count();
    let ts1110_count = parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code == 1110)
        .count();

    assert_eq!(
        ts1005_count, 0,
        "Expected no TS1005 errors for optional tuple element, got {}",
        ts1005_count
    );
    assert_eq!(
        ts1110_count, 0,
        "Expected no TS1110 errors for optional tuple element, got {}",
        ts1110_count
    );
}

#[test]
fn test_readonly_optional_tuple_element() {
    // readonly [T?] should parse correctly
    let source = r#"
interface Buzz { id: number; }
type T = readonly [Buzz?];
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    // Should not emit any parser errors
    assert!(
        parser.get_diagnostics().is_empty(),
        "Expected no parser errors for readonly optional tuple, got {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_named_tuple_element_still_works() {
    // name?: T should still parse as a named tuple element
    let source = r#"
type T = [name?: string];
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    // Should not emit any parser errors
    assert!(
        parser.get_diagnostics().is_empty(),
        "Expected no parser errors for named optional tuple element, got {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_mixed_tuple_elements() {
    // Mix of optional, named, and rest elements should work
    let source = r#"
interface A { a: number; }
interface B { b: string; }
type T = [A?, name: B, ...rest: string[]];
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    // Should not emit any parser errors
    assert!(
        parser.get_diagnostics().is_empty(),
        "Expected no parser errors for mixed tuple elements, got {:?}",
        parser.get_diagnostics()
    );
}

#[test]
fn test_argument_list_recovery_on_return_keyword() {
    let source = r#"
const x = fn(
  return
);
const y = 1;
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let ts1109_count = diagnostics.iter().filter(|d| d.code == 1109).count();
    let ts1005_count = diagnostics.iter().filter(|d| d.code == 1005).count();

    assert!(
        ts1109_count >= 1,
        "Expected at least 1 TS1109 for malformed argument list, got diagnostics: {diagnostics:?}"
    );
    assert!(
        ts1005_count <= 2,
        "Expected limited TS1005 cascade for malformed argument list, got {} diagnostics: {diagnostics:?}",
        ts1005_count
    );
}

#[test]
fn test_invalid_unicode_escape_in_var_no_extra_semicolon_error() {
    let source = r#"var arg\uxxxx"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let ts1127_count = diagnostics.iter().filter(|d| d.code == 1127).count();
    let ts1005_count = diagnostics.iter().filter(|d| d.code == 1005).count();

    assert!(
        ts1127_count >= 1,
        "Expected TS1127 for invalid unicode escape, got diagnostics: {diagnostics:?}"
    );
    assert_eq!(
        ts1005_count, 0,
        "Expected no extra TS1005 for invalid unicode escape, got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_invalid_unicode_escape_as_variable_name_no_var_decl_cascade() {
    let source = r#"var \u0031a;"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();

    let diagnostics = parser.get_diagnostics();
    let ts1127_count = diagnostics.iter().filter(|d| d.code == 1127).count();
    let ts1123_count = diagnostics.iter().filter(|d| d.code == 1123).count();
    let ts1134_count = diagnostics.iter().filter(|d| d.code == 1134).count();

    assert!(
        ts1127_count >= 1,
        "Expected TS1127 for invalid unicode escape, got diagnostics: {diagnostics:?}"
    );
    assert_eq!(
        ts1123_count, 0,
        "Expected no TS1123 variable declaration cascade, got diagnostics: {diagnostics:?}"
    );
    assert_eq!(
        ts1134_count, 0,
        "Expected no TS1134 variable declaration cascade, got diagnostics: {diagnostics:?}"
    );
}
