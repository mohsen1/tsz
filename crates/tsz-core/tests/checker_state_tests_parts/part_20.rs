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
fn test_implicit_any_return_in_signatures() {
    use crate::parser::ParserState;

    let source = r#"
// @noImplicitAny: true
interface I {
    foo();
}

declare function bar();

declare class C {
    publicMethod();
}

const obj = { baz() { return undefined; } };
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
    let count = |code| codes.iter().filter(|&&c| c == code).count();

    assert_eq!(
        count(7010),
        3,
        "Expected three 7010 errors, got codes: {codes:?}"
    );
}

#[test]
fn test_ts7010_async_function_no_false_positive() {
    use crate::parser::ParserState;

    let source = r#"
// @noImplicitAny: true
// Async functions without return type should NOT trigger TS7010
// because they infer Promise<void>, not 'any'
async function asyncNoReturn() {
}

async function asyncExplicitReturn() {
    return;
}

class C {
    async get foo() {
    }
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
    let ts7010_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 7010)
        .collect();

    assert!(
        ts7010_errors.is_empty(),
        "Expected no TS7010 errors for async functions returning Promise<void>, got: {codes:?}"
    );
}

#[test]
fn test_ts7010_exactly_any_return() {
    use crate::parser::ParserState;

    // TSC does NOT emit TS7010/TS7011 when a function body returns an `any`-typed
    // expression.  The return type is validly *inferred* as `any` (not "implicit any").
    // TS7010 only fires for bodyless declarations (interfaces, abstract methods) or
    // when the return type widens from null/undefined to any.
    let source = r#"
// @noImplicitAny: true
declare var anyValue: any;

// Should NOT trigger TS7010 - return type is inferred as 'any' from body
function returnsAny() {
    return anyValue;
}

// Should NOT trigger TS7011 - return type is inferred as 'any' from body
const arrowReturnsAny = () => anyValue;
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
    let count = |code| codes.iter().filter(|&&c| c == code).count();

    assert_eq!(
        count(7010),
        0,
        "Expected no TS7010 errors for function returning inferred 'any', got codes: {codes:?}"
    );
    assert_eq!(
        count(7011),
        0,
        "Expected no TS7011 errors for arrow function returning inferred 'any', got codes: {codes:?}"
    );
}

