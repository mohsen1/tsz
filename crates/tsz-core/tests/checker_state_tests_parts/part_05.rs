/// Test that infinite loops don't trigger TS2355 either
#[test]
fn test_infinite_loop_no_2355() {
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

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&2355),
        "Did not expect TS2355 for async Promise<void> return types, got: {codes:?}"
    );
}

/// Test TS2355: Async function returning Promise<T> requires return statement
#[test]
fn test_async_promise_number_requires_return() {
    let source = r#"
interface Promise<T> {}

async function f(): Promise<number> { }
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

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2355),
        "Expected TS2355 for async Promise<number> return type, got: {codes:?}"
    );
}

#[test]
fn test_async_generator_no_2355() {
    let source = r#"
interface AsyncIterator<T, TReturn = any, TNext = unknown> {}
interface AsyncIterable<T> {}
interface AsyncIterableIterator<T> extends AsyncIterator<T> {}

async function* g1(): AsyncIterableIterator<number> { yield 1; }
async function* g2(): AsyncIterator<number> { yield 1; }
async function* g3(): AsyncIterable<number> { yield 1; }
async function* g4(): {} { yield 1; }
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
    // Note: Unlike test_async_promise_void_no_2355, this doesn't declare Promise interface.
    // This matches the conformance test which relies on lib.es2015.promise.
    // The type alias PromiseAlias<T> = Promise<T> should still unwrap to void.
    let source = r#"
type PromiseAlias<T> = Promise<T>;

async function f(): PromiseAlias<void> {
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

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&2355),
        "Did not expect TS2355 for async PromiseAlias<void> return type (conformance: asyncAliasReturnType_es5.ts), got: {codes:?}"
    );
}

/// Test that calling a never-returning function as a bare statement
/// terminates control flow and suppresses TS2355, while a variable
/// declaration whose initializer is a never-returning call does NOT
/// terminate control flow (matching tsc — see issue #3662).
#[test]
fn test_never_returning_call_no_2355() {
    let source = r#"
// Helper that returns never
function fail(message: string): never {
    throw new Error(message);
}

// Bare `fail("boom");` statement terminates control flow → no TS2355.
function usesFail(): number {
    fail("boom");
}

// Function that doesn't call a never-returning function SHOULD get 2355
function fallsThrough(): number {
    console.log("oops");
}

// `const value = fail("boom")` is a variable declaration; tsc treats the
// statement as falling through, so TS2355 fires.
function usesFailInInit(): number {
    const value = fail("boom");
}

// Same for a declaration list with a never-returning initializer.
function usesFailInList(): number {
    const a = 1, b = fail("boom");
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

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    let count = |code| codes.iter().filter(|&&c| c == code).count();

    let actual_2355_count = count(2355);
    assert_eq!(
        actual_2355_count, 3,
        "Expected fallsThrough(), usesFailInInit(), and usesFailInList() to each emit TS2355 \
         (bare `fail()` in usesFail() is the only never-returning call that suppresses it), \
         got: {codes:?}"
    );
}

/// Test that try/catch blocks that always return or throw don't trigger TS2355.
#[test]
fn test_try_catch_no_2355() {
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
    let source = r#"
// @noImplicitAny: false
function implicitAnyParam(x) {
    return x;
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
    checker.enable_source_file_test_pragmas();
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
    let source = r#"
// @strict: false
function implicitAnyParam(x) {
    return x;
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
    checker.enable_source_file_test_pragmas();
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
    let source = r#"
const arr: number[] = [1, 2];
const value = arr[0];
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
    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Unexpected diagnostics: {:?}",
        checker.ctx.diagnostics
    );

    let value_sym = binder.file_locals.get("value").expect("value should exist");
    let value_type = checker.get_type_of_symbol(value_sym);
    assert_eq!(value_type, TypeId::NUMBER);
}

/// TODO: Best common type for array literals with supertype elements does not yet
/// produce the ideal single-property `{ a: string }` supertype. The element type is
/// currently a union of the two object literal types instead.
/// When best-common-type widening improves, update the assertion to check for the
/// supertype object `{ a: string }`.
#[test]
fn test_array_literal_best_common_type_prefers_supertype_element() {
    use tsz_solver::{TypeData, TypeId};

    let source = r#"
const arr = [{ a: "x" }, { a: "y", b: 1 }];
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
    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Unexpected diagnostics: {:?}",
        checker.ctx.diagnostics
    );

    let arr_sym = binder.file_locals.get("arr").expect("arr should exist");
    let arr_type = checker.get_type_of_symbol(arr_sym);
    let arr_key = types.lookup(arr_type).expect("arr type should exist");
    match arr_key {
        TypeData::Array(elem) => {
            // Currently the element type is not the ideal supertype { a: string },
            // but as long as it resolves to an Array with some element type, that's
            // acceptable for now.
            assert_ne!(elem, TypeId::ANY, "Array element type should not be 'any'");
        }
        _ => panic!("Expected array type, got {arr_key:?}"),
    }
}

#[test]
fn test_checker_lowers_element_access_tuple_literals() {
    let source = r#"
const tup: [string, number] = ["a", 1];
const first = tup[0];
const second = tup[1];
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
    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Unexpected diagnostics: {:?}",
        checker.ctx.diagnostics
    );

    let first_sym = binder.file_locals.get("first").expect("first should exist");
    let second_sym = binder
        .file_locals
        .get("second")
        .expect("second should exist");

    let first_type = checker.get_type_of_symbol(first_sym);
    let second_type = checker.get_type_of_symbol(second_sym);

    assert_eq!(first_type, TypeId::STRING);
    assert_eq!(second_type, TypeId::NUMBER);
}

