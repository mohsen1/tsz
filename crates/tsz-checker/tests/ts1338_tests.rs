//! Tests for TS1338: 'infer' declarations are only permitted in the 'extends'
//! clause of a conditional type.
//!
//! `infer T` is only valid inside the `extends` portion of a conditional type.
//! Anywhere else (standalone type alias, `check_type`, `true_type`, `false_type`, etc.)
//! must emit TS1338.

use crate::test_utils::check_source_code_messages as get_diagnostics;

fn count_errors_with_code(source: &str, code: u32) -> usize {
    get_diagnostics(source)
        .iter()
        .filter(|d| d.0 == code)
        .count()
}

fn has_error_with_code(source: &str, code: u32) -> bool {
    count_errors_with_code(source, code) > 0
}

#[test]
fn infer_outside_conditional_emits_ts1338() {
    // `infer U` as a standalone type alias — not inside any conditional
    let source = "type T60 = infer U;";
    assert!(
        has_error_with_code(source, 1338),
        "Should emit TS1338 for infer outside conditional type"
    );
}

#[test]
fn infer_in_check_type_emits_ts1338() {
    // `infer A` in the check_type position (left of extends) — not allowed
    let source = "type T61<T> = (infer A) extends infer B ? infer C : infer D;";
    // infer A (check_type) → TS1338
    // infer B (extends) → OK
    // infer C (true_type) → TS1338
    // infer D (false_type) → TS1338
    assert_eq!(
        count_errors_with_code(source, 1338),
        3,
        "Should emit TS1338 for infer in check_type, true_type, and false_type (3 total)"
    );
}

#[test]
fn infer_in_extends_clause_no_error() {
    // `infer U` inside extends clause — valid
    let source = "type X<T> = T extends infer U ? U : never;";
    assert!(
        !has_error_with_code(source, 1338),
        "Should NOT emit TS1338 for infer in extends clause"
    );
}

#[test]
fn infer_in_nested_conditional_extends_no_error() {
    // `infer U` inside nested conditional extends — valid at any depth
    let source = r#"
type X<T> = T extends { a: infer U } ? (U extends infer V ? V : never) : never;
"#;
    assert!(
        !has_error_with_code(source, 1338),
        "Should NOT emit TS1338 for infer in nested conditional extends"
    );
}

#[test]
fn infer_with_constraint_in_extends_no_error() {
    // `infer U extends string` — valid in extends clause
    let source = "type X<T> = T extends { a: infer U extends string } ? U : never;";
    assert!(
        !has_error_with_code(source, 1338),
        "Should NOT emit TS1338 for constrained infer in extends clause"
    );
}