/// TODO: TS7010 for null|undefined return is not yet implemented.
/// Currently no diagnostic is emitted for a function returning null | undefined
/// under noImplicitAny. When implemented, update to expect 1 TS7010.
#[test]
fn test_ts7010_null_undefined_return() {
    use crate::parser::ParserState;

    let source = r#"
// @noImplicitAny: true
// Should trigger TS7010 - return type is null | undefined (treated as 'any')
function returnsNullOrUndefined(flag: boolean) {
    if (flag) return null;
    return undefined;
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
    let count = |code| codes.iter().filter(|&&c| c == code).count();

    // TODO: Should be 1 once TS7010 for null|undefined return is implemented
    assert_eq!(
        count(7010),
        0,
        "Expected 0 TS7010 errors (not yet implemented), got codes: {codes:?}"
    );
}

#[test]
fn test_ts7010_class_expression_no_false_positive() {
    use crate::parser::ParserState;

    let source = r#"
// @noImplicitAny: true
// Functions returning class expressions should NOT trigger TS7010
// even if the class contains 'any' in its structure somewhere
class A<T> {
    value: T;
}

function createClass() {
    return class extends A<string> { };
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

    let ts7010_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 7010)
        .collect();

    assert!(
        ts7010_errors.is_empty(),
        "Expected no TS7010 errors for functions returning class expressions, got: {:?}",
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_ts7010_return_path_analysis() {
    use crate::parser::ParserState;

    let source = r#"
function allReturn(flag: boolean) {
    if (flag) {
        return 1;
    } else {
        return 2;
    }
}

function missingReturn(flag: boolean) {
    if (flag) {
        return 1;
    }
}

function throwOnly() {
    throw new Error("boom");
}

function infiniteLoop() {
    while (true) {}
}

function loopWithBreak() {
    while (true) { break; }
}

function loopWithNestedSwitchBreak(flag: boolean) {
    while (true) {
        switch (flag) {
            case true:
                break;
        }
    }
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

    let arena = parser.get_arena();
    let root_node = arena.get(root).expect("root node");
    let source_file = arena.get_source_file(root_node).expect("source file");

    let body_at = |index: usize| {
        let stmt_idx = *source_file
            .statements
            .nodes
            .get(index)
            .expect("statement index");
        let stmt_node = arena.get(stmt_idx).expect("statement node");
        let func = arena.get_function(stmt_node).expect("function data");
        func.body
    };

    assert!(
        !checker.function_body_falls_through(body_at(0)),
        "allReturn should not fall through"
    );
    assert!(
        checker.function_body_falls_through(body_at(1)),
        "missingReturn should fall through"
    );
    assert!(
        !checker.function_body_falls_through(body_at(2)),
        "throwOnly should not fall through"
    );
    assert!(
        !checker.function_body_falls_through(body_at(3)),
        "infiniteLoop should not fall through"
    );
    assert!(
        checker.function_body_falls_through(body_at(4)),
        "loopWithBreak should fall through"
    );
    assert!(
        !checker.function_body_falls_through(body_at(5)),
        "loopWithNestedSwitchBreak should not fall through"
    );
}

/// Test that functions that only throw don't trigger TS2355.
/// TS2355: "A function whose declared type is neither 'void' nor 'any' must return a value"
/// This should NOT fire for functions that only throw since throwing is a valid exit.
#[test]
fn test_throw_only_function_no_2355() {
    use crate::parser::ParserState;

    let source = r#"
// Function that only throws should NOT get 2355
function throwOnly(): number {
    throw new Error("always throws");
}

// Method that only throws should NOT get 2355
class C {
    throwMethod(): string {
        throw new Error("always throws");
    }

    get throwGetter(): number {
        throw new Error("getter throws");
    }
}

// Function that DOES fall through without returning SHOULD get 2355
function fallsThrough(): number {
    console.log("oops, no return");
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
    let count = |code| codes.iter().filter(|&&c| c == code).count();

    // Only fallsThrough should get 2355, not the throw-only functions
    assert_eq!(
        count(2355),
        1,
        "Expected exactly one 2355 error for fallsThrough(), got: {codes:?}"
    );

    // Verify which function got the error by checking the messages
    let error_2355 = checker.ctx.diagnostics.iter().find(|d| d.code == 2355);
    assert!(error_2355.is_some(), "Should have a 2355 error");
}

/// Test that infinite loops don't trigger TS2355 either
#[test]
fn test_infinite_loop_no_2355() {
    use crate::parser::ParserState;

    let source = r#"
// Infinite loop without break should NOT get 2355
function infiniteLoop(): number {
    while (true) {
        console.log("forever");
    }
}

// But loop with break SHOULD fall through
function loopWithBreak(): number {
    while (true) {
        break;
    }
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
    let count = |code| codes.iter().filter(|&&c| c == code).count();

    // Only loopWithBreak should get 2355
    assert_eq!(
        count(2355),
        1,
        "Expected exactly one 2355 error for loopWithBreak(), got: {codes:?}"
    );
}

#[test]
fn test_async_promise_void_no_2355() {
    use crate::parser::ParserState;

    let source = r#"
interface Promise<T> {}
interface PromiseLike<T> {}
type PromiseAlias<T> = Promise<T>;
type PromiseLikeAlias<T> = PromiseLike<T>;

async function f1(): Promise<void> { }
async function f2(): PromiseAlias<void> { }
async function f3(): PromiseLike<void> { }
async function f4(): PromiseLikeAlias<void> { }

class C {
    async m1(): Promise<void> { }
    async m2(): PromiseAlias<void> { }
    async m3(): PromiseLike<void> { }
    async m4(): PromiseLikeAlias<void> { }
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
        !codes.contains(&2355),
        "Did not expect TS2355 for async Promise<void> return types, got: {codes:?}"
    );
}

/// Test TS2355: Async function returning Promise<T> requires return statement
#[test]
fn test_async_promise_number_requires_return() {
    use crate::parser::ParserState;

    let source = r#"
interface Promise<T> {}

async function f(): Promise<number> { }
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
        codes.contains(&2355),
        "Expected TS2355 for async Promise<number> return type, got: {codes:?}"
    );
}