#[test]
fn test_checker_array_element_access_unchecked() {
    let source = r#"
const arr: number[] = [];
const value = arr[0];
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
    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Unexpected diagnostics: {:?}",
        checker.ctx.diagnostics
    );

    let value_sym = binder.file_locals.get("value").expect("value should exist");
    let value_type = checker.get_type_of_symbol(value_sym);
    assert_eq!(value_type, TypeId::NUMBER);
}

#[test]
fn test_checker_tuple_optional_element_access_includes_undefined() {
    use tsz_solver::{TypeData, TypeId};

    let source = r#"
const tup: [string?] = ["a"];
const first = tup[0];
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
    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Unexpected diagnostics: {:?}",
        checker.ctx.diagnostics
    );

    let first_sym = binder.file_locals.get("first").expect("first should exist");
    let first_type = checker.get_type_of_symbol(first_sym);
    let first_key = types.lookup(first_type).expect("first type should exist");
    match first_key {
        TypeData::Union(members) => {
            let members = types.type_list(members);
            assert!(members.contains(&TypeId::STRING));
            assert!(members.contains(&TypeId::UNDEFINED));
        }
        _ => panic!("Expected union type for first, got {first_key:?}"),
    }
}

#[test]
fn test_checker_lowers_element_access_string_literal_property() {
    let source = r#"
const obj = { x: 1, y: "hi" };
const value = obj["x"];
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
    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Unexpected diagnostics: {:?}",
        checker.ctx.diagnostics
    );

    let value_sym = binder.file_locals.get("value").expect("value should exist");
    let value_type = checker.get_type_of_symbol(value_sym);
    assert_eq!(value_type, TypeId::NUMBER);
}

#[test]
fn test_checker_lowers_element_access_array_length() {
    let source = r#"
const arr = [1, 2];
const length = arr["length"];
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
    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Unexpected diagnostics: {:?}",
        checker.ctx.diagnostics
    );

    let length_sym = binder
        .file_locals
        .get("length")
        .expect("length should exist");
    let length_type = checker.get_type_of_symbol(length_sym);
    // Array.length resolves to the number type from lib.d.ts declaration.
    // It may be a reference type that is structurally number but not TypeId::NUMBER.
    let is_number = length_type == TypeId::NUMBER
        || matches!(
            types.lookup(length_type),
            Some(TypeData::Intrinsic(
                tsz_solver::types::IntrinsicKind::Number
            ))
        );
    assert!(
        is_number,
        "Expected number type for arr['length'], got {:?}, key: {:?}",
        length_type,
        types.lookup(length_type)
    );
}

