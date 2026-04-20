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
fn test_ts2362_ts2363_all_arithmetic_operators() {
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::parser::ParserState;

    let source = r#"
const str = "hello";
const num = 10;
const r1 = str - num;  // TS2362
const r2 = str * num;  // TS2362
const r3 = str / num;  // TS2362
const r4 = str % num;  // TS2362
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
    let ts2362_count = codes
        .iter()
        .filter(|&&c| c == diagnostic_codes::THE_LEFT_HAND_SIDE_OF_AN_ARITHMETIC_OPERATION_MUST_BE_OF_TYPE_ANY_NUMBER_BIGINT)
        .count();

    assert_eq!(
        ts2362_count, 4,
        "Expected 4 TS2362 errors for all arithmetic operators. All codes: {codes:?}"
    );
}

// =============================================================================
// Iterator Protocol Tests (TS2488)
// =============================================================================

/// Test that for-of with a non-iterable number type emits TS2488
#[test]
fn test_iterator_for_of_number_emits_ts2488() {
    use crate::binder::BinderState;
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::checker::state::CheckerState;
    use crate::parser::ParserState;
    use tsz_solver::TypeInterner;

    let source = r#"
const num: number = 42;
for (const x of num) {
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
    let ts2488_count = codes
        .iter()
        .filter(|&&c| {
            c == diagnostic_codes::TYPE_MUST_HAVE_A_SYMBOL_ITERATOR_METHOD_THAT_RETURNS_AN_ITERATOR
        })
        .count();

    assert_eq!(
        ts2488_count, 1,
        "Expected 1 TS2488 error for for-of on number. All codes: {codes:?}"
    );
}

/// Test that for-of with a valid array type does not emit TS2488
#[test]
fn test_iterator_for_of_array_no_error() {
    use crate::binder::BinderState;
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::checker::state::CheckerState;
    use crate::parser::ParserState;
    use tsz_solver::TypeInterner;

    let source = r#"
const arr: number[] = [1, 2, 3];
for (const x of arr) {
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
    let ts2488_count = codes
        .iter()
        .filter(|&&c| {
            c == diagnostic_codes::TYPE_MUST_HAVE_A_SYMBOL_ITERATOR_METHOD_THAT_RETURNS_AN_ITERATOR
        })
        .count();

    assert_eq!(
        ts2488_count, 0,
        "Expected 0 TS2488 errors for for-of on array. All codes: {codes:?}"
    );
}

/// Test that for-of with a string type does not emit TS2488
#[test]
fn test_iterator_for_of_string_no_error() {
    use crate::binder::BinderState;
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::checker::state::CheckerState;
    use crate::parser::ParserState;
    use tsz_solver::TypeInterner;

    let source = r#"
const str: string = "hello";
for (const ch of str) {
    console.log(ch);
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
    let ts2488_count = codes
        .iter()
        .filter(|&&c| {
            c == diagnostic_codes::TYPE_MUST_HAVE_A_SYMBOL_ITERATOR_METHOD_THAT_RETURNS_AN_ITERATOR
        })
        .count();

    assert_eq!(
        ts2488_count, 0,
        "Expected 0 TS2488 errors for for-of on string. All codes: {codes:?}"
    );
}

/// Test that spread of a non-iterable type emits TS2488
#[test]
fn test_iterator_spread_number_emits_ts2488() {
    use crate::binder::BinderState;
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::checker::state::CheckerState;
    use crate::parser::ParserState;
    use tsz_solver::TypeInterner;

    let source = r#"
const num: number = 42;
const arr = [...num];
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
        "Expected 1 TS2488 error for spread of number. All codes: {codes:?}"
    );
}

/// Test that spread of a valid array type does not emit TS2488
#[test]
fn test_iterator_spread_array_no_error() {
    use crate::binder::BinderState;
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::checker::state::CheckerState;
    use crate::parser::ParserState;
    use tsz_solver::TypeInterner;

    let source = r#"
const arr1: number[] = [1, 2, 3];
const arr2 = [...arr1];
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
        "Expected 0 TS2488 errors for spread of array. All codes: {codes:?}"
    );
}

/// Test that spread in function arguments with non-iterable emits TS2488
#[test]
fn test_iterator_spread_in_call_emits_ts2488() {
    use crate::binder::BinderState;
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::checker::state::CheckerState;
    use crate::parser::ParserState;
    use tsz_solver::TypeInterner;

    let source = r#"
function foo(a: number, b: number): void {}
const obj: { x: number } = { x: 1 };
foo(...obj);
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
        "Expected 1 TS2488 error for spread of object in call. All codes: {codes:?}"
    );
}

/// Test that for-of with boolean type emits TS2488
#[test]
fn test_iterator_for_of_boolean_emits_ts2488() {
    use crate::binder::BinderState;
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::checker::state::CheckerState;
    use crate::parser::ParserState;
    use tsz_solver::TypeInterner;

    let source = r#"
const b: boolean = true;
for (const x of b) {
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
    let ts2488_count = codes
        .iter()
        .filter(|&&c| {
            c == diagnostic_codes::TYPE_MUST_HAVE_A_SYMBOL_ITERATOR_METHOD_THAT_RETURNS_AN_ITERATOR
        })
        .count();

    assert_eq!(
        ts2488_count, 1,
        "Expected 1 TS2488 error for for-of on boolean. All codes: {codes:?}"
    );
}

/// Test that for-of with tuple type does not emit TS2488
#[test]
fn test_iterator_for_of_tuple_no_error() {
    use crate::binder::BinderState;
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::checker::state::CheckerState;
    use crate::parser::ParserState;
    use tsz_solver::TypeInterner;

    let source = r#"
const tuple: [number, string, boolean] = [1, "hello", true];
for (const x of tuple) {
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
    let ts2488_count = codes
        .iter()
        .filter(|&&c| {
            c == diagnostic_codes::TYPE_MUST_HAVE_A_SYMBOL_ITERATOR_METHOD_THAT_RETURNS_AN_ITERATOR
        })
        .count();

    assert_eq!(
        ts2488_count, 0,
        "Expected 0 TS2488 errors for for-of on tuple. All codes: {codes:?}"
    );
}

/// Test that array destructuring with non-iterable number type emits TS2488
#[test]
fn test_iterator_array_destructuring_number_emits_ts2488() {
    use crate::binder::BinderState;
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::checker::state::CheckerState;
    use crate::parser::ParserState;
    use tsz_solver::TypeInterner;

    let source = r#"
const num: number = 42;
const [a, b] = num;
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
