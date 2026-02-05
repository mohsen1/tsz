//! Tests for symbol resolution behavior in the checker.

use crate::binder::BinderState;
use crate::checker::context::CheckerOptions;
use crate::checker::state::CheckerState;
use crate::parser::ParserState;
use crate::solver::TypeInterner;

use crate::test_fixtures::{merge_shared_lib_symbols, setup_lib_contexts};

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

    // Enable TS2304 emission for unresolved names
    checker.ctx.report_unresolved_imports = true;

    checker.check_source_file(root);
    checker.ctx.diagnostics.clone()
}

fn collect_diagnostics_with_libs(source: &str) -> Vec<crate::checker::types::Diagnostic> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);

    // Enable TS2304 emission for unresolved names
    checker.ctx.report_unresolved_imports = true;

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
    // Filter out TS2318 (Cannot find global type) which is expected when no lib files are loaded
    let filtered: Vec<_> = diagnostics.iter().filter(|d| d.code != 2318).collect();
    let ts2304_count = filtered.iter().filter(|d| d.code == 2304).count();

    assert!(
        ts2304_count >= 1,
        "Expected TS2304 for unqualified class member reference, got: {:?}",
        filtered
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
#[ignore = "TODO: Lib file loading behavior change - this test was passing with empty lib loading (load_lib_files_for_test returned empty Vec), but with embedded libs the behavior differs. The test expects TS2584 for console (DOM global not in ES5), but embedded libs may have different behavior. Need to investigate whether console should be found in embedded ES5 libs or if the error emission is different."]
fn test_symbol_resolution_global_console_with_libs() {
    let diagnostics = collect_diagnostics_with_libs(r#"console.log("ok");"#);
    let ts2304_count = diagnostics.iter().filter(|d| d.code == 2304).count();
    let ts2584_count = diagnostics.iter().filter(|d| d.code == 2584).count();

    // console is a DOM global, not an ES5 global, so TS2584 is expected
    // when only ES5 lib is loaded
    assert_eq!(
        ts2304_count, 0,
        "Expected no TS2304 for console with lib files, got: {:?}",
        diagnostics
    );
    assert_eq!(
        ts2584_count, 1,
        "Expected TS2584 for console (DOM global) with ES5 lib only, got: {:?}",
        diagnostics
    );
}

#[test]
fn test_symbol_resolution_parameter_in_nested_function() {
    let source = r#"
function outer(x: number) {
    function inner() {
        const y = x + 1;
        return y;
    }
    return inner();
}
"#;

    let diagnostics = collect_diagnostics(source);
    let ts2304_count = diagnostics.iter().filter(|d| d.code == 2304).count();

    assert_eq!(
        ts2304_count, 0,
        "Expected no TS2304 for outer parameter in nested function, got: {:?}",
        diagnostics
    );
}

#[test]
fn test_symbol_resolution_block_shadowing_is_scoped() {
    let source = r#"
let x = 1;
{
    let x = 2;
    x;
}
x;
"#;

    let diagnostics = collect_diagnostics(source);
    let ts2304_count = diagnostics.iter().filter(|d| d.code == 2304).count();

    assert_eq!(
        ts2304_count, 0,
        "Expected no TS2304 for block-scoped shadowing, got: {:?}",
        diagnostics
    );
}

#[test]
fn test_symbol_resolution_namespace_type_and_value() {
    let source = r#"
namespace N {
    export interface I { a: number; }
    export const value = 1;
}
let x: N.I;
let y = N.value;
"#;

    let diagnostics = collect_diagnostics(source);
    let ts2304_count = diagnostics.iter().filter(|d| d.code == 2304).count();
    let ts2749_count = diagnostics.iter().filter(|d| d.code == 2749).count();

    assert_eq!(
        ts2304_count, 0,
        "Expected no TS2304 for namespace members, got: {:?}",
        diagnostics
    );
    assert_eq!(
        ts2749_count, 0,
        "Expected no TS2749 for namespace type usage, got: {:?}",
        diagnostics
    );
}

#[test]
fn test_symbol_resolution_nested_namespace_qualified_type() {
    let source = r#"
namespace Outer {
    export namespace Inner {
        export interface I { a: number; }
    }
}
let x: Outer.Inner.I;
"#;

    let diagnostics = collect_diagnostics(source);
    let ts2304_count = diagnostics.iter().filter(|d| d.code == 2304).count();
    let ts2749_count = diagnostics.iter().filter(|d| d.code == 2749).count();

    assert_eq!(
        ts2304_count, 0,
        "Expected no TS2304 for nested namespace type, got: {:?}",
        diagnostics
    );
    assert_eq!(
        ts2749_count, 0,
        "Expected no TS2749 for nested namespace type, got: {:?}",
        diagnostics
    );
}

#[test]
fn test_symbol_resolution_type_alias_in_block_scope() {
    let source = r#"
function f() {
    {
        type T = { a: number };
        let x: T;
        return x;
    }
}
"#;

    let diagnostics = collect_diagnostics(source);
    let ts2304_count = diagnostics.iter().filter(|d| d.code == 2304).count();
    let ts2749_count = diagnostics.iter().filter(|d| d.code == 2749).count();

    assert_eq!(
        ts2304_count, 0,
        "Expected no TS2304 for block-scoped type alias, got: {:?}",
        diagnostics
    );
    assert_eq!(
        ts2749_count, 0,
        "Expected no TS2749 for block-scoped type alias, got: {:?}",
        diagnostics
    );
}

#[test]
fn test_symbol_resolution_global_array_with_libs() {
    let diagnostics = collect_diagnostics_with_libs("let xs: Array<string> = [];");
    let ts2304_count = diagnostics.iter().filter(|d| d.code == 2304).count();
    let ts2318_count = diagnostics.iter().filter(|d| d.code == 2318).count();

    assert_eq!(
        ts2304_count, 0,
        "Expected no TS2304 for Array with lib files, got: {:?}",
        diagnostics
    );
    assert_eq!(
        ts2318_count, 0,
        "Expected no TS2318 for Array with lib files, got: {:?}",
        diagnostics
    );
}

#[test]
fn test_symbol_resolution_type_param_shadowing() {
    let source = r#"
function outer<T>() {
    function inner<T>() {
        let x: T;
        return x;
    }
}
"#;

    let diagnostics = collect_diagnostics(source);
    let ts2304_count = diagnostics.iter().filter(|d| d.code == 2304).count();
    let ts2749_count = diagnostics.iter().filter(|d| d.code == 2749).count();

    assert_eq!(
        ts2304_count, 0,
        "Expected no TS2304 for shadowed type parameters, got: {:?}",
        diagnostics
    );
    assert_eq!(
        ts2749_count, 0,
        "Expected no TS2749 for shadowed type parameters, got: {:?}",
        diagnostics
    );
}