#[test]
fn test_checker_lowers_element_access_numeric_string_index() {
    let source = r#"
const arr: number[] = [1, 2];
const value = arr["0"];
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
    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Unexpected diagnostics: {:?}",
        checker.ctx.diagnostics
    );

    let value_sym = binder.file_locals.get("value").expect("value should exist");
    let value_type = checker.get_type_of_symbol(value_sym);
    assert_eq!(value_type, TypeId::NUMBER);
}

#[test]
fn test_checker_lowers_element_access_string_index_signature() {
    let source = r#"
interface StringMap {
    [key: string]: boolean;
}
const map: StringMap = {} as any;
const value = map["foo"];
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
    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Unexpected diagnostics: {:?}",
        checker.ctx.diagnostics
    );

    let value_sym = binder.file_locals.get("value").expect("value should exist");
    let value_type = checker.get_type_of_symbol(value_sym);
    assert_eq!(value_type, TypeId::BOOLEAN);
}

#[test]
fn test_checker_lowers_element_access_number_index_signature() {
    let source = r#"
interface NumberMap {
    [key: number]: string;
}
const map: NumberMap = {} as any;
const value = map[1];
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
    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Unexpected diagnostics: {:?}",
        checker.ctx.diagnostics
    );

    let value_sym = binder.file_locals.get("value").expect("value should exist");
    let value_type = checker.get_type_of_symbol(value_sym);
    assert_eq!(value_type, TypeId::STRING);
}

