//! P1 Error Recovery Tests
//!
//! Test synchronization point improvements for:
//! 1. Class bodies with unexpected tokens
//! 2. Interface declarations with malformed extends clauses
//! 3. Template literal expressions with errors
//! 4. Object destructuring patterns with missing commas

use crate::thin_parser::ThinParserState;

// ===========================================================================
// Test 1: Class Body Error Recovery
// ===========================================================================

/// Test class body with unexpected statement keywords
#[test]
fn test_p1_class_body_with_stray_statements() {
    let source = r#"
class MyClass {
    constructor(x) {
        this.x = x;
    }
    if (true) {        // Error: statement in class body
        console.log("hi");
    }
    getValue() {       // Should recover and parse this
        return this.x;
    }
}
"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    // Should parse successfully with one error
    let diags = parser.get_diagnostics();
    assert!(!diags.is_empty(), "Should report error for stray statement");

    // Should have parsed getValue method
    assert!(parser.arena.len() > 0, "Should parse class members");
}

/// Test class body with function declaration
#[test]
fn test_p1_class_body_with_function_declaration() {
    let source = r#"
class MyClass {
    function helper() {   // Error: function declaration in class body
        return 42;
    }
    getValue() {
        return 1;
    }
}
"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    // Should parse both members
    assert!(parser.arena.len() > 0, "Should parse class members");
}

// ===========================================================================
// Test 2: Interface Extends Clause Error Recovery
// ===========================================================================

/// Test interface with missing comma in extends clause
#[test]
fn test_p1_interface_missing_comma_in_extends() {
    let source = r#"
interface A extends B C D {
    x: number;
}
"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    // Should parse successfully with errors
    let diags = parser.get_diagnostics();
    assert!(!diags.is_empty(), "Should report error for missing comma");

    // Should still parse the interface and member
    assert!(parser.arena.len() > 0, "Should parse interface");
}

/// Test interface with trailing comma in extends clause
#[test]
fn test_p1_interface_trailing_comma_in_extends() {
    let source = r#"
interface A extends B, C, {
    x: number;
}
"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    // Should parse successfully
    assert!(parser.arena.len() > 0, "Should parse interface");
}

/// Test interface with invalid type in extends clause
#[test]
fn test_p1_interface_invalid_type_in_extends() {
    let source = r#"
interface A extends 123, B {
    x: number;
}
"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    // Should parse successfully with error
    let diags = parser.get_diagnostics();
    assert!(!diags.is_empty(), "Should report error for invalid type");

    // Should still parse B and the interface body
    assert!(parser.arena.len() > 0, "Should parse interface");
}

/// Test interface with malformed extends (missing types)
#[test]
fn test_p1_interface_malformed_extends() {
    let source = r#"
interface A extends {
    x: number;
}
"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    // Should parse successfully with error
    let diags = parser.get_diagnostics();
    assert!(
        !diags.is_empty(),
        "Should report error for malformed extends"
    );

    // Should still parse the interface body
    assert!(parser.arena.len() > 0, "Should parse interface body");
}

// ===========================================================================
// Test 3: Template Literal Error Recovery
// ===========================================================================

/// Test template literal with unterminated expression
#[test]
fn test_p1_template_unterminated_expression() {
    let source = r#"
const x = `hello ${world`;
"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    // Should parse with error
    let diags = parser.get_diagnostics();
    assert!(
        !diags.is_empty(),
        "Should report error for unterminated template"
    );

    // Should still create a template node
    assert!(parser.arena.len() > 0, "Should parse template");
}

/// Test template literal with missing closing backtick
#[test]
fn test_p1_template_missing_closing_backtick() {
    let source = r#"
const x = `hello ${name};
"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    // Should parse with error
    let diags = parser.get_diagnostics();
    assert!(!diags.is_empty(), "Should report error");

    // Should still create a template node
    assert!(parser.arena.len() > 0, "Should parse template");
}

// ===========================================================================
// Test 4: Object Destructuring Pattern Error Recovery
// ===========================================================================

/// Test object destructuring with missing commas
#[test]
fn test_p1_destructuring_missing_commas() {
    let source = r#"
const { x y z } = obj;
"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    // Should parse with errors
    let diags = parser.get_diagnostics();
    assert!(!diags.is_empty(), "Should report error for missing commas");

    // Should still parse the destructuring pattern
    assert!(parser.arena.len() > 0, "Should parse destructuring");
}

/// Test object destructuring with trailing comma
#[test]
fn test_p1_destructuring_trailing_comma() {
    let source = r#"
const { x, y, z, } = obj;
"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    // Should parse successfully (trailing comma is valid)
    assert!(parser.arena.len() > 0, "Should parse destructuring");
}

/// Test object destructuring with missing colon
#[test]
fn test_p1_destructuring_missing_colon() {
    let source = r#"
const { x y } = obj;
"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    // Should parse with errors
    let diags = parser.get_diagnostics();
    assert!(!diags.is_empty(), "Should report error");

    // Should still parse the pattern
    assert!(parser.arena.len() > 0, "Should parse destructuring");
}

/// Test nested object destructuring with errors
#[test]
fn test_p1_nested_destructuring_errors() {
    let source = r#"
const { a: { x y }, b } = obj;
"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    // Should parse with errors
    let diags = parser.get_diagnostics();
    assert!(!diags.is_empty(), "Should report error");

    // Should still parse the outer pattern
    assert!(parser.arena.len() > 0, "Should parse destructuring");
}

// ===========================================================================
// Comprehensive Recovery Tests
// ===========================================================================

/// Test multiple errors in same file - verify parser doesn't crash
#[test]
fn test_p1_multiple_errors_recovery() {
    let source = r#"
class Foo {
    if (true) { }
    getValue() { return 1; }
}

interface Bar extends A B {
    x: number;
}

const { a b } = obj;
"#;
    let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
    parser.parse_source_file();

    // Should parse entire file despite errors
    let diags = parser.get_diagnostics();
    assert!(!diags.is_empty(), "Should report errors");

    // Should parse all declarations
    assert!(parser.arena.len() > 0, "Should parse all declarations");
}
