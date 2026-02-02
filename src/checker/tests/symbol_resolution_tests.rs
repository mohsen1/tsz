//! Tests for symbol resolution behavior in the checker.

use crate::binder::BinderState;
use crate::checker::context::CheckerOptions;
use crate::checker::state::CheckerState;
use crate::parser::ParserState;
use crate::solver::TypeInterner;
use std::sync::Arc;

fn collect_diagnostics(source: &str) -> Vec<crate::checker::types::Diagnostic> {
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
        CheckerOptions::default(),
    );

    checker.check_source_file(root);
    checker.ctx.diagnostics.clone()
}

fn collect_diagnostics_with_libs(source: &str) -> Vec<crate::checker::types::Diagnostic> {
    let lib_files = crate::test_fixtures::load_lib_files_for_test();
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file_with_libs(parser.get_arena(), root, &lib_files);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions::default(),
    );

    if !lib_files.is_empty() {
        let lib_contexts: Vec<_> = lib_files
            .iter()
            .map(|lib| crate::checker::context::LibContext {
                arena: Arc::clone(&lib.arena),
                binder: Arc::clone(&lib.binder),
            })
            .collect();
        checker.ctx.set_lib_contexts(lib_contexts);
    }

    checker.check_source_file(root);
    checker.ctx.diagnostics.clone()
}

#[test]
fn test_symbol_resolution_value_shadow_does_not_block_type_lookup() {
    let source = r#"
type Foo = { a: number };
function f() {
    const Foo = 123;
    let x: Foo;
    return x;
}
"#;

    let diagnostics = collect_diagnostics(source);
    let value_as_type = diagnostics.iter().filter(|d| d.code == 2749).count();
    let cannot_find = diagnostics.iter().filter(|d| d.code == 2304).count();

    assert_eq!(
        value_as_type, 0,
        "Expected no TS2749 for type lookup through value shadowing, got: {:?}",
        diagnostics
    );
    assert_eq!(
        cannot_find, 0,
        "Expected no TS2304 for Foo in type position, got: {:?}",
        diagnostics
    );
}

#[test]
fn test_symbol_resolution_class_member_not_resolved_as_value() {
    let source = r#"
class C {
    foo: number;
    method() {
        foo;
    }
}
"#;

    let diagnostics = collect_diagnostics(source);
    let ts2304_count = diagnostics.iter().filter(|d| d.code == 2304).count();

    assert!(
        ts2304_count >= 1,
        "Expected TS2304 for unqualified class member reference, got: {:?}",
        diagnostics
    );
}

#[test]
fn test_symbol_resolution_type_params_in_nested_scopes() {
    let source = r#"
function outer<T>() {
    function inner<U>() {
        let a: T;
        let b: U;
        return [a, b];
    }
}
"#;

    let diagnostics = collect_diagnostics(source);
    let type_param_errors = diagnostics.iter().filter(|d| d.code == 2749).count();
    let cannot_find = diagnostics.iter().filter(|d| d.code == 2304).count();

    assert_eq!(
        type_param_errors, 0,
        "Expected no TS2749 for nested type parameters, got: {:?}",
        diagnostics
    );
    assert_eq!(
        cannot_find, 0,
        "Expected no TS2304 for nested type parameters, got: {:?}",
        diagnostics
    );
}

#[test]
fn test_symbol_resolution_global_console_with_libs() {
    let diagnostics = collect_diagnostics_with_libs(r#"console.log("ok");"#);
    let ts2304_count = diagnostics.iter().filter(|d| d.code == 2304).count();
    let ts2584_count = diagnostics.iter().filter(|d| d.code == 2584).count();

    assert_eq!(
        ts2304_count, 0,
        "Expected no TS2304 for console with lib files, got: {:?}",
        diagnostics
    );
    assert_eq!(
        ts2584_count, 0,
        "Expected no TS2584 for console with lib files, got: {:?}",
        diagnostics
    );
}