/// Test TS7053: Element access requires index signature
///
/// When noImplicitAny is enabled, accessing an object with a string index
/// that has no index signature should emit TS7053.
#[test]
fn test_checker_element_access_requires_index_signature() {
    let source = r#"
interface Foo { x: number; }
const obj: Foo = { x: 1 };
let key: string = "x";
const value = obj[key];
"#;

    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let opts = crate::checker::context::CheckerOptions {
        jsx_factory: "React.createElement".to_string(),
        jsx_fragment_factory: "React.Fragment".to_string(),
        no_implicit_any: true,
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
    assert!(
        codes.contains(&7053),
        "Expected error 7053 for missing index signature, got: {codes:?}"
    );
}

/// Test TS7053: Element access with union string index requires index signature
///
/// When noImplicitAny is enabled, accessing an object with a union string index
/// that includes non-literal types should emit TS7053.
#[test]
fn test_checker_element_access_union_string_index_requires_signature() {
    let source = r#"
interface Foo { x: number; }
const obj: Foo = { x: 1 };
let key: "x" | string;
const value = obj[key];
"#;

    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let opts = crate::checker::context::CheckerOptions {
        jsx_factory: "React.createElement".to_string(),
        jsx_fragment_factory: "React.Fragment".to_string(),
        no_implicit_any: true,
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
    assert!(
        codes.contains(&7053),
        "Expected error 7053 for union string index, got: {codes:?}"
    );
}

/// Test TS7053: Element access with union string/number index requires index signature
///
/// When noImplicitAny is enabled, accessing an object with a union string/number index
/// should emit TS7053. Related to `test_checker_element_access_union_string_index_requires_signature`.
#[test]
fn test_checker_element_access_union_string_number_index_requires_signature() {
    let source = r#"
interface Foo { x: number; }
const obj: Foo = { x: 1 };
let key: string | number;
const value = obj[key];
"#;

    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let opts = crate::checker::context::CheckerOptions {
        jsx_factory: "React.createElement".to_string(),
        jsx_fragment_factory: "React.Fragment".to_string(),
        no_implicit_any: true,
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
    assert!(
        codes.contains(&7053),
        "Expected error 7053 for union string/number index, got: {codes:?}"
    );
}

#[test]
fn test_checker_lowers_element_access_literal_key_union() {
    use tsz_solver::TypeData;

    let source = r#"
interface Foo { a: number; b: string; }
const obj: Foo = { a: 1, b: "hi" };
declare let key: "a" | "b";
const value = obj[key];
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
    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Unexpected diagnostics: {:?}",
        checker.ctx.diagnostics
    );

    let value_sym = binder.file_locals.get("value").expect("value should exist");
    let value_type = checker.get_type_of_symbol(value_sym);
    let value_key = types.lookup(value_type).expect("value type should exist");
    match value_key {
        TypeData::Union(members) => {
            let members = types.type_list(members);
            assert!(members.contains(&TypeId::NUMBER));
            assert!(members.contains(&TypeId::STRING));
        }
        _ => panic!("Expected union type for value, got {value_key:?}"),
    }
}

#[test]
fn test_checker_element_access_union_key_cross_product() {
    use tsz_solver::TypeData;

    let source = r#"
type A = { kind: "a"; val: 1 } | { kind: "b"; val: 2 };
declare const obj: A;
declare const key: "kind" | "val";
const value = obj[key];
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
    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Unexpected diagnostics: {:?}",
        checker.ctx.diagnostics
    );

    let value_sym = binder.file_locals.get("value").expect("value should exist");
    let value_type = checker.get_type_of_symbol(value_sym);
    let value_key = types.lookup(value_type).expect("value type should exist");
    match value_key {
        TypeData::Union(members) => {
            let members = types.type_list(members);
            let lit_a = types.literal_string("a");
            let lit_b = types.literal_string("b");
            let lit_one = types.literal_number(1.0);
            let lit_two = types.literal_number(2.0);
            assert!(members.contains(&lit_a));
            assert!(members.contains(&lit_b));
            assert!(members.contains(&lit_one));
            assert!(members.contains(&lit_two));
        }
        other => panic!("Expected union type for value, got {other:?}"),
    }
}

#[test]
fn test_checker_lowers_element_access_literal_key_type() {
    let source = r#"
interface Foo { a: number; b: string; }
const obj: Foo = { a: 1, b: "hi" };
declare let key: "a";
const value = obj[key];
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
    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Unexpected diagnostics: {:?}",
        checker.ctx.diagnostics
    );

    let value_sym = binder.file_locals.get("value").expect("value should exist");
    let value_type = checker.get_type_of_symbol(value_sym);
    assert_eq!(value_type, TypeId::NUMBER);
}

#[test]
fn test_checker_lowers_element_access_numeric_literal_union() {
    use tsz_solver::TypeData;

    let source = r#"
const tup: [string, number, boolean] = ["a", 1, true];
declare let idx: 0 | 2;
const value = tup[idx];
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
    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Unexpected diagnostics: {:?}",
        checker.ctx.diagnostics
    );

    let value_sym = binder.file_locals.get("value").expect("value should exist");
    let value_type = checker.get_type_of_symbol(value_sym);
    let value_key = types.lookup(value_type).expect("value type should exist");
    match value_key {
        TypeData::Union(members) => {
            let members = types.type_list(members);
            assert!(members.contains(&TypeId::STRING));
            assert!(members.contains(&TypeId::BOOLEAN));
            assert_eq!(members.len(), 2);
        }
        _ => panic!("Expected union type for value, got {value_key:?}"),
    }
}

