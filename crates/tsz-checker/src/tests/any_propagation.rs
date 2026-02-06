//! Tests for Any-Propagation (Lawyer Layer).
//!
//! These tests verify that `any` behaves as both Top and Bottom type
//! in the Lawyer layer, respecting strict mode settings.

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

fn test_no_errors(source: &str) {
    let source = format!(
        "// @strictFunctionTypes: true\n{}\n{}",
        GLOBAL_TYPE_MOCKS, source
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

#[allow(dead_code)]
fn test_expect_error(source: &str, expected_error_code: u32) {
    let source = format!(
        "// @strictFunctionTypes: true\n{}\n{}",
        GLOBAL_TYPE_MOCKS, source
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

/// Test that any is assignable to any type (Top type behavior)
#[test]
fn test_any_assignable_to_string() {
    // Should pass - any is assignable to string
    test_no_errors(
        r#"
        let x: any;
        let y: string = x;
        "#,
    );
}

/// Test that any type is assignable to any (Bottom type behavior)
#[test]
fn test_string_assignable_to_any() {
    // Should pass - string is assignable to any
    test_no_errors(
        r#"
        let x: string = "hello";
        let y: any = x;
        "#,
    );
}

/// Test that any works in object properties
#[test]
fn test_any_in_object_properties() {
    // Should pass - any property silences structural errors
    test_no_errors(
        r#"
        interface A {
            x: string;
        }
        const obj: A = { x: "hello" };
        const anyValue: any = obj;
        "#,
    );
}

/// Test that any works in function parameters
#[test]
fn test_any_in_function_parameters() {
    // Should pass - any can be passed to any parameter type
    test_no_errors(
        r#"
        function takesString(s: string) {
            return s;
        }
        const anyValue: any = "hello";
        takesString(anyValue);
        "#,
    );
}

/// Test that any works with nested object types
#[test]
fn test_any_nested_object() {
    // Should pass - any in nested object silences errors
    test_no_errors(
        r#"
        interface A {
            x: { y: string };
        }
        const obj: A = { x: { y: "hello" } };
        const anyValue: any = obj;
        "#,
    );
}

/// Test that any to any is always assignable
#[test]
fn test_any_to_any() {
    // Should pass - any is assignable to any
    test_no_errors(
        r#"
        let a1: any;
        let a2: any = a1;
        "#,
    );
}

/// Test that any works in array types
#[test]
fn test_any_in_arrays() {
    // Should pass - any[] can be assigned to string[]
    test_no_errors(
        r#"
        const anyArray: any[] = [1, "hello", {}];
        const stringArray: string[] = anyArray;
        "#,
    );
}

/// Test that any works with interface assignments
#[test]
fn test_any_interface_assignment() {
    // Should pass - any is assignable to any interface
    test_no_errors(
        r#"
        interface User {
            name: string;
            age: number;
        }
        const anyValue: any = { name: "John", age: 30 };
        const user: User = anyValue;
        "#,
    );
}

/// Test that any works in union types
#[test]
fn test_any_in_union_types() {
    // Should pass - any is assignable to union
    test_no_errors(
        r#"
        type StringOrNumber = string | number;
        const anyValue: any = "hello";
        const value: StringOrNumber = anyValue;
        "#,
    );
}

/// Test that any works in intersection types
#[test]
fn test_any_in_intersection_types() {
    // Should pass - any is assignable to intersection
    test_no_errors(
        r#"
        type Named = { name: string };
        type Aged = { age: number };
        type NamedAndAged = Named & Aged;
        const anyValue: any = { name: "John", age: 30 };
        const value: NamedAndAged = anyValue;
        "#,
    );
}
