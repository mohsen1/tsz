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
/// Test that interface merging does NOT emit TS2300
#[test]
fn test_interface_merging_no_ts2300() {
    use crate::parser::ParserState;

    let source = r#"
interface Foo {
    a: string;
}
interface Foo {
    b: number;
}
const x: Foo = { a: "hello", b: 42 };
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
        "Interface merging should NOT emit TS2300, got: {codes:?}"
    );
}

/// Test that namespace + function merging does NOT emit TS2300
#[test]
fn test_namespace_function_merging_no_ts2300() {
    use crate::parser::ParserState;

    let source = r#"
namespace MyUtils {
    export function helper(): void {
        console.log("helper");
    }
}
function MyUtils() {
    console.log("constructor");
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
        "Namespace + function merging should NOT emit TS2300, got: {codes:?}"
    );
}

/// Test that namespace + class merging does NOT emit TS2300
#[test]
fn test_namespace_class_merging_no_ts2300() {
    use crate::parser::ParserState;

    let source = r#"
namespace MyNamespace {
    export class MyClass {
        x: number = 42;
    }
}
class MyNamespace {
    y: string = "hello";
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
        "Namespace + class merging should NOT emit TS2300, got: {codes:?}"
    );
}

/// Test that class + interface merging does NOT emit TS2300
#[test]
fn test_class_interface_merging_no_ts2300() {
    use crate::parser::ParserState;

    let source = r#"
interface MyInterface {
    method(): void;
}
class MyInterface {
    method(): void {
        console.log("implementation");
    }
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
        "Class + interface merging should NOT emit TS2300, got: {codes:?}"
    );
}

/// Test that duplicate variable declarations DO emit TS2451 (block-scoped variable redeclaration)
#[test]
fn test_duplicate_variables_emits_ts2451() {
    use crate::parser::ParserState;

    let source = r#"
let x = 1;
let x = 2;
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
        codes.contains(&2451),
        "Duplicate variable declarations should emit TS2451, got: {codes:?}"
    );
}

/// Test that duplicate var declarations are allowed (function-scoped hoisting)
#[test]
fn test_duplicate_var_allowed() {
    use crate::parser::ParserState;

    let source = r#"
var x = 1;
var x = 2;
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
    // Duplicate var declarations should NOT emit TS2300 (they are merged by hoisting)
    assert!(
        !codes.contains(&2300),
        "Duplicate var declarations should be allowed, got: {codes:?}"
    );
}

/// Test that duplicate class declarations DO emit TS2300
#[test]
fn test_duplicate_class_emits_ts2300() {
    use crate::parser::ParserState;

    let source = r#"
class MyClass {
    x: number = 1;
}
class MyClass {
    y: string = "hello";
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
        codes.contains(&2300),
        "Duplicate class declarations should emit TS2300, got: {codes:?}"
    );
}

/// Test that method overloads do NOT emit TS2300
#[test]
fn test_method_overloads_no_ts2300() {
    use crate::parser::ParserState;

    let source = r#"
class MyClass {
    method(x: string): void;
    method(x: number): void;
    method(x: string | number): void {
        console.log(x);
    }
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

    let _codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();

    // Filter to only TS2300 errors for the "method" identifier
    let ts2300_method_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2300 && d.message_text.contains("method"))
        .collect();

    assert!(
        ts2300_method_errors.is_empty(),
        "Method overloads should NOT emit TS2300 for 'method', got {} errors: {:?}",
        ts2300_method_errors.len(),
        ts2300_method_errors
    );
}

/// Test that static and instance members with the same name do NOT emit TS2300
#[test]
fn test_static_instance_member_no_ts2300() {
    use crate::parser::ParserState;

    let source = r#"
class MyClass {
    static x: number = 1;
    x: number = 2;
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
        "Static and instance members with same name should NOT emit TS2300, got: {codes:?}"
    );
}

// =============================================================================
// Lib Symbol Merging Tests (SymbolId Collision Fix)
// =============================================================================

/// Regression test: When lib symbols are merged with unique IDs, basic global
/// types like Array and Object should resolve correctly without TS2318.
#[test]
fn test_lib_merge_no_ts2318_for_basic_globals() {
    use crate::test_fixtures::{merge_shared_lib_symbols, setup_lib_contexts};

    // Source that references Array and Object
    let source = r#"
const arr: Array<number> = [1, 2, 3];
const obj: Object = {};
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    // Verify lib symbols are merged
    assert!(
        binder.lib_symbols_are_merged(),
        "lib_symbols_merged should be true"
    );
    assert!(
        binder.file_locals.has("Array"),
        "Array should be in file_locals"
    );
    assert!(
        binder.file_locals.has("Object"),
        "Object should be in file_locals"
    );

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

    // Should NOT have TS2318 (global type not found)
    let ts2318_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2318)
        .collect();
    assert!(
        ts2318_errors.is_empty(),
        "Should not emit TS2318 for Array/Object when libs are properly merged, got: {ts2318_errors:?}"
    );
}

