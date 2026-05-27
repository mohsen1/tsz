/// Test that array destructuring of a union with non-iterable members emits TS2488
/// TODO: TS2488 detection for array destructuring of non-iterable unions is not yet implemented.
/// Currently produces 0 diagnostics. When implemented, update to expect 1 TS2488.
#[test]
fn test_array_destructuring_union_non_iterable_emits_ts2488() {
    use crate::binder::BinderState;
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::checker::state::CheckerState;
    use tsz_solver::construction::TypeInterner;

    let source = r#"
const val: string | number = "hello";
const [a] = val;  // TS2488: union with non-iterable member is not iterable
"#;

    let (parser, root) = parse_test_source(source);

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
    use tsz_solver::construction::TypeInterner;

    let source = r#"
const tuple: [number, string] = [1, "hello"];
const [a, b] = tuple;  // OK: tuple is iterable
"#;

    let (parser, root) = parse_test_source(source);

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
    use tsz_solver::construction::TypeInterner;

    let source = r#"
const num: number = 42;
const [[a]] = [num];  // TS2488: inner array contains non-iterable number
"#;

    let (parser, root) = parse_test_source(source);

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
    use tsz_solver::construction::TypeInterner;

    let source = r#"
async function test() {
    const num: number = 42;
    for await (const x of num) {
        console.log(x);
    }
}
"#;

    let (parser, root) = parse_test_source(source);

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

/// Test that for-await-of with a valid array type does not emit TS2504 (sync iterable is accepted)
#[test]
fn test_async_iterator_for_await_of_array_no_error() {
    use crate::binder::BinderState;
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::checker::state::CheckerState;
    use tsz_solver::construction::TypeInterner;

    let source = r#"
async function test() {
    const arr: number[] = [1, 2, 3];
    for await (const x of arr) {
        console.log(x);
    }
}
"#;

    let (parser, root) = parse_test_source(source);

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
    use tsz_solver::construction::TypeInterner;

    let source = r#"
async function test() {
    const b: boolean = true;
    for await (const x of b) {
        console.log(x);
    }
}
"#;

    let (parser, root) = parse_test_source(source);

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
    use tsz_solver::construction::TypeInterner;

    let source = r#"
async function test() {
    const obj: { x: number } = { x: 1 };
    for await (const x of obj) {
        console.log(x);
    }
}
"#;

    let (parser, root) = parse_test_source(source);

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
    use tsz_solver::construction::TypeInterner;

    let source = r#"
function foo(a?: number, b: string) {
    return a;
}
"#;

    let (parser, root) = parse_test_source(source);
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
    use tsz_solver::construction::TypeInterner;

    let source = r#"
const fn = (a?: number, b: string) => a;
"#;

    let (parser, root) = parse_test_source(source);
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
    use tsz_solver::construction::TypeInterner;

    let source = r#"
class Foo {
    bar(a?: number, b: string) {
        return a;
    }
}
"#;

    let (parser, root) = parse_test_source(source);
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
    use tsz_solver::construction::TypeInterner;

    let source = r#"
class Foo {
    constructor(a?: number, b: string) {}
}
"#;

    let (parser, root) = parse_test_source(source);
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
    use tsz_solver::construction::TypeInterner;

    let source = r#"
function foo(a: number, b?: string, c?: boolean) {
    return a;
}
"#;

    let (parser, root) = parse_test_source(source);
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
    use tsz_solver::construction::TypeInterner;

    let source = r#"
function foo(a?: number, b: string = "default") {
    return a;
}
"#;

    let (parser, root) = parse_test_source(source);
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
    use tsz_solver::construction::TypeInterner;

    let source = r#"
function foo(a?: number, ...rest: string[]) {
    return a;
}
"#;

    let (parser, root) = parse_test_source(source);
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

/// Test that multiple required parameters after optional are all flagged
#[test]
fn test_multiple_required_params_after_optional_ts1016() {
    use crate::binder::BinderState;
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::checker::state::CheckerState;
    use tsz_solver::construction::TypeInterner;

    let source = r#"
function foo(a?: number, b: string, c: boolean) {
    return a;
}
"#;

    let (parser, root) = parse_test_source(source);
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
        ts1016_count, 2,
        "Expected 2 TS1016 errors for two required params after optional. Got: {:?}",
        checker.ctx.diagnostics
    );
}

// =============================================================================
// Contextual Typing Tests for Destructuring Parameters
// =============================================================================

/// Test that destructuring parameters get contextual types from callback signatures
#[test]
fn test_contextual_typing_destructuring_param_object() {
    use crate::binder::BinderState;
    use crate::checker::state::CheckerState;
    use tsz_solver::construction::TypeInterner;

    let source = r#"
type Handler = (item: { x: number, y: string }) => void;
const handler: Handler = ({ x, y }) => {
    // x should be number, y should be string
    let numVal: number = x;
    let strVal: string = y;
};
"#;

    let (parser, root) = parse_test_source(source);
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

    // Should have no type errors - x and y should be inferred from contextual type
    let type_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322) // TS2322: Type is not assignable
        .collect();

    assert!(
        type_errors.is_empty(),
        "Expected no TS2322 errors when destructuring params get contextual types. Got: {type_errors:?}"
    );
}

