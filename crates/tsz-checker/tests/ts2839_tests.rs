//! Tests for TS2839: This condition will always return 'true'/'false'
//! since JavaScript compares objects by reference, not value.

use crate::test_utils::check_source_code_messages;

fn get_diagnostics(source: &str) -> Vec<(u32, String)> {
    check_source_code_messages(source)
}

fn has_error_with_code(source: &str, code: u32) -> bool {
    get_diagnostics(source).iter().any(|d| d.0 == code)
}

// TS2839 should fire when comparing object literals with equality operators

#[test]
fn object_literal_strict_equals_object_literal() {
    let source = r#"
if ({a: 1} === {a: 1}) {}
"#;
    assert!(has_error_with_code(source, 2839));
    let diags = get_diagnostics(source);
    let msg = &diags.iter().find(|d| d.0 == 2839).unwrap().1;
    assert!(msg.contains("'false'"));
}

#[test]
fn array_literal_strict_equals_array_literal() {
    let source = r#"
if ([1] === [1]) {}
"#;
    assert!(has_error_with_code(source, 2839));
    let diags = get_diagnostics(source);
    let msg = &diags.iter().find(|d| d.0 == 2839).unwrap().1;
    assert!(msg.contains("'false'"));
}

#[test]
fn object_literal_strict_not_equals() {
    let source = r#"
if ({a: 1} !== {a: 1}) {}
"#;
    assert!(has_error_with_code(source, 2839));
    let diags = get_diagnostics(source);
    let msg = &diags.iter().find(|d| d.0 == 2839).unwrap().1;
    assert!(msg.contains("'true'"));
}

#[test]
fn variable_equals_object_literal() {
    // TS2839 fires when one side is a variable and the other is a literal
    let source = r#"
const a = {x: 1};
if (a === {x: 1}) {}
"#;
    assert!(has_error_with_code(source, 2839));
}

#[test]
fn loose_equality_object_literal() {
    // TS2839 also fires for == and !=
    let source = r#"
if ({a: 1} == {a: 1}) {}
"#;
    assert!(has_error_with_code(source, 2839));
    let diags = get_diagnostics(source);
    let msg = &diags.iter().find(|d| d.0 == 2839).unwrap().1;
    assert!(msg.contains("'false'"));
}

#[test]
fn loose_not_equals_array_literal() {
    let source = r#"
if ([1] != [1]) {}
"#;
    assert!(has_error_with_code(source, 2839));
    let diags = get_diagnostics(source);
    let msg = &diags.iter().find(|d| d.0 == 2839).unwrap().1;
    assert!(msg.contains("'true'"));
}

#[test]
fn no_ts2839_for_primitive_comparison() {
    // Comparing primitive values should NOT trigger TS2839
    let source = r#"
if (1 === 1) {}
if ("a" === "b") {}
"#;
    assert!(!has_error_with_code(source, 2839));
}

#[test]
fn no_ts2839_for_variable_variable_comparison() {
    // Comparing two variables should NOT trigger TS2839
    let source = r#"
const a = {x: 1};
const b = {x: 1};
if (a === b) {}
"#;
    assert!(!has_error_with_code(source, 2839));
}
