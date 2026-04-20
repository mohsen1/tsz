// Tests for Checker - Type checker using `NodeArena` and Solver
//
// This module contains comprehensive type checking tests organized into categories:
// - Basic type checking (creation, intrinsic types, type interning)
// - Type compatibility and assignability
// - Excess property checking
// - Function overloads and call resolution
// - Generic types and type inference
// - Control flow analysis
// - Error diagnostics
use crate::binder::BinderState;
use crate::checker::state::CheckerState;
use crate::parser::ParserState;
use crate::parser::node::NodeArena;
use crate::test_fixtures::{TestContext, merge_shared_lib_symbols, setup_lib_contexts};
use tsz_solver::{TypeId, TypeInterner, Visibility, types::RelationCacheKey, types::TypeData};

// =============================================================================
// Basic Type Checker Tests
// =============================================================================
/// Test that non-circular imports don't trigger TS2303
#[test]
fn test_ts2303_no_error_for_different_module() {
    use crate::checker::diagnostics::diagnostic_codes;

    let source = r#"
declare module "moduleA" {
    export class A {}
}

declare module "moduleB" {
    import A = require("moduleA");
    export = A;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(
        parser.get_diagnostics().is_empty(),
        "Parse errors: {:?}",
        parser.get_diagnostics()
    );

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let ts2303_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::CIRCULAR_DEFINITION_OF_IMPORT_ALIAS)
        .collect();

    assert_eq!(
        ts2303_errors.len(),
        0,
        "Expected 0 TS2303 errors for non-circular import, got {}. Diagnostics: {:?}",
        ts2303_errors.len(),
        checker.ctx.diagnostics
    );
}

#[test]
fn test_ts2502_repro_circular_var() {
    let source = "var x: typeof x;";

    // Manually parse to get the root index
    let mut parser = crate::parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = crate::binder::BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = tsz_solver::TypeInterner::new();
    let mut checker = crate::checker::state::CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    crate::test_fixtures::setup_lib_contexts(&mut checker);

    checker.check_source_file(root);

    let diagnostics: Vec<_> = checker.ctx.diagnostics.iter().collect();
    assert!(
        diagnostics.iter().any(|d| d.code == 2502),
        "Expected TS2502 for circular reference, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts2451_global_redeclaration() {
    let source = "
    const x = 1;
    declare global {
        const x: number;
    }
    ";

    let mut parser = crate::parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = crate::binder::BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = tsz_solver::TypeInterner::new();
    let mut checker = crate::checker::state::CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    crate::test_fixtures::setup_lib_contexts(&mut checker);

    checker.check_source_file(root);

    let diagnostics: Vec<_> = checker.ctx.diagnostics.iter().collect();
    assert!(
        diagnostics.iter().any(|d| d.code == 2451),
        "Expected TS2451 for global redeclaration, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts2313_simple_circular_type_alias() {
    let source = "type T = T;";

    let mut parser = crate::parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = crate::binder::BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = tsz_solver::TypeInterner::new();
    let mut checker = crate::checker::state::CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    crate::test_fixtures::setup_lib_contexts(&mut checker);

    checker.check_source_file(root);

    let diagnostics: Vec<_> = checker.ctx.diagnostics.iter().collect();
    assert!(
        diagnostics.iter().any(|d| d.code == 2313 || d.code == 2456),
        "Expected TS2313/TS2456 for simple circular type alias, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts2313_indirect_circular_type_alias() {
    let source = "
            type A = B;
            type B = A;
            ";

    let mut parser = crate::parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = crate::binder::BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = tsz_solver::TypeInterner::new();
    let mut checker = crate::checker::state::CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    crate::test_fixtures::setup_lib_contexts(&mut checker);

    checker.check_source_file(root);

    let diagnostics: Vec<_> = checker.ctx.diagnostics.iter().collect();
    assert!(
        diagnostics.iter().any(|d| d.code == 2313 || d.code == 2456),
        "Expected TS2313/TS2456 for indirect circular type alias, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts2310_circular_interface_inheritance() {
    let source = "
                interface A extends B {}
                interface B extends A {}
                ";

    let mut parser = crate::parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = crate::binder::BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = tsz_solver::TypeInterner::new();
    let mut checker = crate::checker::state::CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    crate::test_fixtures::setup_lib_contexts(&mut checker);

    checker.check_source_file(root);

    let diagnostics: Vec<_> = checker.ctx.diagnostics.iter().collect();
    assert!(
        diagnostics.iter().any(|d| d.code == 2310),
        "Expected TS2310 for circular interface inheritance, got: {diagnostics:?}"
    );
}

#[test]
fn test_namespace_export_binds_global() {
    let source = "
    export as namespace foo;
    export const x = 1;
    ";

    let mut parser = crate::parser::ParserState::new("test.d.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = crate::binder::BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    // Verify 'foo' is in global scope
    // Global scope is binder.scopes[0] usually (or binder.scope_stack bottom if active?)
    // After bind, scopes are in binder.scopes.
    // We need to find the global scope.
    // Assuming the first scope created is global.

    let global_scope = &binder.scopes[0]; // Is this safe assumption?
    assert!(
        global_scope.table.has("foo"),
        "Global scope should contain 'foo'"
    );
}

// ── TS1194: Export declarations in namespaces ──────────────────────────

#[test]
fn test_ts1194_export_in_non_ambient_namespace() {
    // `export { ... }` inside a regular namespace should emit TS1194.
    let source = r#"
        namespace Q {
            function _try() {}
            export { _try as try2 };
        }
    "#;

    let mut parser = crate::parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = crate::binder::BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = tsz_solver::TypeInterner::new();
    let mut checker = crate::checker::state::CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    crate::test_fixtures::setup_lib_contexts(&mut checker);

    checker.check_source_file(root);

    let diagnostics: Vec<_> = checker.ctx.diagnostics.iter().collect();
    assert!(
        diagnostics.iter().any(|d| d.code == 1194),
        "Expected TS1194 for export in non-ambient namespace, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts1194_no_error_in_ambient_namespace() {
    // `export { ... }` inside `declare namespace` should NOT emit TS1194.
    let source = r#"
        declare namespace Q {
            function _try(method: Function, ...args: any[]): any;
            export { _try as try2 };
        }
    "#;

    let mut parser = crate::parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = crate::binder::BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = tsz_solver::TypeInterner::new();
    let mut checker = crate::checker::state::CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    crate::test_fixtures::setup_lib_contexts(&mut checker);

    checker.check_source_file(root);

    let diagnostics: Vec<_> = checker.ctx.diagnostics.iter().collect();
    assert!(
        !diagnostics.iter().any(|d| d.code == 1194),
        "Should NOT emit TS1194 in ambient namespace, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts1194_no_error_in_dts_file() {
    // In `.d.ts` files, all namespaces are ambient, so no TS1194.
    let source = r#"
        namespace Q {
            function _try(): void;
            export { _try as try2 };
        }
    "#;

    let mut parser = crate::parser::ParserState::new("test.d.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = crate::binder::BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = tsz_solver::TypeInterner::new();
    let mut checker = crate::checker::state::CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.d.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    crate::test_fixtures::setup_lib_contexts(&mut checker);

    checker.check_source_file(root);

    let diagnostics: Vec<_> = checker.ctx.diagnostics.iter().collect();
    assert!(
        !diagnostics.iter().any(|d| d.code == 1194),
        "Should NOT emit TS1194 in .d.ts file, got: {diagnostics:?}"
    );
}
