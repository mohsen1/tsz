//! Tests for TS2838: All declarations of '{0}' must have identical constraints.
//!
//! When `infer U` appears multiple times in the same conditional type extends clause,
//! all declarations with explicit constraints must have the same constraint type.

use crate::test_utils::check_source_code_messages;

fn get_diagnostics(source: &str) -> Vec<(u32, String)> {
    check_source_code_messages(source)
}

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
fn duplicate_infer_different_constraints_emits_ts2838() {
    // Same name `U` with different constraints `string` vs `number`
    let source = r#"
type X<T> = T extends { a: infer U extends string, b: infer U extends number } ? U : never;
"#;
    assert!(
        has_error_with_code(source, 2838),
        "Should emit TS2838 when infer U has different constraints (string vs number)"
    );
    // Should emit exactly 2 errors — one at each declaration site
    assert_eq!(
        count_errors_with_code(source, 2838),
        2,
        "Should emit TS2838 at each declaration site"
    );
}

#[test]
fn duplicate_infer_same_constraints_no_error() {
    // Same name `U` with same constraint `string` — no error
    let source = r#"
type X<T> = T extends { a: infer U extends string, b: infer U extends string } ? U : never;
"#;
    assert!(
        !has_error_with_code(source, 2838),
        "Should NOT emit TS2838 when infer U has same constraint on both declarations"
    );
}

#[test]
fn duplicate_infer_one_constrained_one_not_no_error() {
    // `infer U extends string` + `infer U` (no constraint) — TSC allows this
    let source = r#"
type X<T> = T extends { a: infer U extends string, b: infer U } ? U : never;
"#;
    assert!(
        !has_error_with_code(source, 2838),
        "Should NOT emit TS2838 when one infer U has constraint and other doesn't"
    );
}

#[test]
fn duplicate_infer_neither_constrained_no_error() {
    // Both `infer U` without constraints — no error
    let source = r#"
type X<T> = T extends { a: infer U, b: infer U } ? U : never;
"#;
    assert!(
        !has_error_with_code(source, 2838),
        "Should NOT emit TS2838 when neither infer U has a constraint"
    );
}

#[test]
fn duplicate_infer_in_nested_conditionals_no_error() {
    // `infer U extends string` in outer conditional, `infer U extends number` in inner
    // These are DIFFERENT scopes — no TS2838
    let source = r#"
type X<T extends any[]> =
    T extends [infer U extends string] ? ["string", U] :
    T extends [infer U extends number] ? ["number", U] :
    never;
"#;
    assert!(
        !has_error_with_code(source, 2838),
        "Should NOT emit TS2838 for infer U in different conditional scopes"
    );
}

#[test]
fn single_infer_with_constraint_no_error() {
    // Only one `infer U` — no duplicate, no error
    let source = r#"
type X<T> = T extends { a: infer U extends string } ? U : never;
"#;
    assert!(
        !has_error_with_code(source, 2838),
        "Should NOT emit TS2838 for a single infer declaration"
    );
}
