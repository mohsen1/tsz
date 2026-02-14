//! Tests for TS2370: A rest parameter must be of an array type

use crate::CheckerState;
use crate::diagnostics::diagnostic_codes;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn has_error_ts2370(source: &str) -> bool {
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
        crate::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    checker
        .ctx
        .diagnostics
        .iter()
        .any(|d| d.code == diagnostic_codes::A_REST_PARAMETER_MUST_BE_OF_AN_ARRAY_TYPE)
}

#[test]
fn test_rest_parameter_non_array_type_emits_ts2370() {
    let source = r#"
        function f(x: string, ...rest: number) {
        }
    "#;

    assert!(has_error_ts2370(source));
}

#[test]
fn test_rest_parameter_array_type_ok() {
    let source = r#"
        function f(x: string, ...rest: number[]) {
        }
    "#;

    assert!(!has_error_ts2370(source));
}

#[test]
fn test_rest_parameter_tuple_type_ok() {
    let source = r#"
        function f(...rest: [string, number]) {
        }
    "#;

    assert!(!has_error_ts2370(source));
}

#[test]
fn test_rest_parameter_no_type_annotation_ok() {
    let source = r#"
        function f(...rest) {
        }
    "#;

    assert!(!has_error_ts2370(source));
}

#[test]
fn test_rest_parameter_array_generic_ok() {
    let source = r#"
        function f<T>(...rest: T[]) {
        }
    "#;

    assert!(!has_error_ts2370(source));
}

#[test]
fn test_rest_parameter_in_method() {
    let source = r#"
        class C {
            method(...rest: string) {
            }
        }
    "#;

    assert!(has_error_ts2370(source));
}

#[test]
fn test_rest_parameter_in_constructor() {
    let source = r#"
        class C {
            constructor(...rest: boolean) {
            }
        }
    "#;

    assert!(has_error_ts2370(source));
}

#[test]
fn test_rest_parameter_in_arrow_function() {
    let source = r#"
        const f = (...rest: number) => {};
    "#;

    assert!(has_error_ts2370(source));
}
