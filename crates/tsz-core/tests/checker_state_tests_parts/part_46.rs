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
fn test_type_predicate_return_no_ts2304() {
    use crate::parser::ParserState;

    let source = r#"
declare function isString(value: unknown): value is string;
declare function assertIsString(value: unknown): asserts value is string;
declare function assertDefined<T>(value: T): asserts value;
const assertFn: (value: unknown) => asserts value = value => {};
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

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&2304),
        "Unexpected TS2304 for type predicate returns, got: {codes:?}"
    );
}

#[test]
fn test_type_predicate_this_return_no_ts2304() {
    use crate::parser::ParserState;

    let source = r#"
interface Foo {
    ok: boolean;
}

const obj = {
    m(): this is Foo {
        return this.ok;
    }
};
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

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&2304),
        "Unexpected TS2304 for `this is` return type, got: {codes:?}"
    );
}

#[test]
fn test_exports_reference_no_ts2304() {
    use crate::parser::ParserState;

    let source = r#"
exports.foo = 1;
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

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&2304),
        "Unexpected TS2304 for exports reference, got: {codes:?}"
    );
}

#[test]
fn test_mapped_type_param_no_ts2304() {
    use crate::parser::ParserState;

    let source = r#"
type Types = "boolean" | "string";
type Properties<T extends { [key: string]: Types }> = {
    readonly [key in keyof T]: T[key] extends "boolean" ? boolean : string
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
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&2304),
        "Unexpected TS2304 for mapped type parameter, got: {codes:?}"
    );
}

#[test]
fn test_accessor_modifier_declaration_no_ts2304() {
    use crate::parser::ParserState;

    let source = r#"
interface I1 {
    accessor a: number;
}

accessor class C3 {}
accessor var V1: any;
accessor export default V1;
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
        "Unexpected TS2304 for accessor modifier recovery, got: {codes:?}"
    );
}

#[test]
fn test_namespace_sibling_export_resolves() {
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::parser::ParserState;

    let source = r#"
namespace Utils {
    export const x = 1;
}

namespace Utils {
    export const y = x;
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

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&diagnostic_codes::CANNOT_FIND_NAME),
        "Unexpected TS2304 for namespace sibling export, got: {codes:?}"
    );
}

#[test]
fn test_namespace_type_literal_resolves_members() {
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::parser::ParserState;

    let source = r#"
namespace A {
    class Point { x: number = 0; y: number = 0; }
    export type Square = {
        top: { left: Point; right: Point };
        bottom: { left: Point; right: Point };
    };
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

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&diagnostic_codes::CANNOT_FIND_NAME),
        "Unexpected TS2304 for namespace type literal members, got: {codes:?}"
    );
}

#[test]
fn test_namespace_type_query_resolves_alias() {
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::parser::ParserState;

    let source = r#"
namespace A {
    export class Point {}
}

namespace C {
    import a = A;
    type AliasType = typeof a;
    type PointType = a.Point;
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

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&diagnostic_codes::CANNOT_FIND_NAME),
        "Unexpected TS2304 for namespace import alias type query, got: {codes:?}"
    );
}

#[test]
fn test_declare_global_merges_into_global_scope() {
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::parser::ParserState;

    let source = r#"
export {};

declare global {
    interface GlobalThing { value: number; }
    var globalValue: GlobalThing;
}

const x = globalValue;
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

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&diagnostic_codes::CANNOT_FIND_NAME),
        "Unexpected TS2304 for declare global, got: {codes:?}"
    );
}

#[test]
fn test_ambient_module_declaration_resolves_import() {
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::parser::ParserState;

    let source = r#"
declare module "foo" {
    export interface Options { value: number; }
}

import { Options } from "foo";
const opts: Options = { value: 1 };
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

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&diagnostic_codes::CANNOT_FIND_NAME),
        "Unexpected TS2304 for ambient module import, got: {codes:?}"
    );
}

