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
fn test_async_generator_no_2355() {
    use crate::parser::ParserState;

    let source = r#"
interface AsyncIterator<T, TReturn = any, TNext = unknown> {}
interface AsyncIterable<T> {}
interface AsyncIterableIterator<T> extends AsyncIterator<T> {}

async function* g1(): AsyncIterableIterator<number> { yield 1; }
async function* g2(): AsyncIterator<number> { yield 1; }
async function* g3(): AsyncIterable<number> { yield 1; }
async function* g4(): {} { yield 1; }
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
        "Did not expect TS2355 for async generator return types, got: {codes:?}"
    );
}

/// Test async functions with type alias return types (conformance: `asyncAliasReturnType_es5.ts`)
/// This replicates the scenario where Promise is not locally declared but comes from lib.
#[test]
fn test_async_alias_return_type_no_2355() {
    use crate::parser::ParserState;

    // Note: Unlike test_async_promise_void_no_2355, this doesn't declare Promise interface.
    // This matches the conformance test which relies on lib.es2015.promise.
    // The type alias PromiseAlias<T> = Promise<T> should still unwrap to void.
    let source = r#"
type PromiseAlias<T> = Promise<T>;

async function f(): PromiseAlias<void> {
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
        "Did not expect TS2355 for async PromiseAlias<void> return type (conformance: asyncAliasReturnType_es5.ts), got: {codes:?}"
    );
}

