//! Integration tests for const assertions (`as const`).
//!
//! These tests verify that TypeScript's `as const` assertion works correctly:
//! - Primitives preserve literal types
//! - Arrays become readonly tuples
//! - Object properties become readonly recursively
//! - Nested structures are handled correctly

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

    if !checker.ctx.diagnostics.is_empty() {
        panic!(
            "Expected no errors, but got:\n{}",
            checker
                .ctx
                .diagnostics
                .iter()
                .map(|d| format!("  {}", d.message_text))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
}

#[test]
fn test_const_assertion_primitive_literal() {
    // "hello" as const should preserve the literal type "hello"
    test_no_errors(
        r#"
const x = "hello" as const;
// x should have type "hello", not string
let y: "hello" = x; // Should be allowed
"#,
    );
}

#[test]
fn test_const_assertion_number_literal() {
    // 42 as const should preserve the literal type 42
    test_no_errors(
        r#"
const x = 42 as const;
// x should have type 42, not number
let y: 42 = x; // Should be allowed
"#,
    );
}

#[test]
fn test_const_assertion_boolean_literal() {
    // true as const should preserve the literal type true
    test_no_errors(
        r#"
const x = true as const;
// x should have type true, not boolean
let y: true = x; // Should be allowed
"#,
    );
}

#[test]
fn test_const_assertion_array_becomes_readonly_tuple() {
    // [1, 2, 3] as const becomes readonly [1, 2, 3]
    test_no_errors(
        r#"
const arr = [1, 2, 3] as const;
// arr should be readonly tuple [1, 2, 3]
let first: 1 = arr[0]; // Should be allowed
let second: 2 = arr[1]; // Should be allowed
"#,
    );
}

#[test]
fn test_const_assertion_object_properties_readonly() {
    // { x: 1 } as const should have readonly x property
    test_no_errors(
        r#"
const obj = { x: 1, y: "hello" } as const;
// obj.x should be readonly with type 1
// obj.y should be readonly with type "hello"
const val1: 1 = obj.x; // Should be allowed
const val2: "hello" = obj.y; // Should be allowed
"#,
    );
}

#[test]
fn test_const_assertion_nested_object() {
    // Nested objects should have readonly properties recursively
    test_no_errors(
        r#"
const obj = {
    a: 1,
    nested: {
        b: "hello",
        c: true
    }
} as const;
// All properties should be readonly with literal types
const val1: 1 = obj.a; // Should be allowed
const val2: "hello" = obj.nested.b; // Should be allowed
const val3: true = obj.nested.c; // Should be allowed
"#,
    );
}

#[test]
fn test_const_assertion_mixed_array_and_object() {
    // Arrays in objects should also be readonly tuples
    test_no_errors(
        r#"
const obj = {
    x: 1,
    arr: [1, 2, 3]
} as const;
// obj.x should be readonly 1
// obj.arr should be readonly tuple [1, 2, 3]
const val: 1 = obj.x; // Should be allowed
"#,
    );
}

#[test]
fn test_const_assertion_template_literal() {
    // Template literals should preserve their literal type
    test_no_errors(
        r#"
const x = `hello` as const;
// x should have type `hello`, not string
const y: `hello` = x; // Should be allowed
"#,
    );
}

#[test]
fn test_const_assertion_null_and_undefined() {
    // null and undefined should preserve their literal types
    test_no_errors(
        r#"
const x = null as const;
const y = undefined as const;
const a: null = x; // Should be allowed
const b: undefined = y; // Should be allowed
"#,
    );
}

#[test]
fn test_const_assertion_nested_array() {
    // Nested arrays should become readonly tuples recursively
    test_no_errors(
        r#"
const arr = [[1, 2], [3, 4]] as const;
// arr should be readonly tuple of readonly tuples
const val: 1 = arr[0][0]; // Should be allowed
"#,
    );
}

#[test]
fn test_const_assertion_array_of_objects() {
    // Arrays of objects should have readonly objects
    test_no_errors(
        r#"
const arr = [{ x: 1 }, { y: 2 }] as const;
// Each object should have readonly properties
const val: 1 = arr[0].x; // Should be allowed
"#,
    );
}
