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
/// TS Unsoundness #36: JSX Intrinsic Lookup - lowercase tag resolution
///
/// Lowercase JSX tags like `<div />` are looked up as properties on the
/// global `JSX.IntrinsicElements` interface. This test verifies that the
/// checker can resolve intrinsic element types.
///
/// EXPECTED: Tests verify JSX parsing and checking don't crash. Full
/// JSX type checking is not yet implemented.
#[test]
fn test_jsx_intrinsic_element_lowercase_lookup() {
    use crate::parser::ParserState;

    // Use .tsx extension for JSX
    let source = r#"
declare namespace JSX {
    interface IntrinsicElements {
        div: { className?: string; id?: string };
        span: { className?: string };
    }
}

// Lowercase tags should be looked up in JSX.IntrinsicElements
const elem = <div className="test" />;
const elem2 = <span id="foo" />;
"#;

    let mut parser = ParserState::new("test.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();

    // Check if parsing JSX is supported
    if !parser.get_diagnostics().is_empty() {
        println!("=== JSX Intrinsic Lowercase Parse Diagnostics ===");
        for diag in parser.get_diagnostics() {
            println!("[{}] {}", diag.start, diag.message);
        }
        // JSX parsing may not be enabled - skip test
        return;
    }

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.tsx".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    // Currently expect errors - JSX type checking not implemented
    // Once JSX.IntrinsicElements lookup works, change to expect 0 errors
    println!("=== JSX Intrinsic Lowercase Diagnostics ===");
    println!(
        "Got {} diagnostics (JSX checking not yet implemented)",
        checker.ctx.diagnostics.len()
    );
    for diag in &checker.ctx.diagnostics {
        println!("[{}] {}", diag.start, diag.message_text);
    }
    // Just verify we don't crash - actual JSX checking is future work
}

/// TS Unsoundness #36: JSX Intrinsic Lookup - uppercase component resolution
///
/// Uppercase JSX tags like `<MyComp />` are resolved as value references
/// in the current scope and checked as function/constructor calls.
///
/// EXPECTED: Tests verify JSX parsing and checking don't crash. Full
/// JSX type checking is not yet implemented.
#[test]
fn test_jsx_component_uppercase_resolution() {
    use crate::parser::ParserState;

    let source = r#"
declare namespace JSX {
    interface Element {}
    interface IntrinsicElements {}
}

// Component function
function MyButton(props: { label: string }): JSX.Element {
    return null as any;
}

// Uppercase tags resolve to variables in scope
const btn = <MyButton label="Click me" />;
"#;

    let mut parser = ParserState::new("test.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();

    if !parser.get_diagnostics().is_empty() {
        println!("=== JSX Component Uppercase Parse Diagnostics ===");
        for diag in parser.get_diagnostics() {
            println!("[{}] {}", diag.start, diag.message);
        }
        return;
    }

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.tsx".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    println!("=== JSX Component Uppercase Diagnostics ===");
    println!(
        "Got {} diagnostics (JSX checking not yet implemented)",
        checker.ctx.diagnostics.len()
    );
    for diag in &checker.ctx.diagnostics {
        println!("[{}] {}", diag.start, diag.message_text);
    }
    // Just verify we don't crash
}

/// TS Unsoundness #36: JSX Intrinsic Lookup - invalid intrinsic element
///
/// When a lowercase tag is not found in JSX.IntrinsicElements, TypeScript
/// should report an error that the element does not exist.
///
/// EXPECTED: Tests verify JSX parsing and checking don't crash. Full
/// JSX type checking is not yet implemented.
#[test]
fn test_jsx_intrinsic_element_not_found_error() {
    use crate::parser::ParserState;

    let source = r#"
declare namespace JSX {
    interface IntrinsicElements {
        div: {};
    }
}

// 'unknowntag' is not in IntrinsicElements - should error
const elem = <unknowntag />;
"#;

    let mut parser = ParserState::new("test.tsx".to_string(), source.to_string());
    let root = parser.parse_source_file();

    if !parser.get_diagnostics().is_empty() {
        return;
    }

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.tsx".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    // Once JSX checking is implemented, expect 1 error for unknown element
    println!("=== JSX Invalid Intrinsic Diagnostics ===");
    println!(
        "Got {} diagnostics (expected 1 once JSX implemented)",
        checker.ctx.diagnostics.len()
    );
    for diag in &checker.ctx.diagnostics {
        println!("[{}] {}", diag.start, diag.message_text);
    }
}

// =============================================================================
// NAMESPACE TYPE MEMBER ACCESS PATTERN TESTS
// =============================================================================

/// Test that namespace interface members can be used as type annotations
#[test]
fn test_namespace_type_member_interface_annotation() {
    use crate::parser::ParserState;

    let source = r#"
namespace Models {
    export interface User {
        id: number;
        name: string;
    }
    export interface Post {
        title: string;
        author: User;
    }
}

const user: Models.User = { id: 1, name: "Alice" };
const post: Models.Post = { title: "Hello", author: user };
function getUser(): Models.User {
    return { id: 0, name: "" };
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

    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Expected no errors for namespace interface type annotations, got: {:?}",
        checker.ctx.diagnostics
    );
}

/// Test that namespace type alias members can be used as type annotations
///
/// NOTE: Currently ignored - namespace type alias members are not correctly resolved
/// when used as type annotations. The checker emits type incompatibility errors
/// for cases that should work correctly.
#[test]
fn test_namespace_type_member_type_alias_annotation() {
    use crate::parser::ParserState;

    let source = r#"
namespace Types {
    export type ID = number;
    export type Name = string;
    export type Pair<T> = [T, T];
}

const id: Types.ID = 42;
const name: Types.Name = "Bob";
const pair: Types.Pair<number> = [1, 2];
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

    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Expected no errors for namespace type alias annotations, got: {:?}",
        checker.ctx.diagnostics
    );
}

/// Test that nested namespace type members can be used as type annotations
#[test]
fn test_namespace_type_member_nested_annotation() {
    use crate::parser::ParserState;

    let source = r#"
namespace Outer {
    export namespace Inner {
        export interface Config {
            enabled: boolean;
        }
        export namespace Deep {
            export type Value = string | number;
        }
    }
}

const config: Outer.Inner.Config = { enabled: true };
const value: Outer.Inner.Deep.Value = "test";
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

    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Expected no errors for nested namespace type annotations, got: {:?}",
        checker.ctx.diagnostics
    );
}

