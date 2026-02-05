//! Tests for Void Return Exception (Lawyer Layer).
//!
//! These tests verify the "callback ergonomics" feature where
//! a function returning any type T can be assigned to a
//! function type expecting void.
//!
//! Rule: () => T is assignable to () => void for callback ergonomics

use crate::binder::BinderState;
use crate::checker::context::CheckerOptions;
use crate::checker::state::CheckerState;
use crate::parser::ParserState;
use crate::solver::TypeInterner;
use crate::test_fixtures::TestContext;
use std::sync::Arc;

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
interface Promise<T> {}
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

/// Test basic void return exception - function returning value can be assigned to void callback
#[test]
fn test_void_return_basic() {
    // Should pass - () => string is assignable to () => void
    test_no_errors(
        r#"
        function takesCallback(cb: () => void) {
            cb();
        }
        takesCallback(() => "hello");
        "#,
    );
}

/// Test void return exception with number return type
#[test]
fn test_void_return_number() {
    // Should pass - () => number is assignable to () => void
    test_no_errors(
        r#"
        function takesCallback(cb: () => void) {
            cb();
        }
        takesCallback(() => 42);
        "#,
    );
}

/// Test void return exception with object return type
#[test]
fn test_void_return_object() {
    // Should pass - () => object is assignable to () => void
    test_no_errors(
        r#"
        function takesCallback(cb: () => void) {
            cb();
        }
        takesCallback(() => ({ x: 1 }));
        "#,
    );
}

/// Test that undefined return is NOT assignable to void in strict mode
#[test]
fn test_undefined_return_not_assignable_to_void() {
    // Should fail - () => string is NOT assignable to () => undefined
    // (undefined is a specific value, not the "ignore result" marker)
    test_expect_error(
        r#"
        type Callback = () => undefined;
        const f: Callback = () => "hello";
        "#,
        2322, // Type 'string' is not assignable to 'undefined'
    );
}

/// Test that Promise<void> is strict - Promise<string> not assignable to Promise<void>
#[test]
fn test_promise_void_strictness() {
    // Should fail - () => Promise<string> is NOT assignable to () => Promise<void>
    test_expect_error(
        r#"
        type AsyncCallback = () => Promise<void>;
        const f: AsyncCallback = () => Promise.resolve("hello");
        "#,
        2322, // Type 'Promise<string>' is not assignable to 'Promise<void>'
    );
}

/// Test void return exception in interface assignments
#[test]
fn test_void_return_interface_assignment() {
    // Should pass - interface with method returning string assignable to void method
    test_no_errors(
        r#"
        interface VoidCallback {
            method(): void;
        }
        const impl: VoidCallback = {
            method: () => "returns value but ignored"
        };
        "#,
    );
}

/// Test void return exception with function expressions
#[test]
fn test_void_return_function_expression() {
    // Should pass - function expression returning value assignable to void
    test_no_errors(
        r#"
        function takesCallback(cb: () => void) {
            cb();
        }
        takesCallback(function() { return "ignored"; });
        "#,
    );
}

/// Test void return exception with arrow functions
#[test]
fn test_void_return_arrow_function() {
    // Should pass - arrow function returning value assignable to void
    test_no_errors(
        r#"
        function takesCallback(cb: () => void) {
            cb();
        }
        takesCallback(() => { return "ignored"; });
        "#,
    );
}

/// Test that void return is NOT covariant (void not assignable to string)
#[test]
fn test_void_not_assignable_to_string() {
    // Should fail - () => void is NOT assignable to () => string
    test_expect_error(
        r#"
        type StringCallback = () => string;
        const f: StringCallback = () => {};
        "#,
        2322, // Type 'void' is not assignable to 'string'
    );
}

/// Test void return exception in array of callbacks
#[test]
fn test_void_return_array_callbacks() {
    // Should pass - array of functions returning values assignable to void callbacks
    test_no_errors(
        r#"
        const callbacks: Array<() => void> = [
            () => 1,
            () => "hello",
            () => ({ x: 1 })
        ];
        "#,
    );
}
