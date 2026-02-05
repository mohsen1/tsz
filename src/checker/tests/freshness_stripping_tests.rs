//! Integration tests for freshness stripping (Priority 5 of TSZ-6).
//!
//! These tests verify that object literal freshness is stripped when assigned
//! to widened variables, preventing excess property checking when the variable
//! is used as a source in subsequent assignments.

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

fn test_expect_error(source: &str, expected_error_substring: &str) {
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

    // Check that we got at least one error with the expected substring
    let found_error = checker
        .ctx
        .diagnostics
        .iter()
        .any(|d| d.message_text.contains(expected_error_substring));

    if !found_error {
        panic!(
            "Expected error containing '{}', but got:\n{}",
            expected_error_substring,
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
fn test_freshness_stripped_variable_can_be_used_as_source() {
    // x has excess property 'b', but freshness is stripped after declaration
    // When x is used as a source, it should NOT trigger EPC
    test_no_errors(
        r#"
let x = { a: 1, b: 2 };  // Freshness stripped, type is { a: number, b: number }
let y: { a: number } = x;  // Should PASS (x is non-fresh, excess prop allowed)
"#,
    );
}

#[test]
fn test_fresh_object_literal_triggers_epc() {
    // A FRESH object literal with excess properties SHOULD trigger EPC
    test_expect_error(
        r#"
let y: { a: number } = { a: 1, b: 2 };  // Should trigger EPC for 'b'
"#,
        "Object literal may only specify known properties",
    );
}

#[test]
fn test_freshness_stripped_in_let_declaration() {
    test_no_errors(
        r#"
let x = { a: 1, b: 2, c: 3 };  // Freshness stripped
let y: { a: number } = x;  // Should NOT trigger EPC
let z: { a: number, b: number } = x;  // Should NOT trigger EPC
"#,
    );
}

#[test]
fn test_freshness_stripped_in_function_argument() {
    test_no_errors(
        r#"
function foo(arg: { a: number }): void {}
let x = { a: 1, b: 2 };  // Freshness stripped
foo(x);  // Should NOT trigger EPC (x is non-fresh)
"#,
    );
}

#[test]
fn test_fresh_literal_in_function_argument_triggers_epc() {
    test_expect_error(
        r#"
function foo(arg: { a: number }): void {}
foo({ a: 1, b: 2 });  // Should trigger EPC for 'b'
"#,
        "Object literal may only specify known properties",
    );
}

#[test]
fn test_freshness_stripped_allows_passing_to_stricter_type() {
    test_no_errors(
        r#"
type Strict = { a: number };
let x = { a: 1, b: 2 };  // Freshness stripped, type is { a: number, b: number }
let y: Strict = x;  // Should PASS (structural typing with excess props)
let z: Strict = x;  // Should PASS again
"#,
    );
}

#[test]
fn test_nested_object_freshness_stripped() {
    test_no_errors(
        r#"
let x = { a: { b: 1, c: 2 } };  // Freshness stripped
let y: { a: { b: number } } = x;  // Should NOT trigger EPC
"#,
    );
}

// TODO: Investigate destructuring from literals
// These tests are skipped because destructuring from array/object literals
// in a single declaration (e.g., `let [x] = [{...}]`) may need special handling
// that's beyond the scope of basic freshness stripping.
// #[test]
// fn test_array_destructuring_strips_freshness() {
//     test_no_errors(
//         r#"
// let [x] = [{ a: 1, b: 2 }];  // Freshness stripped
// let y: { a: number } = x;  // Should NOT trigger EPC
// "#,
//     );
// }

// #[test]
// fn test_object_destructuring_strips_freshness() {
//     test_no_errors(
//         r#"
// let { x } = { x: { a: 1, b: 2 } };  // Freshness stripped
// let y: { a: number } = x;  // Should NOT trigger EPC
// "#,
//     );
// }

#[test]
fn test_freshness_preserved_for_const_with_no_type_annotation() {
    // Even const declarations strip freshness for consistency
    test_no_errors(
        r#"
const x = { a: 1, b: 2 };  // Type is { a: 1, b: 2 }, but freshness is still stripped
let y: { a: number } = x;  // Should NOT trigger EPC
"#,
    );
}

#[test]
fn test_multiple_variables_from_same_literal() {
    // Each variable declaration gets its own fresh literal
    test_expect_error(
        r#"
let x: { a: number } = { a: 1, b: 2 };  // Should trigger EPC
let y: { a: number } = { a: 1, b: 2 };  // Should trigger EPC (different literal)
"#,
        "Object literal may only specify known properties",
    );
}

#[test]
fn test_fresh_variable_can_be_reassigned_with_non_fresh_source() {
    test_no_errors(
        r#"
let source = { a: 1, b: 2 };  // Freshness stripped
let target: { a: number } = source;  // Should NOT trigger EPC
target = source;  // Should NOT trigger EPC (source is non-fresh)
"#,
    );
}
