//! Integration tests for literal type widening in variable declarations.

use crate::diagnostics::DiagnosticCategory;
use crate::test_utils::check_source_diagnostics;

fn test_no_errors(source: &str) {
    let diagnostics = check_source_diagnostics(source);
    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.category == DiagnosticCategory::Error && d.code != 2318)
        .collect();

    assert!(
        errors.is_empty(),
        "Expected no errors, got {}: {:?}",
        errors.len(),
        errors
    );
}

#[test]
fn test_const_object_literal_property_widening() {
    // Properties of const object literals should be widened
    test_no_errors(
        r#"
        const obj = { x: 1 };
        obj.x = 2; // Should be allowed - x is number, not literal 1
        "#,
    );
}

#[test]
fn test_let_object_literal_property_widening() {
    // Properties of let object literals should be widened
    test_no_errors(
        r#"
        let obj = { x: 1 };
        obj.x = 2; // Should be allowed - x is number
        "#,
    );
}

#[test]
fn test_nested_object_property_widening() {
    // Nested object properties should be widened
    test_no_errors(
        r#"
        const obj = { a: { b: "hello" } };
        obj.a.b = "world"; // Should be allowed - b is string
        "#,
    );
}

#[test]
fn test_const_primitive_literal_preserved() {
    // const with primitive literals should preserve the literal type
    test_no_errors(
        r#"
        const x = 1;
        const y: 1 = x; // Should work - x is literal 1
        "#,
    );
}

#[test]
fn test_let_primitive_literal_widened() {
    // let with primitive literals should widen to the primitive type
    test_no_errors(
        r#"
        let x = 1;
        x = 2; // Should be allowed - x is number
        "#,
    );
}

#[test]
fn test_for_of_loop_variable_widening() {
    // Loop variables in for-of should be widened for let, preserved for const
    test_no_errors(
        r#"
        for (let x of [1, 2, 3]) {
            x = 4; // Should be allowed - x is number
        }
        "#,
    );
}
