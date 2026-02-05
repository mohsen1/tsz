//! Manual tests for generic type inference
//!
//! Tests that generic functions properly infer type arguments from:
//! - Function arguments (upward inference)
//! - Contextual type (downward inference)
//! - Constraints (extends clauses)

use crate::binder::BinderState;
use crate::checker::state::CheckerState;
use crate::parser::ParserState;
use crate::solver::TypeInterner;

#[test]
fn test_identity_function_inference() {
    let source = r#"
function identity<T>(x: T): T {
    return x;
}

const s = identity("hello");
const n = identity(42);
const b = identity(true);
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

    // s should be string, n should be number, b should be boolean
    // Filter out "Cannot find global type" errors - those are expected without lib files
    let type_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code != 2318)
        .collect();

    assert!(
        type_errors.is_empty(),
        "Expected no type errors, got: {:?}",
        type_errors
    );
}

#[test]
fn test_constraint_validation() {
    let source = r#"
function logName<T extends { name: string }>(obj: T): void {
    // Avoid using console.log which requires lib files
    const _unused: void = undefined;
}

const invalid = { id: 1 };
logName(invalid);
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

    // Should have error because { id: 1 } doesn't satisfy { name: string }
    // Look for TS2322 (type mismatch) or similar constraint violation errors
    let type_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code != 2318)
        .collect();

    assert!(
        !type_errors.is_empty(),
        "Expected errors for constraint violation, got: {:?}",
        type_errors
    );
}

#[test]
fn test_downward_inference() {
    let source = r#"
function identity<T>(x: T): T {
    return x;
}

const x: string = identity(42);
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

    // Should have error because 42 is not assignable to string
    let type_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code != 2318)
        .collect();

    assert!(!type_errors.is_empty(), "Expected error for type mismatch");

    // Verify it's specifically a type mismatch error (TS2322)
    assert!(
        type_errors.iter().any(|d| d.code == 2322),
        "Expected TS2322 error for type mismatch, got: {:?}",
        type_errors
    );
}
