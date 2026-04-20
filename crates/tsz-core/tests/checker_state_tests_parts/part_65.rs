//! Tests for Checker - Type checker using `NodeArena` and Solver
//!
//! This module contains comprehensive type checking tests organized into categories:
//! - Basic type checking (creation, intrinsic types, type interning)
//! - Type compatibility and assignability
//! - Excess property checking
//! - Function overloads and call resolution
//! - Generic types and type inference
//! - Control flow analysis
//! - Error diagnostics
use crate::binder::BinderState;
use crate::checker::state::CheckerState;
use crate::parser::ParserState;
use crate::parser::node::NodeArena;
use crate::test_fixtures::{TestContext, merge_shared_lib_symbols, setup_lib_contexts};
use tsz_solver::{TypeId, TypeInterner, Visibility, types::RelationCacheKey, types::TypeData};

// =============================================================================
// Basic Type Checker Tests
// =============================================================================
/// Test that TS2304 is emitted for undeclared variable in object property value.
#[test]
fn test_ts2304_undeclared_var_in_object_literal() {
    use crate::parser::ParserState;

    let source = r#"
const obj = {
    name: undeclaredName,
    value: 42
};
"#;

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
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.ctx.report_unresolved_imports = true;
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2304),
        "Expected TS2304 for undeclared variable in object literal, got: {codes:?}"
    );
}

/// Test that TS2304 is emitted for undeclared variable in conditional (ternary) expression.
#[test]
fn test_ts2304_undeclared_var_in_conditional() {
    use crate::parser::ParserState;

    let source = r#"
const result = true ? undeclaredTrue : 0;
"#;

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
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.ctx.report_unresolved_imports = true;
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2304),
        "Expected TS2304 for undeclared variable in conditional, got: {codes:?}"
    );
}

/// Test that TS2304 is emitted for undeclared class in extends clause.
#[test]
fn test_ts2304_undeclared_class_in_extends() {
    use crate::parser::ParserState;

    let source = r#"
class Child extends MissingParent {}
"#;

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
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.ctx.report_unresolved_imports = true;
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2304),
        "Expected TS2304 for undeclared class in extends clause, got: {codes:?}"
    );
}

/// Test that TS2304 is emitted for undeclared interface in implements clause.
#[test]
fn test_ts2304_undeclared_interface_in_implements() {
    use crate::parser::ParserState;

    let source = r#"
class MyClass implements MissingInterface {}
"#;

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
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.ctx.report_unresolved_imports = true;
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2304),
        "Expected TS2304 for undeclared interface in implements clause, got: {codes:?}"
    );
}

/// Test that TS2304 is emitted for undeclared variable in template literal expression.
#[test]
fn test_ts2304_undeclared_var_in_template_literal() {
    use crate::parser::ParserState;

    let source = r#"
const msg = `Hello ${undeclaredName}!`;
"#;

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
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.ctx.report_unresolved_imports = true;
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2304),
        "Expected TS2304 for undeclared variable in template literal, got: {codes:?}"
    );
}

/// Test that TS2304 is emitted for undeclared variable in for-of loop.
#[test]
fn test_ts2304_undeclared_var_in_for_of() {
    use crate::parser::ParserState;

    let source = r#"
for (const item of undeclaredIterable) {
    let x = item;
}
"#;

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
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.ctx.report_unresolved_imports = true;
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2304),
        "Expected TS2304 for undeclared variable in for-of loop, got: {codes:?}"
    );
}

/// Test that no TS2304 is emitted for a properly declared variable.
#[test]
fn test_no_ts2304_for_declared_variable() {
    use crate::parser::ParserState;

    let source = r#"
const declaredVar = 5;
const result = declaredVar + 1;
"#;

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
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&2304),
        "Unexpected TS2304 for declared variable, got: {codes:?}"
    );
}

/// Test that no TS2304 is emitted for hoisted function declaration.
#[test]
fn test_no_ts2304_for_hoisted_function() {
    use crate::parser::ParserState;

    let source = r#"
// Call before declaration (should work due to hoisting)
const result = hoistedFn();

function hoistedFn() {
    return 42;
}
"#;

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
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&2304),
        "Unexpected TS2304 for hoisted function, got: {codes:?}"
    );
}

/// Test that no TS2304 is emitted for var used after declaration.
#[test]
fn test_no_ts2304_for_var_used_after_declaration() {
    use crate::parser::ParserState;

    let source = r#"
function test() {
    var x = 5;
    return x + 1;
}
"#;

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
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&2304),
        "Unexpected TS2304 for var used after declaration, got: {codes:?}"
    );
}

// =============================================================================
// Duplicate Identifier Tests (TS2300)
// =============================================================================

/// Test that function overloads do NOT emit TS2300
#[test]
fn test_function_overloads_no_ts2300() {
    use crate::parser::ParserState;

    let source = r#"
function foo(x: string): void;
function foo(x: number): void;
function foo(x: string | number): void {
    console.log(x);
}
"#;

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
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&2300),
        "Function overloads should NOT emit TS2300, got: {codes:?}"
    );
}

