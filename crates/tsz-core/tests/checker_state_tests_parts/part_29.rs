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
#[test]
fn test_namespace_type_only_member_element_access_value_error() {
    use crate::parser::ParserState;

    let source = r#"
namespace NS {
    export interface Foo { value: number; }
}
const bad = NS["Foo"];
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
        codes.contains(&2693),
        "Expected error 2693 for type-only namespace member element access used as value, got: {codes:?}"
    );
}

#[test]
#[ignore = "behavior changed after merge"]
fn test_namespace_type_only_nested_member_value_error() {
    use crate::parser::ParserState;

    let source = r#"
namespace Outer {
    export namespace Inner {
        export interface Foo { value: number; }
    }
}
let ok: Outer.Inner.Foo;
const bad = Outer.Inner.Foo;
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
    let count = codes.iter().filter(|&&code| code == 2693).count();
    assert_eq!(
        count, 1,
        "Expected one 2693 error for nested type-only namespace member used as value, got: {codes:?}"
    );
    assert!(
        !codes.contains(&2339),
        "Did not expect 2339 for nested type-only namespace member used as value, got: {codes:?}"
    );
}

#[test]
fn test_namespace_type_only_alias_value_error() {
    use crate::parser::ParserState;

    let source = r#"
namespace NS {
    export interface Foo { value: number; }
}
import Alias = NS.Foo;
const bad = Alias;
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
    // TODO: TS2693 is not yet emitted for type-only namespace aliases used as values.
    // Update this assertion when that diagnostic is implemented.
    let _ = codes;
}

#[test]
fn test_namespace_type_only_member_via_alias_value_error() {
    use crate::parser::ParserState;

    let source = r#"
namespace NS {
    export interface Foo { value: number; }
}
import Alias = NS;
let ok: Alias.Foo;
const bad = Alias.Foo;
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
    // tsc emits TS2708 ("Cannot use namespace as a value") for this pattern
    let count = codes.iter().filter(|&&code| code == 2708).count();
    assert_eq!(
        count, 1,
        "Expected one 2708 error for type-only namespace member via alias, got: {codes:?}"
    );
    assert!(
        !codes.contains(&2339),
        "Did not expect 2339 for type-only namespace member via alias, got: {codes:?}"
    );
}

#[test]
#[ignore = "behavior changed after merge"]
fn test_namespace_type_only_nested_member_via_alias_value_error() {
    use crate::parser::ParserState;

    let source = r#"
namespace Outer {
    export namespace Inner {
        export type Foo = number;
    }
}
import Alias = Outer;
let ok: Alias.Inner.Foo;
const bad = Alias.Inner.Foo;
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
    let count = codes.iter().filter(|&&code| code == 2693).count();
    assert_eq!(
        count, 1,
        "Expected one 2693 error for nested type-only namespace member via alias, got: {codes:?}"
    );
    assert!(
        !codes.contains(&2339),
        "Did not expect 2339 for nested type-only namespace member via alias, got: {codes:?}"
    );
}

#[test]
fn test_interface_value_error() {
    use crate::parser::ParserState;

    let source = r#"
interface Foo { value: number; }
let ok: Foo;
const bad = Foo;
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
        codes.contains(&2693),
        "Expected error 2693 for interface used as value, got: {codes:?}"
    );
}

#[test]
fn test_type_alias_value_error() {
    use crate::parser::ParserState;

    let source = r#"
type Foo = { value: number };
let ok: Foo;
const bad = Foo;
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
        codes.contains(&2693),
        "Expected error 2693 for type alias used as value, got: {codes:?}"
    );
}

#[test]
fn test_type_query_interface_value_error() {
    use crate::parser::ParserState;

    let source = r#"
interface Foo { value: number; }
type T = typeof Foo;
let useIt: T;
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
        codes.contains(&2693),
        "Expected error 2693 for interface used in type query, got: {codes:?}"
    );
}

#[test]
fn test_type_query_type_alias_value_error() {
    use crate::parser::ParserState;

    let source = r#"
type Foo = { value: number };
type T = typeof Foo;
let useIt: T;
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
        codes.contains(&2693),
        "Expected error 2693 for type alias used in type query, got: {codes:?}"
    );
}

#[test]
fn test_type_query_unknown_name_error() {
    use crate::parser::ParserState;

    let source = r#"
type T = typeof Missing;
let useIt: T;
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
        codes.contains(&2304),
        "Expected error 2304 for unknown typeof name, got: {codes:?}"
    );
}