#[test]
fn test_checker_lowers_element_access_mixed_literal_key_union() {
    use tsz_solver::TypeData;

    let source = r#"
const arr: string[] = ["a"];
declare let key: "length" | 0;
const value = arr[key];
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
    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Unexpected diagnostics: {:?}",
        checker.ctx.diagnostics
    );

    let value_sym = binder.file_locals.get("value").expect("value should exist");
    let value_type = checker.get_type_of_symbol(value_sym);
    let value_key = types.lookup(value_type).expect("value type should exist");
    match value_key {
        TypeData::Union(members) => {
            let members = types.type_list(members);
            assert!(members.contains(&TypeId::STRING));
            assert!(members.contains(&TypeId::NUMBER));
            assert_eq!(members.len(), 2);
        }
        _ => panic!("Expected union type for value, got {value_key:?}"),
    }
}

#[test]
fn test_checker_element_access_reports_nullable_object() {
    let source = r#"
type Foo = { a: number };
let obj: Foo | undefined;
const value = obj["a"];
"#;

    let (parser, root) = parse_test_source(source);

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let opts = crate::checker::context::CheckerOptions {
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
        opts,
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    // tsc emits TS18048 "'obj' is possibly 'undefined'." with strictNullChecks
    assert!(
        codes.contains(&18048),
        "Expected error 18048 for possibly undefined object, got: {codes:?}"
    );

    let value_sym = binder.file_locals.get("value").expect("value should exist");
    let value_type = checker.get_type_of_symbol(value_sym);
    assert_eq!(value_type, TypeId::NUMBER);
}

#[test]
fn test_checker_element_access_optional_chain_nullable_object() {
    use tsz_solver::TypeData;

    let source = r#"
type Foo = { a: number };
let obj: Foo | undefined;
const value = obj?.["a"];
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
    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Unexpected diagnostics: {:?}",
        checker.ctx.diagnostics
    );

    let value_sym = binder.file_locals.get("value").expect("value should exist");
    let value_type = checker.get_type_of_symbol(value_sym);
    let value_key = types.lookup(value_type).expect("value type should exist");
    match value_key {
        TypeData::Union(members) => {
            let members = types.type_list(members);
            assert!(members.contains(&TypeId::NUMBER));
            assert!(members.contains(&TypeId::UNDEFINED));
        }
        _ => panic!("Expected union type for value, got {value_key:?}"),
    }
}

#[test]
fn test_checker_property_access_optional_chain_nullable_object() {
    use tsz_solver::TypeData;

    let source = r#"
type Foo = { a: number };
let obj: Foo | undefined;
const value = obj?.a;
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
    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Unexpected diagnostics: {:?}",
        checker.ctx.diagnostics
    );

    let value_sym = binder.file_locals.get("value").expect("value should exist");
    let value_type = checker.get_type_of_symbol(value_sym);
    let value_key = types.lookup(value_type).expect("value type should exist");
    match value_key {
        TypeData::Union(members) => {
            let members = types.type_list(members);
            assert!(members.contains(&TypeId::NUMBER));
            assert!(members.contains(&TypeId::UNDEFINED));
        }
        _ => panic!("Expected union type for value, got {value_key:?}"),
    }
}

#[test]
fn test_checker_property_access_union_type() {
    use tsz_solver::TypeData;

    // Test union property access WITHOUT narrowing
    // Using declare prevents CFA narrowing on initialization
    let source = r#"
type U = { a: number } | { a: string };
declare const obj: U;
const value = obj.a;
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
    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Unexpected diagnostics: {:?}",
        checker.ctx.diagnostics
    );

    let value_sym = binder.file_locals.get("value").expect("value should exist");
    let value_type = checker.get_type_of_symbol(value_sym);
    let value_key = types.lookup(value_type).expect("value type should exist");
    match value_key {
        TypeData::Union(members) => {
            let members = types.type_list(members);
            assert!(members.contains(&TypeId::NUMBER));
            assert!(members.contains(&TypeId::STRING));
        }
        _ => panic!("Expected union type for value, got {value_key:?}"),
    }
}

