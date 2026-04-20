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
/// Test that for-await-of with a valid array type does not emit TS2504 (sync iterable is accepted)
#[test]
fn test_async_iterator_for_await_of_array_no_error() {
    use crate::binder::BinderState;
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::checker::state::CheckerState;
    use crate::parser::ParserState;
    use tsz_solver::TypeInterner;

    let source = r#"
async function test() {
    const arr: number[] = [1, 2, 3];
    for await (const x of arr) {
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
    let ts2504_count = codes
        .iter()
        .filter(|&&c| c == diagnostic_codes::TYPE_MUST_HAVE_A_SYMBOL_ASYNCITERATOR_METHOD_THAT_RETURNS_AN_ASYNC_ITERATOR)
        .count();

    assert_eq!(
        ts2504_count, 0,
        "Expected 0 TS2504 errors for for-await-of on array (sync iterable is accepted). All codes: {codes:?}"
    );
}

/// Test that for-await-of with a boolean type emits TS2504
#[test]
fn test_async_iterator_for_await_of_boolean_emits_ts2495_without_asynciter_lib() {
    use crate::binder::BinderState;
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::checker::state::CheckerState;
    use crate::parser::ParserState;
    use tsz_solver::TypeInterner;

    let source = r#"
async function test() {
    const b: boolean = true;
    for await (const x of b) {
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
        "Expected 1 TS2495 error for for-await-of on boolean (AsyncIterator lib missing). All codes: {codes:?}"
    );
}

/// Test that for-await-of with an object type (non-iterable) emits an error.
///
/// With only the es5/es2015 lib set loaded, `AsyncIterator`/`AsyncIterable`
/// aren't available, so tsc (and now tsz) emit TS2495 rather than TS2504.
#[test]
fn test_async_iterator_for_await_of_object_emits_ts2495_without_asynciter_lib() {
    use crate::binder::BinderState;
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::checker::state::CheckerState;
    use crate::parser::ParserState;
    use tsz_solver::TypeInterner;

    let source = r#"
async function test() {
    const obj: { x: number } = { x: 1 };
    for await (const x of obj) {
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
        "Expected 1 TS2495 error for for-await-of on object (AsyncIterator lib missing). All codes: {codes:?}"
    );
}

// =============================================================================
// Parameter Ordering Tests (TS1016)
// =============================================================================

/// Test that TS1016 is emitted when a required parameter follows an optional parameter
#[test]
fn test_required_param_after_optional_ts1016() {
    use crate::binder::BinderState;
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::checker::state::CheckerState;
    use crate::parser::ParserState;
    use tsz_solver::TypeInterner;

    let source = r#"
function foo(a?: number, b: string) {
    return a;
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

    let ts1016_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| {
            d.code == diagnostic_codes::A_REQUIRED_PARAMETER_CANNOT_FOLLOW_AN_OPTIONAL_PARAMETER
        })
        .count();

    assert_eq!(
        ts1016_count, 1,
        "Expected TS1016 for required parameter after optional. Got: {:?}",
        checker.ctx.diagnostics
    );
}

/// Test that TS1016 is emitted for arrow functions
#[test]
fn test_required_param_after_optional_arrow_ts1016() {
    use crate::binder::BinderState;
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::checker::state::CheckerState;
    use crate::parser::ParserState;
    use tsz_solver::TypeInterner;

    let source = r#"
const fn = (a?: number, b: string) => a;
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

    let ts1016_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| {
            d.code == diagnostic_codes::A_REQUIRED_PARAMETER_CANNOT_FOLLOW_AN_OPTIONAL_PARAMETER
        })
        .count();

    assert_eq!(
        ts1016_count, 1,
        "Expected TS1016 for required parameter after optional in arrow function. Got: {:?}",
        checker.ctx.diagnostics
    );
}

/// Test that TS1016 is emitted for methods
#[test]
fn test_required_param_after_optional_method_ts1016() {
    use crate::binder::BinderState;
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::checker::state::CheckerState;
    use crate::parser::ParserState;
    use tsz_solver::TypeInterner;

    let source = r#"
class Foo {
    bar(a?: number, b: string) {
        return a;
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

    let ts1016_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| {
            d.code == diagnostic_codes::A_REQUIRED_PARAMETER_CANNOT_FOLLOW_AN_OPTIONAL_PARAMETER
        })
        .count();

    assert_eq!(
        ts1016_count, 1,
        "Expected TS1016 for required parameter after optional in method. Got: {:?}",
        checker.ctx.diagnostics
    );
}

/// Test that TS1016 is emitted for constructors
#[test]
fn test_required_param_after_optional_constructor_ts1016() {
    use crate::binder::BinderState;
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::checker::state::CheckerState;
    use crate::parser::ParserState;
    use tsz_solver::TypeInterner;

    let source = r#"
class Foo {
    constructor(a?: number, b: string) {}
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

    let ts1016_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| {
            d.code == diagnostic_codes::A_REQUIRED_PARAMETER_CANNOT_FOLLOW_AN_OPTIONAL_PARAMETER
        })
        .count();

    assert_eq!(
        ts1016_count, 1,
        "Expected TS1016 for required parameter after optional in constructor. Got: {:?}",
        checker.ctx.diagnostics
    );
}

/// Test that no TS1016 is emitted when all parameters are properly ordered
#[test]
fn test_no_ts1016_for_proper_parameter_order() {
    use crate::binder::BinderState;
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::checker::state::CheckerState;
    use crate::parser::ParserState;
    use tsz_solver::TypeInterner;

    let source = r#"
function foo(a: number, b?: string, c?: boolean) {
    return a;
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

    let ts1016_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| {
            d.code == diagnostic_codes::A_REQUIRED_PARAMETER_CANNOT_FOLLOW_AN_OPTIONAL_PARAMETER
        })
        .count();

    assert_eq!(
        ts1016_count, 0,
        "Expected no TS1016 for proper parameter order. Got: {:?}",
        checker.ctx.diagnostics
    );
}

/// Test that TS1016 is NOT emitted when required parameter has default value (it becomes optional)
#[test]
fn test_no_ts1016_for_param_with_default_after_optional() {
    use crate::binder::BinderState;
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::checker::state::CheckerState;
    use crate::parser::ParserState;
    use tsz_solver::TypeInterner;

    let source = r#"
function foo(a?: number, b: string = "default") {
    return a;
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

    let ts1016_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| {
            d.code == diagnostic_codes::A_REQUIRED_PARAMETER_CANNOT_FOLLOW_AN_OPTIONAL_PARAMETER
        })
        .count();

    assert_eq!(
        ts1016_count, 0,
        "Expected no TS1016 when parameter has default value. Got: {:?}",
        checker.ctx.diagnostics
    );
}

/// Test that rest parameter can follow optional parameter (no TS1016)
#[test]
fn test_no_ts1016_for_rest_param_after_optional() {
    use crate::binder::BinderState;
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::checker::state::CheckerState;
    use crate::parser::ParserState;
    use tsz_solver::TypeInterner;

    let source = r#"
function foo(a?: number, ...rest: string[]) {
    return a;
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

    let ts1016_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| {
            d.code == diagnostic_codes::A_REQUIRED_PARAMETER_CANNOT_FOLLOW_AN_OPTIONAL_PARAMETER
        })
        .count();

    assert_eq!(
        ts1016_count, 0,
        "Expected no TS1016 for rest parameter after optional. Got: {:?}",
        checker.ctx.diagnostics
    );
}
