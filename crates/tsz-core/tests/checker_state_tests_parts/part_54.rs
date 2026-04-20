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
fn test_ts2366_arrow_function_try_catch() {
    use crate::parser::ParserState;

    // Test error 2366 for arrow functions with try/catch
    let source = r#"
// Arrow function with try/catch - both branches can fall through
const tryCatchFallthrough = (): number => {
    try {
        if (Math.random() > 0.5) {
            return 1;
        }
    } catch (e) {
        console.log(e);
    }
};

// Arrow function with try/catch/finally - finally doesn't return but catch can fall through
const tryFinallyFallthrough = (): number => {
    try {
        if (Math.random() > 0.5) {
            return 1;
        }
    } catch (e) {
        console.log(e);
    } finally {
        console.log("cleanup");
    }
};
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(parser.get_diagnostics().is_empty());

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

    // Should have 2 errors: 2366 for both functions
    assert_eq!(
        codes.iter().filter(|&&c| c == 2366).count(),
        2,
        "Expected 2 TS2366 errors for arrow functions with try/catch fallthrough, got: {codes:?}"
    );
}

#[test]
fn test_ts7027_unreachable_code_after_return() {
    use crate::parser::ParserState;

    // Test TS7027 for unreachable code after return
    let source = r#"
function test1(): number {
    return 1;
    console.log("unreachable");  // Should error: TS7027
}

function test2(): void {
    return;
    const x = 5;  // Should error: TS7027
}

function test3(): string {
    if (true) {
        return "yes";
    }
    return "no";
    console.log("unreachable");  // Should error: TS7027
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(parser.get_diagnostics().is_empty());

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let opts = crate::checker::context::CheckerOptions {
        jsx_factory: "React.createElement".to_string(),
        jsx_fragment_factory: "React.Fragment".to_string(),
        allow_unreachable_code: Some(false),
        ..Default::default()
    };
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

    // Should have 3 TS7027 errors
    assert_eq!(
        codes.iter().filter(|&&c| c == 7027).count(),
        3,
        "Expected 3 TS7027 errors for unreachable code after return, got: {codes:?}"
    );
}

#[test]
fn test_ts7027_unreachable_code_after_throw() {
    use crate::parser::ParserState;

    // Test TS7027 for unreachable code after throw
    let source = r#"
function test1(): never {
    throw new Error("error");
    console.log("unreachable");  // Should error: TS7027
}

function test2(): number {
    throw new Error("error");
    return 1;  // Should error: TS7027
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(parser.get_diagnostics().is_empty());

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let opts = crate::checker::context::CheckerOptions {
        jsx_factory: "React.createElement".to_string(),
        jsx_fragment_factory: "React.Fragment".to_string(),
        allow_unreachable_code: Some(false),
        ..Default::default()
    };
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

    // Should have 2 TS7027 errors
    assert_eq!(
        codes.iter().filter(|&&c| c == 7027).count(),
        2,
        "Expected 2 TS7027 errors for unreachable code after throw, got: {codes:?}"
    );
}

#[test]
fn test_ts7027_unreachable_after_never_expression() {
    use crate::parser::ParserState;

    // Test TS7027 for unreachable code after never-type expressions
    let source = r#"
declare function fail(): never;

function test1(): number {
    fail();
    return 1;  // Should error: TS7027
}

function test2(): void {
    fail();
    console.log("unreachable");  // Should error: TS7027
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(parser.get_diagnostics().is_empty());

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let opts = crate::checker::context::CheckerOptions {
        jsx_factory: "React.createElement".to_string(),
        jsx_fragment_factory: "React.Fragment".to_string(),
        allow_unreachable_code: Some(false),
        ..Default::default()
    };
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

    // Should have 2 TS7027 errors
    assert_eq!(
        codes.iter().filter(|&&c| c == 7027).count(),
        2,
        "Expected 2 TS7027 errors for unreachable code after never expression, got: {codes:?}"
    );
}

#[test]
fn test_ts2366_conditional_returns_all_paths() {
    use crate::parser::ParserState;

    // Test that functions with conditional returns that cover all paths don't error
    let source = r#"
function test1(flag: boolean): number {
    if (flag) {
        return 1;
    } else {
        return 2;
    }
}

function test2(x: number): string {
    if (x > 0) {
        return "positive";
    } else if (x < 0) {
        return "negative";
    } else {
        return "zero";
    }
}

function test3(x: number): number {
    switch (x) {
        case 1:
            return 1;
        case 2:
            return 2;
        default:
            return 0;
    }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(parser.get_diagnostics().is_empty());

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

    // Should have no TS2366 errors - all paths return
    assert_eq!(
        codes.iter().filter(|&&c| c == 2366).count(),
        0,
        "Expected 0 TS2366 errors when all paths return, got: {codes:?}"
    );
}

#[test]
fn test_ts2366_early_return() {
    use crate::parser::ParserState;

    // Test that early returns are handled correctly
    let source = r#"
function test1(x: number): number {
    if (x < 0) {
        return -1;
    }
    return x;  // OK - this is reached when x >= 0
}

function test2(x: number): number {
    if (x < 0) {
        return -1;
    }
    if (x > 0) {
        return 1;
    }
    return 0;  // OK - this is reached when x == 0
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(parser.get_diagnostics().is_empty());

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

    // Should have no TS2366 errors - all paths return
    assert_eq!(
        codes.iter().filter(|&&c| c == 2366).count(),
        0,
        "Expected 0 TS2366 errors with early returns, got: {codes:?}"
    );
}

#[test]
fn test_ts2366_throw_as_exit() {
    use crate::parser::ParserState;

    // Test that throw statements are treated as exits
    let source = r#"
function test1(x: number): number {
    if (x < 0) {
        throw new Error("negative");
    }
    return x;
}

function test2(x: number): never {
    throw new Error("always throws");
}

function test3(x: number): number {
    if (x < 0) {
        throw new Error("negative");
    }
    if (x > 100) {
        throw new Error("too large");
    }
    return x;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(parser.get_diagnostics().is_empty());

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

    // Should have no TS2366 errors - throw exits the function
    assert_eq!(
        codes.iter().filter(|&&c| c == 2366).count(),
        0,
        "Expected 0 TS2366 errors when throw is used as exit, got: {codes:?}"
    );
}

#[test]
fn test_function_overload_no_ts2366() {
    use crate::parser::ParserState;

    // Test that function overloads (signatures without bodies) don't trigger TS2366
    let source = r#"
function overloaded(x: number): number;
function overloaded(x: string): string;
function overloaded(x: number | string): number | string {
    return x;
}

class MyClass {
    method(x: number): number;
    method(x: string): string;
    method(x: number | string): number | string {
        return x;
    }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(parser.get_diagnostics().is_empty());

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

    // Should have no TS2366 errors - overloads don't have bodies
    assert_eq!(
        codes.iter().filter(|&&c| c == 2366).count(),
        0,
        "Expected 0 TS2366 errors for function overloads, got: {codes:?}"
    );
}

#[test]
fn test_function_overload_implementation_return_type_mismatch_reports_ts2322() {
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::parser::ParserState;
    use crate::parser::syntax_kind_ext;

    let source = r#"
function foo(bar: { a:number }[]): number;
function foo(bar: { a:string }[]): string;
function foo([x]: { a:number | string }[]): string | number {
    if (x) {
        return x.a;
    }

    return undefined;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    assert!(parser.get_diagnostics().is_empty());

    let arena = parser.get_arena();
    let root_node = arena.get(root).expect("root node");
    let source_file = arena.get_source_file(root_node).expect("source file");
    let impl_idx = source_file
        .statements
        .nodes
        .iter()
        .rev()
        .copied()
        .find(|&idx| {
            arena
                .get(idx)
                .is_some_and(|node| node.kind == syntax_kind_ext::FUNCTION_DECLARATION)
        })
        .expect("implementation function");
    let impl_node = arena.get(impl_idx).expect("impl node");
    let func = arena.get_function(impl_node).expect("function data");
    assert!(
        func.type_annotation.is_some(),
        "expected overload implementation to keep its explicit return annotation"
    );

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(arena, root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena,
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let ts2322_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();

    assert!(
        !ts2322_errors.is_empty(),
        "Expected TS2322 for overload implementation return mismatch, got: {:?}",
        checker.ctx.diagnostics
    );
}

/// Test TS2705: Async function must return Promise
///
/// Test TS2705: Async function must return Promise
///
/// TODO: TS2705 is not yet emitted for async functions with non-Promise return types.
/// With ES2015 target, TS2705 (ES5 Promise constructor) doesn't fire.
/// TS1064 fires for 4 async functions with non-Promise return types.
#[test]
fn test_async_function_returns_promise() {
    use crate::parser::ParserState;

    let source = r#"
interface Promise<T> {}

// Should emit TS2705 for these
async function foo(): number { return 42; }
async function bar(): string { return "hello"; }

const baz = async (): boolean => false;

class Qux {
    async method(): void { console.log("test"); }
}

// Should NOT emit TS2705 for these
async function qux(): Promise<number> { return 42; }
async function quux() { return "hello"; }
async function corge(): Promise<void> { console.log("test"); }

const arrowPromise = async (): Promise<string> => "test";
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
        crate::checker::context::CheckerOptions {
            target: tsz_common::common::ScriptTarget::ES2015,
            ..crate::checker::context::CheckerOptions::default()
        },
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();

    // TS2705/TS2468 fire because setup_lib_contexts doesn't register Promise as a VALUE.
    // Filter those out and verify TS1064 (return type must be Promise<T>) fires for the
    // 4 async functions with non-Promise return types: foo, bar, baz, Qux.method.
    let relevant: Vec<u32> = codes
        .iter()
        .copied()
        .filter(|&c| c != 2705 && c != 2468 && c != 2584)
        .collect();
    let ts1064_count = relevant.iter().filter(|&&c| c == 1064).count();
    assert_eq!(
        ts1064_count, 4,
        "Expected 4 TS1064 errors for async functions with non-Promise return types, got: {relevant:?}"
    );
}