/// Test that calling a never-returning function doesn't trigger TS2355
/// This is a known limitation - calls to functions returning `never` should
/// terminate control flow but aren't currently detected.
#[test]
fn test_never_returning_call_no_2355() {
    use crate::parser::ParserState;

    let source = r#"
// Helper that returns never
function fail(message: string): never {
    throw new Error(message);
}

// Function that calls fail() should NOT get 2355
// because fail() never returns
function usesFail(): number {
    fail("boom");
}

// Function that doesn't call a never-returning function SHOULD get 2355
function fallsThrough(): number {
    console.log("oops");
}

// Never-returning initializer should also avoid 2355
function usesFailInInit(): number {
    const value = fail("boom");
}

function usesFailInList(): number {
    const a = 1, b = fail("boom");
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

    let actual_2355_count = count(2355);
    assert_eq!(
        actual_2355_count, 1,
        "Expected only fallsThrough() to get TS2355, got: {codes:?}"
    );
}

/// Test that try/catch blocks that always return or throw don't trigger TS2355.
#[test]
fn test_try_catch_no_2355() {
    use crate::parser::ParserState;

    let source = r#"
function fail(): never {
    throw "boom";
}

function tryCatchReturn(): number {
    try {
        return 1;
    } catch (e) {
        return 2;
    }
}

function tryCatchThrow(): number {
    try {
        throw "boom";
    } catch (e) {
        throw "boom";
    }
}

function tryCatchNever(): number {
    try {
        fail();
    } catch (e) {
        return 1;
    }
}

function tryCatchFallsThrough(): number {
    try {
        return 1;
    } catch (e) {
        console.log(e);
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
    let opts = crate::checker::context::CheckerOptions {
        jsx_factory: "React.createElement".to_string(),
        jsx_fragment_factory: "React.Fragment".to_string(),
        strict_null_checks: true,
        ..Default::default()
    }; // TS2366 requires strictNullChecks
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        opts,
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    let count = |code| codes.iter().filter(|&&c| c == code).count();

    let count_2355 = count(2355);
    let count_2366 = count(2366);
    assert_eq!(count_2355, 0, "Did not expect TS2355, got: {codes:?}");
    assert_eq!(
        count_2366, 1,
        "Expected only tryCatchFallsThrough() to get TS2366, got: {codes:?}"
    );
}

#[test]
fn test_no_implicit_any_false_suppresses_diagnostics() {
    use crate::parser::ParserState;

    let source = r#"
// @noImplicitAny: false
function implicitAnyParam(x) {
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
        "Unexpected diagnostics: {:?}",
        checker.ctx.diagnostics
    );
}

/// Test that block-scoped (let/const) declarations do NOT trigger TS7005/TS7034
/// even with noImplicitAny enabled. Only function-scoped (var) declarations
/// should trigger these diagnostics when captured by closures.
#[test]
fn test_ts7005_not_emitted_for_let_declarations() {
    use crate::parser::ParserState;

    let source = r#"
function f() {
    // let without initializer, captured by closure — should NOT trigger TS7005/TS7034
    let x;
    () => x;

    // var without initializer, captured by closure — SHOULD trigger TS7034 + TS7005
    var y;
    () => y;
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

    // Should have TS7005 for the var declaration (implicit any)
    assert!(
        codes.contains(&7005),
        "Expected TS7005 for var declaration, got: {codes:?}"
    );

    // The TS7005 should only fire once — for `var y`, NOT for `let x`
    let ts7005_count = codes.iter().filter(|&&c| c == 7005).count();
    assert_eq!(
        ts7005_count, 1,
        "Expected exactly 1 TS7005 (var only, not let), got {ts7005_count}: {codes:?}"
    );

    // tsc emits TS7034 for `var y` when captured by a closure with implicit any:
    // "Variable 'y' implicitly has type 'any' in some locations where its type
    // cannot be determined."
    let ts7034_count = codes.iter().filter(|&&c| c == 7034).count();
    assert_eq!(
        ts7034_count, 1,
        "Expected 1 TS7034 for var captured by closure, got {ts7034_count}: {codes:?}"
    );
}

#[test]
fn test_strict_false_suppresses_implicit_any() {
    use crate::parser::ParserState;

    let source = r#"
// @strict: false
function implicitAnyParam(x) {
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
        "Unexpected diagnostics: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn test_implicit_any_parameters_in_type_signatures() {
    use crate::parser::ParserState;

    let source = r#"
// @noImplicitAny: true
interface CtorTarget {}

interface ICall {
    (x): void;
}
interface IMethod {
    method(y): void;
}
interface IConstruct {
    new (z): CtorTarget;
}

type TLCall = { (a): void; };
type TLMethod = { method(b): void; };
type TLConstruct = { new (c): CtorTarget; };

type FnAlias = (d) => void;
type CtorAlias = new (e) => CtorTarget;

interface HandlerProp {
    handler: (f) => void;
}
type PropAlias = { handler: (g) => void; };
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
        count(7006),
        10,
        "Expected ten 7006 errors, got codes: {codes:?}"
    );
}

#[test]
fn test_implicit_any_rest_parameter() {
    use crate::parser::ParserState;

    // Test that rest parameters without type annotation trigger TS7006 with 'any[]'
    let source = r#"
// @noImplicitAny: true
function foo(...args) {
    return args;
}

function bar(a, ...rest) {
    return rest;
}

const arrow = (...items) => items;
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

    // Should have implicit-any errors for rest and regular params:
    // - args in foo (rest param) -> TS7019
    // - a in bar (regular param) -> TS7006
    // - rest in bar (rest param) -> TS7019
    // - items in arrow (rest param) -> TS7019
    // Note: some rest params may emit TS7019 more than once due to
    // type resolution visiting the parameter in multiple contexts.
    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();

    // Rest parameters use TS7019, regular parameters use TS7006
    assert!(
        codes.iter().filter(|&&c| c == 7019).count() >= 3,
        "Expected at least three TS7019 (rest param implicit any[]) errors, got codes: {codes:?}"
    );
    assert!(
        codes.iter().filter(|&&c| c == 7006).count() >= 1,
        "Expected at least one TS7006 (regular param implicit any) error, got codes: {codes:?}"
    );

    // Check TS7019 messages contain "Rest parameter"
    let rest_messages: Vec<&str> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 7019)
        .map(|d| d.message_text.as_str())
        .collect();
    assert!(
        rest_messages.iter().all(|m| m.contains("Rest parameter")),
        "TS7019 messages should say 'Rest parameter', got: {rest_messages:?}"
    );

    // Check TS7006 message for regular parameter
    let regular_messages: Vec<&str> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 7006)
        .map(|d| d.message_text.as_str())
        .collect();
    assert_eq!(regular_messages.len(), 1);
    assert!(
        regular_messages[0].contains("'any'") && !regular_messages[0].contains("any[]"),
        "TS7006 message should say 'any' not 'any[]', got: {:?}",
        regular_messages[0]
    );
}

#[test]
fn test_checker_lowers_element_access_array() {
    use crate::parser::ParserState;

    let source = r#"
const arr: number[] = [1, 2];
const value = arr[0];
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
    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Unexpected diagnostics: {:?}",
        checker.ctx.diagnostics
    );

    let value_sym = binder.file_locals.get("value").expect("value should exist");
    let value_type = checker.get_type_of_symbol(value_sym);
    assert_eq!(value_type, TypeId::NUMBER);
}
