//! Tests for contextual typing and type inference

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
fn test_arrow_function_parameter_inference_from_map() {
    let source = r#"
const numbers = [1, 2, 3];
const strings = numbers.map(x => x.toString());
// x should be inferred as number from the array
"#;

    let diagnostics = check_source(source);

    // Should NOT emit TS2322 - x should be correctly inferred as number
    let ts2322_count = diagnostics.iter().filter(|d| d.code == 2322).count();
    assert_eq!(
        ts2322_count, 0,
        "Expected no TS2322 error for arrow function parameter inference, got {}",
        ts2322_count
    );
}

#[test]
fn test_arrow_function_parameter_inference_with_context() {
    let source = r#"
type Handler = (n: number) => string;
const h: Handler = n => n.toString();
// n should be inferred as number from the Handler type
"#;

    let diagnostics = check_source(source);

    // Should NOT emit TS2322 - n should be correctly inferred
    let ts2322_count = diagnostics.iter().filter(|d| d.code == 2322).count();
    assert_eq!(
        ts2322_count, 0,
        "Expected no TS2322 error for arrow function with contextual type, got {}",
        ts2322_count
    );
}

#[test]
fn test_object_literal_property_inference() {
    let source = r#"
interface Person {
    name: string;
    age: number;
}
const p: Person = { name: "Alice", age: 30 };
// Properties should be contextually typed
"#;

    let diagnostics = check_source(source);

    // Should NOT emit TS2322
    let ts2322_count = diagnostics.iter().filter(|d| d.code == 2322).count();
    assert_eq!(
        ts2322_count, 0,
        "Expected no TS2322 error for object literal property inference, got {}",
        ts2322_count
    );
}

#[test]
fn test_return_statement_contextual_typing() {
    let source = r#"
function getString(): string {
    return "hello";
}
function getNumber(): number {
    return 42;
}
"#;

    let diagnostics = check_source(source);

    // Should NOT emit TS2322
    let ts2322_count = diagnostics.iter().filter(|d| d.code == 2322).count();
    assert_eq!(
        ts2322_count, 0,
        "Expected no TS2322 error for return statement contextual typing, got {}",
        ts2322_count
    );
}

#[test]
fn test_ternary_branch_contextual_typing() {
    let source = r#"
let x: string;
x = Math.random() > 0.5 ? "hello" : "world";
// Both branches should be contextually typed as string
"#;

    let diagnostics = check_source(source);

    // Should NOT emit TS2322
    let ts2322_count = diagnostics.iter().filter(|d| d.code == 2322).count();
    assert_eq!(
        ts2322_count, 0,
        "Expected no TS2322 error for ternary branch contextual typing, got {}",
        ts2322_count
    );
}

#[test]
fn test_destructuring_contextual_typing() {
    let source = r#"
const obj = { x: 1, y: 2 };
const { x, y }: { x: number; y: number } = obj;
// x and y should be contextually typed as number
"#;

    let diagnostics = check_source(source);

    // Should NOT emit TS2322
    let ts2322_count = diagnostics.iter().filter(|d| d.code == 2322).count();
    assert_eq!(
        ts2322_count, 0,
        "Expected no TS2322 error for destructuring contextual typing, got {}",
        ts2322_count
    );
}

#[test]
fn test_arrow_function_return_inference() {
    let source = r#"
const numbers = [1, 2, 3];
const doubled = numbers.map(x => x * 2);
// Return type should be inferred as number
// x should be inferred as number from the array
"#;

    let diagnostics = check_source(source);

    // Should NOT emit TS2322
    let ts2322_count = diagnostics.iter().filter(|d| d.code == 2322).count();
    assert_eq!(
        ts2322_count, 0,
        "Expected no TS2322 error for arrow function return inference, got {}",
        ts2322_count
    );
}

#[test]
fn test_object_literal_method_inference() {
    let source = r#"
interface Calculator {
    add(a: number, b: number): number;
}
const calc: Calculator = {
    add(a, b) {
        return a + b;
    }
};
// Parameters a and b should be inferred as number
// Return type should be inferred as number
"#;

    let diagnostics = check_source(source);

    // Should NOT emit TS2322
    let ts2322_count = diagnostics.iter().filter(|d| d.code == 2322).count();
    assert_eq!(
        ts2322_count, 0,
        "Expected no TS2322 error for object literal method inference, got {}",
        ts2322_count
    );
}

#[test]
fn test_array_literal_contextual_typing() {
    let source = r#"
const arr: number[] = [1, 2, 3];
// Elements should be contextually typed as number
const arr2: string[] = ["a", "b", "c"];
// Elements should be contextually typed as string
"#;

    let diagnostics = check_source(source);

    // Should NOT emit TS2322
    let ts2322_count = diagnostics.iter().filter(|d| d.code == 2322).count();
    assert_eq!(
        ts2322_count, 0,
        "Expected no TS2322 error for array literal contextual typing, got {}",
        ts2322_count
    );
}

#[test]
fn test_generic_function_contextual_typing() {
    let source = r#"
function identity<T>(x: T): T {
    return x;
}
const result = identity("hello");
// T should be inferred as string
"#;

    let diagnostics = check_source(source);

    // Should NOT emit TS2322
    let ts2322_count = diagnostics.iter().filter(|d| d.code == 2322).count();
    assert_eq!(
        ts2322_count, 0,
        "Expected no TS2322 error for generic function contextual typing, got {}",
        ts2322_count
    );
}