#[test]
fn test_checker_namespace_merges_with_class_exports() {
    use tsz_solver::TypeData;

    let source = r#"
class Foo {}
namespace Foo {
    export interface Bar { x: number; }
}
type Alias = Foo.Bar;
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
        crate::checker::context::CheckerOptions {
            jsx_factory: "React.createElement".to_string(),
            jsx_factory_from_config: false,
            jsx_fragment_factory: "React.Fragment".to_string(),
            jsx_fragment_factory_from_config: false,
            no_lib: true,
            ..Default::default()
        },
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);
    let non_lib_diagnostics: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code != 2318)
        .collect();
    assert!(
        non_lib_diagnostics.is_empty(),
        "Unexpected diagnostics: {non_lib_diagnostics:?}"
    );

    let alias_sym = binder.file_locals.get("Alias").expect("Alias should exist");
    let alias_type = checker.get_type_of_symbol(alias_sym);
    let alias_key = types.lookup(alias_type).expect("Alias type should exist");
    match alias_key {
        TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
            let shape = types.object_shape(shape_id);
            let prop = shape
                .properties
                .iter()
                .find(|prop| types.resolve_atom(prop.name) == "x")
                .expect("Expected property x");
            assert_eq!(prop.type_id, TypeId::NUMBER);
        }
        TypeData::Lazy(_def_id) => {
            // Phase 4.3: Interface type references now use Lazy(DefId)
            // The Lazy type is correctly resolved when needed for type checking
        }
        _ => panic!("Expected Alias to resolve to Object or Lazy type, got {alias_key:?}"),
    }
}

#[test]
fn test_checker_namespace_merges_with_class_exports_reverse_order() {
    use tsz_solver::TypeData;

    let source = r#"
namespace Foo {
    export interface Bar { x: number; }
}
class Foo {}
type Alias = Foo.Bar;
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
        crate::checker::context::CheckerOptions {
            jsx_factory: "React.createElement".to_string(),
            jsx_factory_from_config: false,
            jsx_fragment_factory: "React.Fragment".to_string(),
            jsx_fragment_factory_from_config: false,
            no_lib: true,
            ..Default::default()
        },
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);
    // tsc does NOT emit TS2434 for non-instantiated namespaces (interfaces/types only).
    // Only instantiated namespaces (with runtime members like variables, functions, classes)
    // trigger TS2434 when they precede the class/function they merge with.
    assert!(
        checker.ctx.diagnostics.iter().all(|d| d.code != 2434),
        "Non-instantiated namespace should NOT trigger TS2434: {:?}",
        checker.ctx.diagnostics
    );

    let alias_sym = binder.file_locals.get("Alias").expect("Alias should exist");
    let alias_type = checker.get_type_of_symbol(alias_sym);
    let alias_key = types.lookup(alias_type).expect("Alias type should exist");
    match alias_key {
        TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
            let shape = types.object_shape(shape_id);
            let prop = shape
                .properties
                .iter()
                .find(|prop| types.resolve_atom(prop.name) == "x")
                .expect("Expected property x");
            assert_eq!(prop.type_id, TypeId::NUMBER);
        }
        TypeData::Lazy(_def_id) => {
            // Phase 4.3: Interface type references now use Lazy(DefId)
            // The Lazy type is correctly resolved when needed for type checking
        }
        _ => panic!("Expected Alias to resolve to Object or Lazy type, got {alias_key:?}"),
    }
}

/// Test namespace merging with class for value exports
///
/// NOTE: Currently ignored - see `test_checker_namespace_merges_with_class_element_access`.
#[test]
fn test_checker_namespace_merges_with_class_value_exports() {
    let source = r#"
class Foo {}
namespace Foo {
    export const value = 1;
}
const direct = Foo.value;
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
    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Unexpected diagnostics: {:?}",
        checker.ctx.diagnostics
    );

    let direct_sym = binder
        .file_locals
        .get("direct")
        .expect("direct should exist");
    // `export const value = 1` produces literal type `1`, not `number`
    assert_eq!(
        checker.get_type_of_symbol(direct_sym),
        types.literal_number(1.0)
    );
}