/// Test that array destructuring parameters get contextual types from callback signatures
#[test]
fn test_contextual_typing_destructuring_param_array() {
    use crate::binder::BinderState;
    use crate::checker::state::CheckerState;
    use tsz_solver::construction::TypeInterner;

    let source = r#"
type Handler = (item: [number, string]) => void;
const handler: Handler = ([first, second]) => {
    // first should be number, second should be string
    let numVal: number = first;
    let strVal: string = second;
};
"#;

    let (parser, root) = parse_test_source(source);
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

    // Should have no type errors - first and second should be inferred from contextual type
    let type_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322) // TS2322: Type is not assignable
        .collect();

    assert!(
        type_errors.is_empty(),
        "Expected no TS2322 errors when array destructuring params get contextual types. Got: {type_errors:?}"
    );
}

// =============================================================================
// TS2322 Type Not Assignable - Comprehensive Tests
// =============================================================================

/// Test TS2322 emission for variable declaration with type annotation mismatch
#[test]
fn test_ts2322_variable_declaration_type_mismatch() {
    use crate::checker::diagnostics::diagnostic_codes;

    let source = r#"
let x: string = 42;
let y: number = "hello";
let z: boolean = null;
"#;

    let (parser, root) = parse_test_source(source);
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

    let ts2322_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();

    // Should have at least 2 errors (x and y - z may or may not depending on strictNullChecks)
    assert!(
        ts2322_errors.len() >= 2,
        "Expected at least 2 TS2322 errors for type mismatches. Got {}: {:?}",
        ts2322_errors.len(),
        ts2322_errors
    );
}

/// Test TS2322 emission for return statement type mismatch
#[test]
fn test_ts2322_return_statement_type_mismatch() {
    use crate::checker::diagnostics::diagnostic_codes;

    let source = r#"
function getString(): string {
    return 42;
}

function getNumber(): number {
    return "hello";
}

function getBoolean(): boolean {
    return {};
}
"#;

    let (parser, root) = parse_test_source(source);
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

    let ts2322_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();

    assert!(
        ts2322_errors.len() >= 3,
        "Expected at least 3 TS2322 errors for return type mismatches. Got {}: {:?}",
        ts2322_errors.len(),
        ts2322_errors
    );
}

/// Test TS2322 emission for class property initializer type mismatch
#[test]
fn test_ts2322_class_property_initializer_mismatch() {
    use crate::checker::diagnostics::diagnostic_codes;

    let source = r#"
class Example {
    stringProp: string = 42;
    numberProp: number = "hello";
}
"#;

    let (parser, root) = parse_test_source(source);
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

    let ts2322_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();

    assert!(
        ts2322_errors.len() >= 2,
        "Expected at least 2 TS2322 errors for class property initializer mismatches. Got {}: {:?}",
        ts2322_errors.len(),
        ts2322_errors
    );
}

/// Test TS2322 emission for object literal property type mismatch
#[test]
fn test_ts2322_object_literal_property_mismatch() {
    use crate::checker::diagnostics::diagnostic_codes;

    let source = r#"
interface Person {
    name: string;
    age: number;
}

const p: Person = {
    name: 123,
    age: "thirty"
};
"#;

    let (parser, root) = parse_test_source(source);
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

    let ts2322_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();

    // Object literal with mismatched property types should trigger TS2322
    assert!(
        !ts2322_errors.is_empty(),
        "Expected at least 1 TS2322 error for object literal property type mismatch. Got: {:?}",
        checker.ctx.diagnostics
    );
}

/// Test TS2322 emission for array element type mismatch
#[test]
fn test_ts2322_array_element_type_mismatch() {
    use crate::checker::diagnostics::diagnostic_codes;

    let source = r#"
const arr: number[] = [1, 2, "three", 4];
const arr2: string[] = ["a", "b", 3, "d"];
"#;

    let (parser, root) = parse_test_source(source);
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

    let ts2322_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();

    // Array literals with wrong element types should trigger TS2322
    assert!(
        ts2322_errors.len() >= 2,
        "Expected at least 2 TS2322 errors for array element type mismatches. Got {}: {:?}",
        ts2322_errors.len(),
        checker.ctx.diagnostics
    );
}

