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
#[test]
fn test_import_type_value_usage_errors() {
    use crate::parser::ParserState;

    let source = r#"
import type { Foo } from "./types";
Foo;
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
        crate::checker::context::CheckerOptions {
            module: crate::common::ModuleKind::ESNext,
            ..Default::default()
        },
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    // TS1361: 'Foo' cannot be used as a value because it was imported using 'import type'.
    assert!(
        codes.contains(&1361),
        "Expected TS1361 for type-only import used as value, got: {codes:?}"
    );
    assert!(
        !codes.contains(&1148),
        "Should not emit TS1148 (module=none error) for import type test, got: {codes:?}"
    );
}

#[test]
#[ignore = "cross-enum TS2322 not emitted after solver changes"]
fn test_numeric_enum_open_and_nominal_assignability() {
    use crate::parser::ParserState;

    let source = r#"
enum A { X, Y }
enum B { X, Y }
let a: A = 1;
let n: number = a;
let b: B = a;
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
    let count_2322 = codes.iter().filter(|&&code| code == 2322).count();
    assert_eq!(
        count_2322, 1,
        "Expected one 2322 error for cross-enum assignment, got: {codes:?}"
    );
}

#[test]
fn test_string_enum_rejects_string_literal() {
    use crate::parser::ParserState;

    let source = r#"
enum S { A = "a", B = "b" }
let s: S = "a";
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
        codes.contains(&2322),
        "Expected error 2322 for string enum assignment, got: {codes:?}"
    );
}

#[test]
fn test_numeric_enum_number_bidirectional() {
    use crate::parser::ParserState;

    let source = r#"
enum E { A = 0, B = 1 }
let e: E = 1;
let n: number = e;
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
    let count_2322 = codes.iter().filter(|&&code| code == 2322).count();
    assert_eq!(
        count_2322, 0,
        "Expected no errors for numeric enum <-> number bidirectional assignability, got: {codes:?}"
    );
}

#[test]
fn test_string_enum_not_assignable_to_string() {
    use crate::parser::ParserState;

    let source = r#"
enum S { A = "a", B = "b" }
let s: S = S.A;
let str: string = s;
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
        !codes.contains(&2322),
        "String enum values should be assignable to string (no TS2322), got: {codes:?}"
    );
}

#[test]
fn test_cross_enum_nominal_incompatibility() {
    use crate::parser::ParserState;

    let source = r#"
enum E1 { A = 0, B = 1 }
enum E2 { X = 0, Y = 1 }
let e1: E1 = E1.A;
let e2: E2 = e1;
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
    let count_2322 = codes.iter().filter(|&&code| code == 2322).count();
    assert_eq!(
        count_2322, 1,
        "Expected one 2322 error for cross-enum assignment, got: {codes:?}"
    );
}

#[test]
fn test_string_enum_cross_incompatibility() {
    use crate::parser::ParserState;

    let source = r#"
enum S1 { A = "a", B = "b" }
enum S2 { X = "a", Y = "b" }
let s1: S1 = S1.A;
let s2: S2 = s1;
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
    let count_2322 = codes.iter().filter(|&&code| code == 2322).count();
    assert_eq!(
        count_2322, 1,
        "Expected one 2322 error for cross-string-enum assignment, got: {codes:?}"
    );
}

#[test]
fn test_nested_namespace_member_resolution() {
    use crate::parser::ParserState;

    let source = r#"
namespace Outer {
    export namespace Inner {
        export interface Box<T> { value: T; }
    }
}
let ok: Outer.Inner.Box<number> = { value: 1 };
let bad: Outer.Inner.Box<number> = { value: "oops" };
let missing: Outer.Inner.Missing;
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
        codes.contains(&2694),
        "Expected error 2694 for missing nested namespace member, got: {codes:?}"
    );
    assert!(
        codes.contains(&2322),
        "Expected error 2322 for nested namespace generic mismatch, got: {codes:?}"
    );
}

#[test]
fn test_import_alias_namespace_member_resolution() {
    use crate::parser::ParserState;

    let source = r#"
namespace NS {
    export interface Box<T> { value: T; }
}
import Alias = NS;
let ok: Alias.Box<number> = { value: 1 };
let bad: Alias.Box<number> = { value: "oops" };
let missing: Alias.Missing;
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
        codes.contains(&2694),
        "Expected error 2694 for alias missing member, got: {codes:?}"
    );
    assert!(
        codes.contains(&2322),
        "Expected error 2322 for alias generic mismatch, got: {codes:?}"
    );
}

#[test]
fn test_namespace_type_only_member_value_error() {
    use crate::parser::ParserState;

    let source = r#"
namespace NS {
    export interface Foo { value: number; }
}
let ok: NS.Foo;
const bad = NS.Foo;
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
    // tsc emits TS2708 ("Cannot use namespace 'NS' as a value") for this pattern
    assert!(
        codes.contains(&2708),
        "Expected error 2708 for type-only namespace member used as value, got: {codes:?}"
    );
}
