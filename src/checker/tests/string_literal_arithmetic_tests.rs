//! Tests for TS2362 with string literal union types
//!
//! These tests verify that we don't emit false positive TS2362 errors
//! when the + operator is used with union types containing string literals.

use crate::binder::BinderState;
use crate::checker::CheckerState;
use crate::parser::ParserState;
use crate::solver::TypeInterner;

#[test]
fn test_string_literal_union_plus_no_error() {
    let source = r#"
let x: "hello" | number;
let y = x + 1;  // Should not emit TS2362 - this is string concatenation
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

    // Should not emit TS2362 for string literal union + number
    let error_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2362 || d.code == 2363)
        .count();
    assert_eq!(
        error_count, 0,
        "Expected no TS2362/TS2363 errors for string literal union + number, got {}",
        error_count
    );
}

#[test]
fn test_number_string_union_minus_emits_ts2362() {
    let source = r#"
declare let x: number | string;
let y = x - 1;  // Should emit TS2362 - this is arithmetic, not string concatenation
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

    // Should emit TS2362 for number | string - number (arithmetic operator)
    let error_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2362)
        .count();
    assert!(
        error_count >= 1,
        "Expected at least 1 TS2362 error for number | string - number, got {}",
        error_count
    );
}

#[test]
fn test_multiple_string_literals_union_plus_no_error() {
    let source = r#"
let x: "hello" | "world" | number;
let y = x + 1;  // Should not emit TS2362
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

    // Should not emit TS2362
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
fn test_number_literal_union_plus_number_no_error() {
    let source = r#"
let x: 1 | 2 | 3;
let y = x + 1;  // Should not emit TS2362 - number literal union is valid for arithmetic
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

    // Should not emit TS2362
    let error_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2362 || d.code == 2363)
        .count();
    assert_eq!(
        error_count, 0,
        "Expected no TS2362/TS2363 errors for number literal union, got {}",
        error_count
    );
}
