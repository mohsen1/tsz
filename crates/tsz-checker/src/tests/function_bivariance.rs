//! Tests for Function Bivariance (Lawyer Layer).
//!
//! These tests verify that methods are bivariant while function properties
//! are contravariant, per TypeScript's function variance rules.

use crate::checker::context::CheckerOptions;
use crate::checker::state::CheckerState;
use crate::test_fixtures::TestContext;
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

/// Workaround for TS2318 (Cannot find global type) errors in test infrastructure.
const GLOBAL_TYPE_MOCKS: &str = r#"
interface Array<T> {}
interface String {}
interface Boolean {}
interface Number {}
interface Object {}
interface Function {}
interface RegExp {}
interface IArguments {}
"#;

fn test_function_variance(source: &str, expected_error_code: u32) {
    // Prepend @strictFunctionTypes comment BEFORE GLOBAL_TYPE_MOCKS
    // because the parser stops at the first non-comment line
    // Remove any existing @strictFunctionTypes from source to avoid duplication
    let source_clean = source.replace("// @strictFunctionTypes: true", "");
    let source_clean = source_clean.trim();
    let source = format!(
        "// @strictFunctionTypes: true\n{}\n{}",
        GLOBAL_TYPE_MOCKS, source_clean
    );

    let ctx = TestContext::new();

    let mut parser = ParserState::new("test.ts".to_string(), source);
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file_with_libs(parser.get_arena(), root, &ctx.lib_files);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions::default(),
    );

    // Set lib contexts for global symbol resolution
    if !ctx.lib_files.is_empty() {
        let lib_contexts: Vec<crate::checker::context::LibContext> = ctx
            .lib_files
            .iter()
            .map(|lib| crate::checker::context::LibContext {
                arena: Arc::clone(&lib.arena),
                binder: Arc::clone(&lib.binder),
            })
            .collect();
        checker.ctx.set_lib_contexts(lib_contexts);
    }

    checker.check_source_file(root);

    let error_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == expected_error_code)
        .count();

    assert!(
        error_count >= 1,
        "Expected at least 1 TS{} error, got {}: {:?}",
        expected_error_code,
        error_count,
        checker.ctx.diagnostics
    );
}

fn test_no_errors(source: &str) {
    // Prepend @strictFunctionTypes comment BEFORE GLOBAL_TYPE_MOCKS
    // because the parser stops at the first non-comment line
    // Remove any existing @strictFunctionTypes from source to avoid duplication
    let source_clean = source.replace("// @strictFunctionTypes: true", "");
    let source_clean = source_clean.trim();
    let source = format!(
        "// @strictFunctionTypes: true\n{}\n{}",
        GLOBAL_TYPE_MOCKS, source_clean
    );

    let ctx = TestContext::new();

    let mut parser = ParserState::new("test.ts".to_string(), source);
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file_with_libs(parser.get_arena(), root, &ctx.lib_files);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions::default(),
    );

    // Set lib contexts for global symbol resolution
    if !ctx.lib_files.is_empty() {
        let lib_contexts: Vec<crate::checker::context::LibContext> = ctx
            .lib_files
            .iter()
            .map(|lib| crate::checker::context::LibContext {
                arena: Arc::clone(&lib.arena),
                binder: Arc::clone(&lib.binder),
            })
            .collect();
        checker.ctx.set_lib_contexts(lib_contexts);
    }

    checker.check_source_file(root);

    let errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.category == crate::checker::types::DiagnosticCategory::Error)
        .collect();

    assert!(
        errors.is_empty(),
        "Expected no errors, got {}: {:?}",
        errors.len(),
        errors
    );
}

/// Test that methods are bivariant (same parameter types work both ways).
#[test]
fn test_method_bivariance_same_params() {
    // Should pass - methods are bivariant
    test_no_errors(
        r#"
        interface A {
            method(x: number): void;
        }
        interface B {
            method(x: number): void;
        }
        let a: A = { method: (x: number) => {} };
        let b: B = a;
        "#,
    );
}

/// Test that method parameters accept wider types (bivariance).
#[test]
fn test_method_bivariance_wider_param() {
    // Should pass - methods are bivariant (accept wider in either direction)
    test_no_errors(
        r#"
        interface A {
            method(x: number): void;
        }
        interface B {
            method(x: number | string): void;
        }
        let a: A = { method: (x: number | string) => {} };
        let b: B = a;
        "#,
    );
}

/// Test that function properties are contravariant (not bivariant).
#[test]
fn test_function_property_contravariance() {
    // Should fail - function properties are contravariant
    test_function_variance(
        r#"
        // @strictFunctionTypes: true: true
        interface A {
            prop: (x: number | string) => void;
        }
        interface B {
            prop: (x: number) => void;
        }
        let b: B = { prop: (x: number) => {} };
        let a: A = b;
        "#,
        2322, // Type 'number' is not assignable to 'number | string'
    );
}

/// Test arrow function properties are contravariant (not bivariant).
#[test]
fn test_arrow_function_property_contravariance() {
    // Should fail - arrow functions are properties, not methods
    test_function_variance(
        r#"
        // @strictFunctionTypes: true: true
        interface A {
            prop: (x: number) => void;
        }
        interface B {
            prop: (x: number | string) => void;
        }
        let b: B = { prop: (x: number) => {} };
        let a: A = b;
        "#,
        2322, // Type error
    );
}

/// Test method shorthand syntax is bivariant.
#[test]
fn test_method_shorthand_bivariant() {
    // Should pass - method shorthand is bivariant
    test_no_errors(
        r#"
        // @strictFunctionTypes: true
        interface A {
            method(x: number): void;
        }
        interface B {
            method(x: number | string): void;
        }
        let b: B = { method: (x: number | string) => {} };
        let a: A = b;
        "#,
    );
}

/// Test that strictFunctionTypes doesn't affect methods.
#[test]
fn test_method_bivariance_strict_mode() {
    // Should pass - methods are bivariant even in strict mode
    test_no_errors(
        r#"
        // @strictFunctionTypes: true
        interface A {
            method(x: number): void;
        }
        interface B {
            method(x: number | string): void;
        }
        let b: B = { method: (x: number | string) => {} };
        let a: A = b;
        "#,
    );
}

/// Test that strictFunctionTypes enforces contravariance for function properties.
#[test]
fn test_function_property_contravariance_strict_mode() {
    // Should fail - function properties are contravariant in strict mode
    test_function_variance(
        r#"
        // @strictFunctionTypes: true
        interface A {
            prop: (x: number | string) => void;
        }
        interface B {
            prop: (x: number) => void;
        }
        let b: B = { prop: (x: number) => {} };
        let a: A = b;
        "#,
        2322, // Type error
    );
}
