//! Tests for spread and rest operator type checking

use crate::checker::types::Diagnostic;
use crate::test_fixtures::TestContext;

/// Helper function to check source and return diagnostics
fn check_source(source: &str) -> Vec<Diagnostic> {
    let mut ctx = TestContext::new_without_lib();
    let mut parser = crate::parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    ctx.binder.bind_source_file(parser.get_arena(), root);
    let mut checker = ctx.checker();
    checker.check_source_file(root);
    checker.ctx.diagnostics.clone()
}

#[test]
fn test_array_spread_with_tuple() {
    let source = r#"
type Tuple = [string, number];
const t: Tuple = ["hello", 42];
const arr = [...t];  // Should be (string | number)[]
"#;

    let diagnostics = check_source(source);

    // Should NOT emit TS2322 or TS2488
    let errors = diagnostics
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

    let diagnostics = check_source(source);

    // Should NOT emit TS2322 or TS2488
    let errors = diagnostics
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

    let diagnostics = check_source(source);

    // Should emit TS2488
    let ts2488_count = diagnostics.iter().filter(|d| d.code == 2488).count();
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

    let _diagnostics = check_source(source);
    // This is a complex case - spread in tuple context
    // The behavior depends on implementation
}

#[test]
fn test_object_spread() {
    let source = r#"
const obj1 = { a: 1, b: 2 };
const obj2 = { c: 3 };
const merged = { ...obj1, ...obj2 };  // Should be { a: number, b: number, c: number }
"#;

    let diagnostics = check_source(source);

    // Should NOT emit TS2322
    let ts2322_count = diagnostics.iter().filter(|d| d.code == 2322).count();
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

    let diagnostics = check_source(source);

    // Should NOT emit TS2322
    let ts2322_count = diagnostics.iter().filter(|d| d.code == 2322).count();
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

    let diagnostics = check_source(source);

    // Should emit TS2322 for string argument
    let ts2322_count = diagnostics.iter().filter(|d| d.code == 2322).count();
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

    let diagnostics = check_source(source);

    // Should NOT emit TS2322
    let ts2322_count = diagnostics.iter().filter(|d| d.code == 2322).count();
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

    let diagnostics = check_source(source);

    // Should NOT emit TS2322
    let ts2322_count = diagnostics.iter().filter(|d| d.code == 2322).count();
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

    let diagnostics = check_source(source);

    // Should NOT emit TS2322
    let ts2322_count = diagnostics.iter().filter(|d| d.code == 2322).count();
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

    let diagnostics = check_source(source);

    // Should emit TS2322
    let ts2322_count = diagnostics.iter().filter(|d| d.code == 2322).count();
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

    let diagnostics = check_source(source);

    // Should NOT emit TS2322
    let ts2322_count = diagnostics.iter().filter(|d| d.code == 2322).count();
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

    let diagnostics = check_source(source);

    // Should emit TS2322
    let ts2322_count = diagnostics.iter().filter(|d| d.code == 2322).count();
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

    let diagnostics = check_source(source);

    // Should NOT emit TS2322
    let ts2322_count = diagnostics.iter().filter(|d| d.code == 2322).count();
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

    let diagnostics = check_source(source);

    // Should NOT emit TS2322
    let ts2322_count = diagnostics.iter().filter(|d| d.code == 2322).count();
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

    let diagnostics = check_source(source);

    // Should emit TS2322 for number argument
    let ts2322_count = diagnostics.iter().filter(|d| d.code == 2322).count();
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

    let _diagnostics = check_source(source);
    // This is a complex case - spread in tuple context
    // The behavior depends on implementation
}

#[test]
fn test_spread_string() {
    let source = r#"
const str = "hello";
const chars = [...str];  // Should be string[]
"#;

    let diagnostics = check_source(source);

    // Should NOT emit TS2488 (string is iterable)
    let ts2488_count = diagnostics.iter().filter(|d| d.code == 2488).count();
    assert_eq!(
        ts2488_count, 0,
        "Expected no TS2488 error for string spread, got {}",
        ts2488_count
    );
}