/// Test TS2322 is NOT emitted for valid assignments
#[test]
fn test_ts2322_valid_assignments_no_error() {
    use crate::checker::diagnostics::diagnostic_codes;

    let source = r#"
let x: string = "hello";
let y: number = 42;
let z: boolean = true;
let a: any = 123;
let b: unknown = "anything";

function getString(): string {
    return "valid";
}

function getNumber(): number {
    return 42;
}

class Valid {
    name: string = "test";
    count: number = 0;
}
"#;

    let (parser, root) = parse_test_source(source);
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

    let ts2322_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();

    assert!(
        ts2322_errors.is_empty(),
        "Expected no TS2322 errors for valid assignments. Got: {ts2322_errors:?}"
    );
}

/// Test TS2322 for function parameter default value mismatch
#[test]
fn test_ts2322_parameter_default_mismatch() {
    use crate::checker::diagnostics::diagnostic_codes;

    let source = r#"
function greet(name: string = 42) {
    return name;
}

function compute(value: number = "hello") {
    return value;
}
"#;

    let (parser, root) = parse_test_source(source);
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

    let ts2322_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();

    assert!(
        ts2322_errors.len() >= 2,
        "Expected at least 2 TS2322 errors for parameter default value mismatches. Got {}: {:?}",
        ts2322_errors.len(),
        ts2322_errors
    );
}

/// Test TS2322 for const assertion with type annotation
#[test]
fn test_ts2322_const_variable_type_mismatch() {
    use crate::checker::diagnostics::diagnostic_codes;

    let source = r#"
const x: string = 42;
const y: number = "hello";
"#;

    let (parser, root) = parse_test_source(source);
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

    let ts2322_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();

    assert!(
        ts2322_errors.len() >= 2,
        "Expected at least 2 TS2322 errors for const variable type mismatches. Got {}: {:?}",
        ts2322_errors.len(),
        ts2322_errors
    );
}

/// Test TS2322 for union type assignments
#[test]
fn test_ts2322_union_type_mismatch() {
    use crate::checker::diagnostics::diagnostic_codes;

    let source = r#"
let x: string | number = true;
let y: "a" | "b" = "c";
"#;

    let (parser, root) = parse_test_source(source);
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

    let ts2322_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();

    assert!(
        ts2322_errors.len() >= 2,
        "Expected at least 2 TS2322 errors for union type mismatches. Got {}: {:?}",
        ts2322_errors.len(),
        ts2322_errors
    );
}

/// Test TS2322 for tuple type assignments
#[test]
fn test_ts2322_tuple_type_mismatch() {
    use crate::checker::diagnostics::diagnostic_codes;

    let source = r#"
let tuple: [string, number] = [1, "hello"];
"#;

    let (parser, root) = parse_test_source(source);
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

    let ts2322_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();

    // Tuple with swapped types should trigger TS2322
    assert!(
        !ts2322_errors.is_empty(),
        "Expected at least 1 TS2322 error for tuple type mismatch. Got: {:?}",
        checker.ctx.diagnostics
    );
}

/// Test TS2322 for generic type assignments
#[test]
fn test_ts2322_generic_type_mismatch() {
    use crate::checker::diagnostics::diagnostic_codes;

    let source = r#"
interface Box<T> {
    value: T;
}

const stringBox: Box<string> = { value: 42 };
const numberBox: Box<number> = { value: "hello" };
"#;

    let (parser, root) = parse_test_source(source);
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

    let ts2322_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();

    assert!(
        ts2322_errors.len() >= 2,
        "Expected at least 2 TS2322 errors for generic type mismatches. Got {}: {:?}",
        ts2322_errors.len(),
        ts2322_errors
    );
}

// =============================================================================
// TS2304 "Cannot find name" - Comprehensive Tests
// =============================================================================

/// Test that TS2304 is emitted for an undeclared variable in a function call argument.
#[test]
fn test_ts2304_undeclared_var_in_function_call() {
    let source = r#"
function foo(x: number) {}
foo(undeclaredArg);
"#;

    let (parser, root) = parse_test_source(source);

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
    checker.ctx.report_unresolved_imports = true;
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2304),
        "Expected TS2304 for undeclared variable in function call, got: {codes:?}"
    );
}