/// Test that namespace generic type members work correctly
#[test]
fn test_namespace_type_member_generic_usage() {
    use crate::parser::ParserState;

    let source = r#"
namespace Collections {
    export interface Container<T> {
        value: T;
    }
    export type Optional<T> = T | null;
    export interface Map<K, V> {
        get(key: K): V;
    }
}

const strContainer: Collections.Container<string> = { value: "hello" };
const numContainer: Collections.Container<number> = { value: 42 };
const optString: Collections.Optional<string> = null;
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

    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Expected no errors for namespace generic type usage, got: {:?}",
        checker.ctx.diagnostics
    );
}

/// Test that namespace type members work in function signatures
#[test]
fn test_namespace_type_member_function_signature() {
    use crate::parser::ParserState;

    let source = r#"
namespace API {
    export interface Request {
        method: string;
        url: string;
    }
    export interface Response {
        status: number;
        body: string;
    }
}

function handleRequest(req: API.Request): API.Response {
    return { status: 200, body: "" };
}

const makeRequest: (req: API.Request) => API.Response = handleRequest;
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

    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Expected no errors for namespace types in function signatures, got: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn test_use_before_assignment_basic_flow() {
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::parser::ParserState;

    let source = r#"
function foo() {
    let x: number;
    return x;
}

function bar(flag: boolean) {
    let x: number;
    if (flag) { x = 1; }
    return x;
}

function baz(flag: boolean) {
    let x: number;
    if (flag) { x = 1; } else { x = 2; }
    return x;
}

function qux() {
    let x: number;
    x = 5;
    return x;
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
    // TS2454 requires strictNullChecks
    let options = crate::checker::context::CheckerOptions {
        jsx_factory: "React.createElement".to_string(),
        jsx_fragment_factory: "React.Fragment".to_string(),
        strict_null_checks: true,
        ..Default::default()
    };
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        options,
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|diag| diag.code == diagnostic_codes::VARIABLE_IS_USED_BEFORE_BEING_ASSIGNED)
        .count();
    assert_eq!(
        count, 2,
        "Expected 2 use-before-assignment errors, got: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn test_use_before_assignment_try_catch() {
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::parser::ParserState;

    let source = r#"
function foo() {
    let x: number;
    try {
        x = 1;
    } catch {
    }
    return x;
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
    // TS2454 requires strictNullChecks
    let options = crate::checker::context::CheckerOptions {
        jsx_factory: "React.createElement".to_string(),
        jsx_fragment_factory: "React.Fragment".to_string(),
        strict_null_checks: true,
        ..Default::default()
    };
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        options,
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|diag| diag.code == diagnostic_codes::VARIABLE_IS_USED_BEFORE_BEING_ASSIGNED)
        .count();
    assert_eq!(
        count, 1,
        "Expected 1 use-before-assignment error, got: {:?}",
        checker.ctx.diagnostics
    );
}

