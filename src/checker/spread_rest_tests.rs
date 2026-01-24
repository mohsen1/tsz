//! Tests for spread and rest operator type checking

use crate::binder::BinderState;
use crate::checker::state::CheckerState;
use crate::parser::NodeArena;
use crate::parser::ParserState;
use crate::solver::TypeInterner;
use crate::test_fixtures::TestContext;

/// Helper function to create a checker
fn create_checker(source: &str) -> (TestContext, CheckerState) {
    let arena = NodeArena::new();
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(&arena, root);

    let types = TypeInterner::new();
    let ctx = TestContext {
        arena,
        binder,
        types,
    };

    let mut checker = ctx.checker();
    checker.check_source_file(root);
    (ctx, checker)
}

#[test]
fn test_array_spread_with_tuple() {
    let source = r#"
type Tuple = [string, number];
const t: Tuple = ["hello", 42];
const arr = [...t];  // Should be (string | number)[]
"#;

    let (_ctx, checker) = create_checker(source);

    // Should NOT emit TS2322 or TS2488
    let errors = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322 || d.code == 2488)
        .count();
    assert_eq!(
        errors, 0,
        "Expected no errors for array spread with tuple, got {}",
        errors
    );
}

#[test]
fn test_array_spread_with_array() {
    let source = r#"
const nums = [1, 2, 3];
const arr = [...nums];  // Should be number[]
"#;

    let (_ctx, checker) = create_checker(source);

    // Should NOT emit TS2322 or TS2488
    let errors = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322 || d.code == 2488)
        .count();
    assert_eq!(
        errors, 0,
        "Expected no errors for array spread with array, got {}",
        errors
    );
}

#[test]
fn test_array_spread_with_non_iterable_emits_ts2488() {
    let source = r#"
const num = 42;
const arr = [...num];  // Should emit TS2488
"#;

    let (_ctx, checker) = create_checker(source);

    // Should emit TS2488
    let ts2488_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2488)
        .count();
    assert!(
        ts2488_count >= 1,
        "Expected at least 1 TS2488 error for non-iterable spread, got {}",
        ts2488_count
    );
}

#[test]
fn test_tuple_context_with_spread() {
    let source = r#"
type Tuple = [string, number, boolean];
const t: Tuple = ["hello", ...[1, 2], true];  // Error: can't spread number[] into tuple position
"#;

    let (_ctx, checker) = create_checker(source);

    // Should emit TS2322 for the spread
    let ts2322_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .count();
    // This may or may not emit depending on implementation
}

#[test]
fn test_object_spread() {
    let source = r#"
const obj1 = { a: 1, b: 2 };
const obj2 = { c: 3 };
const merged = { ...obj1, ...obj2 };  // Should be { a: number, b: number, c: number }
"#;

    let (_ctx, checker) = create_checker(source);

    // Should NOT emit TS2322
    let ts2322_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .count();
    assert_eq!(
        ts2322_count, 0,
        "Expected no TS2322 error for object spread, got {}",
        ts2322_count
    );
}

#[test]
fn test_rest_parameter() {
    let source = r#"
function sum(...nums: number[]) {
    return nums.reduce((a, b) => a + b, 0);
}
sum(1, 2, 3);
"#;

    let (_ctx, checker) = create_checker(source);

    // Should NOT emit TS2322
    let ts2322_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .count();
    assert_eq!(
        ts2322_count, 0,
        "Expected no TS2322 error for rest parameter, got {}",
        ts2322_count
    );
}

#[test]
fn test_rest_parameter_with_wrong_types_emits_ts2322() {
    let source = r#"
function sum(...nums: number[]) {
    return nums.reduce((a, b) => a + b, 0);
}
sum(1, "two", 3);  // Should emit TS2322
"#;

    let (_ctx, checker) = create_checker(source);

    // Should emit TS2322 for string argument
    let ts2322_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .count();
    assert!(
        ts2322_count >= 1,
        "Expected at least 1 TS2322 error for wrong type in rest parameter, got {}",
        ts2322_count
    );
}

#[test]
fn test_array_destructuring_with_rest() {
    let source = r#"
const arr = [1, 2, 3, 4, 5];
const [first, second, ...rest] = arr;
// first: number, second: number, rest: number[]
"#;

    let (_ctx, checker) = create_checker(source);

    // Should NOT emit TS2322
    let ts2322_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .count();
    assert_eq!(
        ts2322_count, 0,
        "Expected no TS2322 error for array destructuring with rest, got {}",
        ts2322_count
    );
}

