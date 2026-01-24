//! Tests for TS2322 assignability errors
//!
//! These tests verify that TS2322 "Type 'X' is not assignable to type 'Y'" errors
//! are properly emitted in various contexts.

use crate::checker::state::CheckerState;
use crate::checker::types::diagnostics::diagnostic_codes;
use crate::parser::ParserState;
use crate::test_fixtures::TestContext;

/// Helper function to check if a diagnostic with a specific code was emitted
fn has_error_with_code(source: &str, code: u32) -> bool {
    let ctx = TestContext::new();
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_file().expect("Parse failed");

    let mut checker = ctx.checker();
    checker.check_file(root);

    checker
        .ctx
        .diagnostics
        .errors()
        .any(|d| d.code == code)
}

/// Helper to count errors with a specific code
fn count_errors_with_code(source: &str, code: u32) -> usize {
    let ctx = TestContext::new();
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_file().expect("Parse failed");

    let mut checker = ctx.checker();
    checker.check_file(root);

    checker
        .ctx
        .diagnostics
        .by_code(code)
        .count()
}

// =============================================================================
// Return Statement Tests (TS2322)
// =============================================================================

#[test]
fn test_ts2322_return_wrong_primitive() {
    let source = r#"
        function returnNumber(): number {
            return "string";
        }
    "#;

    assert!(has_error_with_code(source, diagnostic_codes::TYPE_NOT_ASSIGNABLE));
}

#[test]
fn test_ts2322_return_wrong_object_property() {
    let source = r#"
        function returnObject(): { a: number } {
            return { a: "string" };
        }
    "#;

    assert!(has_error_with_code(source, diagnostic_codes::TYPE_NOT_ASSIGNABLE));
}

#[test]
fn test_ts2322_return_wrong_array_element() {
    let source = r#"
        function returnArray(): number[] {
            return ["string"];
        }
    "#;

    assert!(has_error_with_code(source, diagnostic_codes::TYPE_NOT_ASSIGNABLE));
}

// =============================================================================
// Variable Declaration Tests (TS2322)
// =============================================================================

#[test]
fn test_ts2322_variable_declaration_wrong_type() {
    let source = r#"
        let x: number = "string";
    "#;

    assert!(has_error_with_code(source, diagnostic_codes::TYPE_NOT_ASSIGNABLE));
}

#[test]
fn test_ts2322_variable_declaration_wrong_object_property() {
    let source = r#"
        let y: { a: number } = { a: "string" };
    "#;

    assert!(has_error_with_code(source, diagnostic_codes::TYPE_NOT_ASSIGNABLE));
}

#[test]
fn test_ts2322_variable_declaration_wrong_array_element() {
    let source = r#"
        let z: string[] = [1, 2, 3];
    "#;

    assert!(has_error_with_code(source, diagnostic_codes::TYPE_NOT_ASSIGNABLE));
}

// =============================================================================
// Assignment Expression Tests (TS2322)
// =============================================================================

#[test]
fn test_ts2322_assignment_wrong_primitive() {
    let source = r#"
        let a: number;
        a = "string";
    "#;

    assert!(has_error_with_code(source, diagnostic_codes::TYPE_NOT_ASSIGNABLE));
}

#[test]
fn test_ts2322_assignment_wrong_object_property() {
    let source = r#"
        let obj: { a: number };
        obj = { a: "string" };
    "#;

    assert!(has_error_with_code(source, diagnostic_codes::TYPE_NOT_ASSIGNABLE));
}

// =============================================================================
// Property Assignment Tests (TS2322)
// =============================================================================

#[test]
fn test_ts2322_property_assignment_wrong_type() {
    let source = r#"
        interface PropTarget {
            prop: number;
        }
        const t: PropTarget = { prop: 42 };
        t.prop = "string";
    "#;

    assert!(has_error_with_code(source, diagnostic_codes::TYPE_NOT_ASSIGNABLE));
}

// =============================================================================
// Array Destructuring Tests (TS2322)
// =============================================================================

#[test]
fn test_ts2322_array_destructuring_wrong_type() {
    let source = r#"
        const [num]: number = ["string"];
    "#;

    assert!(has_error_with_code(source, diagnostic_codes::TYPE_NOT_ASSIGNABLE));
}

// =============================================================================
// Object Destructuring Tests (TS2322)
// =============================================================================

#[test]
fn test_ts2322_object_destructuring_wrong_property_type() {
    let source = r#"
        const { prop }: { prop: number } = { prop: "string" };
    "#;

    assert!(has_error_with_code(source, diagnostic_codes::TYPE_NOT_ASSIGNABLE));
}

// =============================================================================
// Multiple TS2322 Errors
// =============================================================================

#[test]
fn test_ts2322_multiple_errors() {
    let source = r#"
        function f1(): number {
            return "string";
        }
        function f2(): string {
            return 42;
        }
        let x: number = "x";
        let y: string = 123;
    "#;

    let count = count_errors_with_code(source, diagnostic_codes::TYPE_NOT_ASSIGNABLE);
    assert!(count >= 4, "Expected at least 4 TS2322 errors, got {}", count);
}

// =============================================================================
// No Error Tests (Verify we don't emit false positives)
// =============================================================================

#[test]
fn test_ts2322_no_error_correct_types() {
    let source = r#"
        function returnNumber(): number {
            return 42;
        }
        let x: number = 42;
        let y: { a: number } = { a: 42 };
        let z: string[] = ["a", "b"];
        let a: number;
        a = 42;
    "#;

    assert!(!has_error_with_code(source, diagnostic_codes::TYPE_NOT_ASSIGNABLE));
}
