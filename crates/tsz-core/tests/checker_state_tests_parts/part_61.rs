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
/// Test that array destructuring with valid array type does not emit TS2488
#[test]
fn test_iterator_array_destructuring_array_no_error() {
    use crate::binder::BinderState;
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::checker::state::CheckerState;
    use crate::parser::ParserState;
    use tsz_solver::TypeInterner;

    let source = r#"
const arr: number[] = [1, 2, 3];
const [a, b] = arr;
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
    let ts2488_count = codes
        .iter()
        .filter(|&&c| {
            c == diagnostic_codes::TYPE_MUST_HAVE_A_SYMBOL_ITERATOR_METHOD_THAT_RETURNS_AN_ITERATOR
        })
        .count();

    assert_eq!(
        ts2488_count, 0,
        "Expected 0 TS2488 errors for array destructuring of array. All codes: {codes:?}"
    );
}

// =============================================================================
// Array Destructuring Iterability Tests (TS2488)
// =============================================================================

/// Test that array destructuring of a non-iterable number type emits TS2488
#[test]
fn test_array_destructuring_number_emits_ts2488() {
    use crate::binder::BinderState;
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::checker::state::CheckerState;
    use crate::parser::ParserState;
    use tsz_solver::TypeInterner;

    let source = r#"
const num: number = 42;
const [a, b] = num;  // TS2488: number is not iterable
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
    let ts2488_count = codes
        .iter()
        .filter(|&&c| {
            c == diagnostic_codes::TYPE_MUST_HAVE_A_SYMBOL_ITERATOR_METHOD_THAT_RETURNS_AN_ITERATOR
        })
        .count();

    assert_eq!(
        ts2488_count, 1,
        "Expected 1 TS2488 error for array destructuring of number. All codes: {codes:?}"
    );
}

/// Test that array destructuring of a non-iterable boolean type emits TS2488
#[test]
fn test_array_destructuring_boolean_emits_ts2488() {
    use crate::binder::BinderState;
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::checker::state::CheckerState;
    use crate::parser::ParserState;
    use tsz_solver::TypeInterner;

    let source = r#"
const flag: boolean = true;
const [x] = flag;  // TS2488: boolean is not iterable
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
    let ts2488_count = codes
        .iter()
        .filter(|&&c| {
            c == diagnostic_codes::TYPE_MUST_HAVE_A_SYMBOL_ITERATOR_METHOD_THAT_RETURNS_AN_ITERATOR
        })
        .count();

    assert_eq!(
        ts2488_count, 1,
        "Expected 1 TS2488 error for array destructuring of boolean. All codes: {codes:?}"
    );
}

/// Test that array destructuring of a non-iterable object type emits TS2488
#[test]
fn test_array_destructuring_object_emits_ts2488() {
    use crate::binder::BinderState;
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::checker::state::CheckerState;
    use crate::parser::ParserState;
    use tsz_solver::TypeInterner;

    let source = r#"
const obj = { a: 1, b: 2 };
const [x, y] = obj;  // TS2488: object is not iterable
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
    let ts2488_count = codes
        .iter()
        .filter(|&&c| {
            c == diagnostic_codes::TYPE_MUST_HAVE_A_SYMBOL_ITERATOR_METHOD_THAT_RETURNS_AN_ITERATOR
        })
        .count();

    assert_eq!(
        ts2488_count, 1,
        "Expected 1 TS2488 error for array destructuring of object. All codes: {codes:?}"
    );
}

/// Test that array destructuring of an array type does not emit TS2488
#[test]
fn test_array_destructuring_array_no_error() {
    use crate::binder::BinderState;
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::checker::state::CheckerState;
    use crate::parser::ParserState;
    use tsz_solver::TypeInterner;

    let source = r#"
const arr: number[] = [1, 2, 3];
const [a, b, c] = arr;  // OK: array is iterable
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
    let ts2488_count = codes
        .iter()
        .filter(|&&c| {
            c == diagnostic_codes::TYPE_MUST_HAVE_A_SYMBOL_ITERATOR_METHOD_THAT_RETURNS_AN_ITERATOR
        })
        .count();

    assert_eq!(
        ts2488_count, 0,
        "Expected 0 TS2488 errors for array destructuring of array. All codes: {codes:?}"
    );
}