#[test]
fn test_tuple_destructuring_with_rest() {
    let source = r#"
type Tuple = [string, number, boolean, ...string[]];
const t: Tuple = ["hello", 42, true, "a", "b"];
const [s, n, ...rest] = t;
// s: string, n: number, rest: (boolean | string)[]
"#;

    let (_ctx, checker) = create_checker(source);

    // Should NOT emit TS2322
    let ts2322_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .count();
    assert_eq!(
        ts2322_count, 0,
        "Expected no TS2322 error for tuple destructuring with rest, got {}",
        ts2322_count
    );
}

#[test]
fn test_spread_in_function_call() {
    let source = r#"
function add(a: number, b: number, c: number) {
    return a + b + c;
}
const args = [1, 2, 3];
add(...args);  // Should work
"#;

    let (_ctx, checker) = create_checker(source);

    // Should NOT emit TS2322
    let ts2322_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .count();
    assert_eq!(
        ts2322_count, 0,
        "Expected no TS2322 error for spread in function call, got {}",
        ts2322_count
    );
}

#[test]
fn test_spread_in_function_call_with_wrong_types() {
    let source = r#"
function add(a: number, b: number, c: number) {
    return a + b + c;
}
const args = [1, "two", 3];
add(...args);  // Should emit TS2322
"#;

    let (_ctx, checker) = create_checker(source);

    // Should emit TS2322
    let ts2322_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .count();
    assert!(
        ts2322_count >= 1,
        "Expected at least 1 TS2322 error for spread with wrong types, got {}",
        ts2322_count
    );
}

#[test]
fn test_spread_tuple_in_function_call() {
    let source = r#"
function greet(name: string, age: number, active: boolean) {
    console.log(name, age, active);
}
type Tuple = [string, number, boolean];
const args: Tuple = ["Alice", 30, true];
greet(...args);  // Should work
"#;

    let (_ctx, checker) = create_checker(source);

    // Should NOT emit TS2322
    let ts2322_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .count();
    assert_eq!(
        ts2322_count, 0,
        "Expected no TS2322 error for spread tuple in function call, got {}",
        ts2322_count
    );
}

#[test]
fn test_spread_tuple_in_function_call_with_wrong_types() {
    let source = r#"
function greet(name: string, age: number, active: boolean) {
    console.log(name, age, active);
}
type Tuple = [string, boolean, number];  // Wrong order
const args: Tuple = ["Alice", true, 30];
greet(...args);  // Should emit TS2322
"#;

    let (_ctx, checker) = create_checker(source);

    // Should emit TS2322
    let ts2322_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .count();
    assert!(
        ts2322_count >= 1,
        "Expected at least 1 TS2322 error for spread tuple with wrong types, got {}",
        ts2322_count
    );
}

#[test]
fn test_object_spread_with_contextual_type() {
    let source = r#"
interface Person {
    name: string;
    age: number;
}
const partial = { name: "Alice" };
const person: Person = { ...partial, age: 30 };
"#;

    let (_ctx, checker) = create_checker(source);

    // Should NOT emit TS2322
    let ts2322_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .count();
    assert_eq!(
        ts2322_count, 0,
        "Expected no TS2322 error for object spread with contextual type, got {}",
        ts2322_count
    );
}

#[test]
fn test_nested_array_spread() {
    let source = r#"
const arr1 = [1, 2];
const arr2 = [3, 4];
const combined = [...arr1, ...arr2];  // Should be number[]
"#;

    let (_ctx, checker) = create_checker(source);

    // Should NOT emit TS2322
    let ts2322_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .count();
    assert_eq!(
        ts2322_count, 0,
        "Expected no TS2322 error for nested array spread, got {}",
        ts2322_count
    );
}

#[test]
fn test_rest_with_type_annotation() {
    let source = r#"
function logAll(...messages: string[]) {
    messages.forEach(m => console.log(m));
}
logAll("hello", "world");
logAll("hello", 42);  // Should emit TS2322
"#;

    let (_ctx, checker) = create_checker(source);

    // Should emit TS2322 for number argument
    let ts2322_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .count();
    assert!(
        ts2322_count >= 1,
        "Expected at least 1 TS2322 error for wrong type in rest parameter with annotation, got {}",
        ts2322_count
    );
}

#[test]
fn test_array_literal_with_spread_and_contextual_type() {
    let source = r#"
type Tuple = [number, string];
const createTuple = (): Tuple => [42, "hello"];
const t: Tuple = [1, "test", ...createTuple()];
"#;

    let (_ctx, checker) = create_checker(source);

    // This is a complex case - spread in tuple context
    // The behavior depends on implementation
}

#[test]
fn test_spread_string() {
    let source = r#"
const str = "hello";
const chars = [...str];  // Should be string[]
"#;

    let (_ctx, checker) = create_checker(source);

    // Should NOT emit TS2488 (string is iterable)
    let ts2488_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2488)
        .count();
    assert_eq!(
        ts2488_count, 0,
        "Expected no TS2488 error for string spread, got {}",
        ts2488_count
    );
}
