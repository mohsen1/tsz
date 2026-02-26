//! Tests for TS2428: All declarations of 'X' must have identical type parameters.

use crate::CheckerState;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn get_diagnostics(source: &str) -> Vec<(u32, String)> {
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
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

fn has_error_with_code(source: &str, code: u32) -> bool {
    get_diagnostics(source).iter().any(|d| d.0 == code)
}

#[test]
fn generic_and_non_generic_interface_same_name_emits_ts2428() {
    let source = r#"
interface A {
    foo: string;
}
interface A<T> {
    bar: T;
}
"#;
    assert!(
        has_error_with_code(source, 2428),
        "Should emit TS2428 when generic and non-generic interfaces share a name"
    );
}

#[test]
fn same_interface_no_type_params_no_error() {
    let source = r#"
interface A {
    foo: string;
}
interface A {
    bar: number;
}
"#;
    assert!(
        !has_error_with_code(source, 2428),
        "Should NOT emit TS2428 when interfaces have identical (no) type params"
    );
}

#[test]
fn same_generic_interface_same_params_no_error() {
    let source = r#"
interface A<T> {
    foo: T;
}
interface A<T> {
    bar: T;
}
"#;
    assert!(
        !has_error_with_code(source, 2428),
        "Should NOT emit TS2428 when interfaces have identical type params"
    );
}

#[test]
fn different_arity_emits_ts2428() {
    let source = r#"
interface A<T> {
    x: T;
}
interface A<T, U> {
    y: T;
}
"#;
    assert!(
        has_error_with_code(source, 2428),
        "Should emit TS2428 when interface type parameter arity differs"
    );
}

#[test]
fn namespace_separate_blocks_emits_ts2428() {
    let source = r#"
namespace M3 {
    export interface A {
        foo: string;
    }
}

namespace M3 {
    export interface A<T> {
        bar: T;
    }
}
"#;
    assert!(
        has_error_with_code(source, 2428),
        "Should emit TS2428 for interfaces in separate namespace blocks with different type params"
    );
}

#[test]
fn namespace_same_block_emits_ts2428() {
    let source = r#"
namespace M {
    interface A<T> {
        bar: T;
    }
    interface A {
        foo: string;
    }
}
"#;
    assert!(
        has_error_with_code(source, 2428),
        "Should emit TS2428 for interfaces in same namespace block with different type params"
    );
}
