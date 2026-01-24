//! Tests for TS2693 (type-only as value) and TS2362/TS2363 (arithmetic operand errors)

use crate::binder::BinderState;
use crate::checker::state::CheckerState;
use crate::parser::ParserState;
use crate::solver::TypeInterner;

#[test]
fn test_interface_used_as_value() {
    let source = r#"
interface Foo {
    a: number;
}
const x = new Foo();
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    // Should emit TS2693 for using interface as value
    let ts2693_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2693)
        .count();
    assert!(
        ts2693_count >= 1,
        "Expected at least 1 TS2693 error, got {}",
        ts2693_count
    );
}

#[test]
fn test_type_alias_used_as_value() {
    let source = r#"
type Foo = {
    a: number;
};
const x = new Foo();
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    // Should emit TS2693 for using type alias as value
    let ts2693_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2693)
        .count();
    assert!(
        ts2693_count >= 1,
        "Expected at least 1 TS2693 error, got {}",
        ts2693_count
    );
}

#[test]
fn test_string_subtraction_emits_ts2362() {
    let source = r#"
const str = "hello";
const result = str - 5;
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    // Should emit TS2362 for left-hand side of - operation
    let ts2362_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2362)
        .count();
    assert!(
        ts2362_count >= 1,
        "Expected at least 1 TS2362 error, got {}",
        ts2362_count
    );
}

#[test]
fn test_boolean_multiplication_emits_ts2362() {
    let source = r#"
const flag = true;
const result = flag * 10;
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    // Should emit TS2362 for left-hand side of * operation
    let ts2362_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2362)
        .count();
    assert!(
        ts2362_count >= 1,
        "Expected at least 1 TS2362 error, got {}",
        ts2362_count
    );
}

#[test]
fn test_number_divided_by_string_emits_ts2363() {
    let source = r#"
const num = 10;
const str = "hello";
const result = num / str;
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    // Should emit TS2363 for right-hand side of / operation
    let ts2363_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2363)
        .count();
    assert!(
        ts2363_count >= 1,
        "Expected at least 1 TS2363 error, got {}",
        ts2363_count
    );
}

#[test]
fn test_arithmetic_on_non_numeric_types() {
    let source = r#"
const obj = { a: 1 };
const arr = [1, 2, 3];
const r1 = obj - 1;  // TS2362
const r2 = 10 * arr;  // TS2363
const r3 = obj % 2;  // TS2362
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    // Should emit multiple TS2362 and TS2363 errors
    let ts2362_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2362)
        .count();
    let ts2363_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2363)
        .count();

    assert!(
        ts2362_count >= 2,
        "Expected at least 2 TS2362 errors, got {}",
        ts2362_count
    );
    assert!(
        ts2363_count >= 1,
        "Expected at least 1 TS2363 error, got {}",
        ts2363_count
    );
}

#[test]
fn test_valid_arithmetic_no_errors() {
    let source = r#"
const a = 10;
const b = 5;
const r1 = a + b;  // OK - number addition
const r2 = a - b;  // OK - number subtraction
const r3 = a * b;  // OK - number multiplication
const r4 = a / b;  // OK - number division
const r5 = a % b;  // OK - number modulo
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    // Should not emit TS2362 or TS2363 for valid arithmetic
    let error_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2362 || d.code == 2363)
        .count();
    assert_eq!(
        error_count, 0,
        "Expected no TS2362/TS2363 errors, got {}",
        error_count
    );
}

#[test]
fn test_for_of_variable_type_annotation_emits_ts2322() {
    let source = r#"
const numbers = [1, 2, 3];
for (const x: string of numbers) {
    // Should emit TS2322: number is not assignable to string
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    // Should emit TS2322 for for-of variable with incompatible type annotation
    let ts2322_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .count();
    assert!(
        ts2322_count >= 1,
        "Expected at least 1 TS2322 error, got {}",
        ts2322_count
    );
}

#[test]
fn test_for_of_variable_compatible_type_no_error() {
    let source = r#"
const numbers = [1, 2, 3];
for (const x: number of numbers) {
    // OK - number is assignable to number
}
"#;
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    // Should not emit TS2322 for compatible types
    let ts2322_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .count();
    assert_eq!(
        ts2322_count, 0,
        "Expected no TS2322 errors, got {}",
        ts2322_count
    );
}