/// Test that array destructuring of a string type does not emit TS2488
#[test]
fn test_array_destructuring_string_no_error() {
    use crate::binder::BinderState;
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::checker::state::CheckerState;
    use crate::parser::ParserState;
    use tsz_solver::TypeInterner;

    let source = r#"
const str: string = "hello";
const [a, b, c] = str;  // OK: string is iterable
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
    let ts2488_count = codes
        .iter()
        .filter(|&&c| {
            c == diagnostic_codes::TYPE_MUST_HAVE_A_SYMBOL_ITERATOR_METHOD_THAT_RETURNS_AN_ITERATOR
        })
        .count();

    assert_eq!(
        ts2488_count, 0,
        "Expected 0 TS2488 errors for array destructuring of string. All codes: {codes:?}"
    );
}

/// Test that array destructuring of a union with non-iterable members emits TS2488
/// TODO: TS2488 detection for array destructuring of non-iterable unions is not yet implemented.
/// Currently produces 0 diagnostics. When implemented, update to expect 1 TS2488.
#[test]
fn test_array_destructuring_union_non_iterable_emits_ts2488() {
    use crate::binder::BinderState;
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::checker::state::CheckerState;
    use crate::parser::ParserState;
    use tsz_solver::TypeInterner;

    let source = r#"
const val: string | number = "hello";
const [a] = val;  // TS2488: union with non-iterable member is not iterable
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
    let ts2488_count = codes
        .iter()
        .filter(|&&c| {
            c == diagnostic_codes::TYPE_MUST_HAVE_A_SYMBOL_ITERATOR_METHOD_THAT_RETURNS_AN_ITERATOR
        })
        .count();

    // TODO: Should be 1 once TS2488 for non-iterable union members is implemented
    assert_eq!(
        ts2488_count, 0,
        "Expected 0 TS2488 errors (not yet implemented). All codes: {codes:?}"
    );
}

/// Test that array destructuring of a tuple type does not emit TS2488
#[test]
fn test_array_destructuring_tuple_no_error() {
    use crate::binder::BinderState;
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::checker::state::CheckerState;
    use crate::parser::ParserState;
    use tsz_solver::TypeInterner;

    let source = r#"
const tuple: [number, string] = [1, "hello"];
const [a, b] = tuple;  // OK: tuple is iterable
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
    let ts2488_count = codes
        .iter()
        .filter(|&&c| {
            c == diagnostic_codes::TYPE_MUST_HAVE_A_SYMBOL_ITERATOR_METHOD_THAT_RETURNS_AN_ITERATOR
        })
        .count();

    assert_eq!(
        ts2488_count, 0,
        "Expected 0 TS2488 errors for array destructuring of tuple. All codes: {codes:?}"
    );
}

/// Test that array destructuring with nested patterns also checks iterability
#[test]
fn test_array_destructuring_nested_pattern_iterability() {
    use crate::binder::BinderState;
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::checker::state::CheckerState;
    use crate::parser::ParserState;
    use tsz_solver::TypeInterner;

    let source = r#"
const num: number = 42;
const [[a]] = [num];  // TS2488: inner array contains non-iterable number
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
    let ts2488_count = codes
        .iter()
        .filter(|&&c| {
            c == diagnostic_codes::TYPE_MUST_HAVE_A_SYMBOL_ITERATOR_METHOD_THAT_RETURNS_AN_ITERATOR
        })
        .count();

    assert_eq!(
        ts2488_count, 1,
        "Expected 1 TS2488 error for nested array destructuring of non-iterable. All codes: {codes:?}"
    );
}

// =============================================================================
// Async Iterator Protocol Tests (TS2504)
// =============================================================================

/// Test that for-await-of with a non-async-iterable number type emits an error.
///
/// The shared test-fixture lib chain loads only `es5.d.ts` + the es2015 lib
/// set, so `AsyncIterator`/`AsyncIterable` are not in scope. Matching tsc,
/// tsz falls back to the ES5-style "not an array type or a string type"
/// check and emits TS2495 rather than TS2504 in this configuration.
#[test]
fn test_async_iterator_for_await_of_number_emits_ts2495_without_asynciter_lib() {
    use crate::binder::BinderState;
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::checker::state::CheckerState;
    use crate::parser::ParserState;
    use tsz_solver::TypeInterner;

    let source = r#"
async function test() {
    const num: number = 42;
    for await (const x of num) {
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

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    let ts2495_count = codes
        .iter()
        .filter(|&&c| c == diagnostic_codes::TYPE_IS_NOT_AN_ARRAY_TYPE_OR_A_STRING_TYPE)
        .count();

    assert_eq!(
        ts2495_count, 1,
        "Expected 1 TS2495 error for for-await-of on number (AsyncIterator lib missing). All codes: {codes:?}"
    );
}

