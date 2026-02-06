//! Integration tests for literal type widening in variable declarations.

use tsz_binder::BinderState;
use crate::context::CheckerOptions;
use crate::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;
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
        let lib_contexts: Vec<crate::context::LibContext> = ctx
            .lib_files
            .iter()
            .map(|lib| crate::context::LibContext {
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
        .filter(|d| d.category == crate::types::DiagnosticCategory::Error)
        .collect();

    assert!(
        errors.is_empty(),
        "Expected no errors, got {}: {:?}",
        errors.len(),
        errors
    );
}

#[test]
fn test_const_object_literal_property_widening() {
    // Properties of const object literals should be widened
    test_no_errors(
        r#"
        const obj = { x: 1 };
        obj.x = 2; // Should be allowed - x is number, not literal 1
        "#,
    );
}

#[test]
fn test_let_object_literal_property_widening() {
    // Properties of let object literals should be widened
    test_no_errors(
        r#"
        let obj = { x: 1 };
        obj.x = 2; // Should be allowed - x is number
        "#,
    );
}

#[test]
fn test_nested_object_property_widening() {
    // Nested object properties should be widened
    test_no_errors(
        r#"
        const obj = { a: { b: "hello" } };
        obj.a.b = "world"; // Should be allowed - b is string
        "#,
    );
}

#[test]
fn test_const_primitive_literal_preserved() {
    // const with primitive literals should preserve the literal type
    test_no_errors(
        r#"
        const x = 1;
        const y: 1 = x; // Should work - x is literal 1
        "#,
    );
}

#[test]
fn test_let_primitive_literal_widened() {
    // let with primitive literals should widen to the primitive type
    test_no_errors(
        r#"
        let x = 1;
        x = 2; // Should be allowed - x is number
        "#,
    );
}

#[test]
fn test_for_of_loop_variable_widening() {
    // Loop variables in for-of should be widened for let, preserved for const
    test_no_errors(
        r#"
        for (let x of [1, 2, 3]) {
            x = 4; // Should be allowed - x is number
        }
        "#,
    );
}