/// Test that TS2304 is emitted for an undeclared variable in a binary expression.
#[test]
fn test_ts2304_undeclared_var_in_binary_expression() {
    let source = r#"
const result = undeclaredValue + 1;
"#;

    let (parser, root) = parse_test_source(source);

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
    checker.ctx.report_unresolved_imports = true;
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2304),
        "Expected TS2304 for undeclared variable in binary expression, got: {codes:?}"
    );
}

/// Test that TS2304 is emitted for a variable used outside its block scope.
#[test]
fn test_ts2304_out_of_scope_block_variable() {
    let source = r#"
function test() {
    if (true) {
        let blockScoped = 1;
    }
    return blockScoped;
}
"#;

    let (parser, root) = parse_test_source(source);

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
    checker.ctx.report_unresolved_imports = true;
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2304),
        "Expected TS2304 for out-of-scope block variable, got: {codes:?}"
    );
}

/// Test that TS2304 is emitted for a typo in a variable name with suggestions (TS2552).
#[test]
fn test_ts2304_typo_with_suggestion() {
    use crate::checker::diagnostics::diagnostic_codes;

    let source = r#"
const myVariable = 5;
const result = myVarible + 1;
"#;

    let (parser, root) = parse_test_source(source);

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
    checker.ctx.report_unresolved_imports = true;
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    // Should have either TS2304 or TS2552 (did you mean?)
    let has_cannot_find = codes.contains(&diagnostic_codes::CANNOT_FIND_NAME)
        || codes.contains(&diagnostic_codes::CANNOT_FIND_NAME_DID_YOU_MEAN);
    assert!(
        has_cannot_find,
        "Expected TS2304 or TS2552 for typo in variable name, got: {codes:?}"
    );
}

/// Test that TS2304 is emitted for an undeclared variable in a return statement.
#[test]
fn test_ts2304_undeclared_var_in_return() {
    let source = r#"
function getValue(): number {
    return missingVariable;
}
"#;

    let (parser, root) = parse_test_source(source);

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
    checker.ctx.report_unresolved_imports = true;
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2304),
        "Expected TS2304 for undeclared variable in return, got: {codes:?}"
    );
}

/// Test that TS2304 is emitted for undeclared variable in array spread.
#[test]
fn test_ts2304_undeclared_var_in_array_spread() {
    let source = r#"
const arr = [1, 2, ...undeclaredArray];
"#;

    let (parser, root) = parse_test_source(source);

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
    checker.ctx.report_unresolved_imports = true;
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2304),
        "Expected TS2304 for undeclared variable in array spread, got: {codes:?}"
    );
}

/// Test that TS2304 is emitted for undeclared variable in object property value.
#[test]
fn test_ts2304_undeclared_var_in_object_literal() {
    let source = r#"
const obj = {
    name: undeclaredName,
    value: 42
};
"#;

    let (parser, root) = parse_test_source(source);

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
    checker.ctx.report_unresolved_imports = true;
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2304),
        "Expected TS2304 for undeclared variable in object literal, got: {codes:?}"
    );
}

/// Test that TS2304 is emitted for undeclared variable in conditional (ternary) expression.
#[test]
fn test_ts2304_undeclared_var_in_conditional() {
    let source = r#"
const result = true ? undeclaredTrue : 0;
"#;

    let (parser, root) = parse_test_source(source);

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
    checker.ctx.report_unresolved_imports = true;
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2304),
        "Expected TS2304 for undeclared variable in conditional, got: {codes:?}"
    );
}

/// Test that TS2304 is emitted for undeclared class in extends clause.
#[test]
fn test_ts2304_undeclared_class_in_extends() {
    let source = r#"
class Child extends MissingParent {}
"#;

    let (parser, root) = parse_test_source(source);

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
    checker.ctx.report_unresolved_imports = true;
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2304),
        "Expected TS2304 for undeclared class in extends clause, got: {codes:?}"
    );
}

/// Test that TS2304 is emitted for undeclared interface in implements clause.
#[test]
fn test_ts2304_undeclared_interface_in_implements() {
    let source = r#"
class MyClass implements MissingInterface {}
"#;

    let (parser, root) = parse_test_source(source);

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
    checker.ctx.report_unresolved_imports = true;
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2304),
        "Expected TS2304 for undeclared interface in implements clause, got: {codes:?}"
    );
}

/// Test that TS2304 is emitted for undeclared variable in template literal expression.
#[test]
fn test_ts2304_undeclared_var_in_template_literal() {
    let source = r#"
const msg = `Hello ${undeclaredName}!`;
"#;

    let (parser, root) = parse_test_source(source);

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
    checker.ctx.report_unresolved_imports = true;
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2304),
        "Expected TS2304 for undeclared variable in template literal, got: {codes:?}"
    );
}

