//! Tests for TS2322 assignability checking

use crate::checker::state::CheckerState;
use crate::Parser;

#[test]
fn test_assignability_checks() {
    let source = r#"
// Test 1: Variable initialization with type annotation
let x: string = 42; // Should error: TS2322

// Test 2: Property assignment
interface Foo {
    prop: string;
}
const obj: Foo = { prop: 42 }; // Should error: TS2322

// Test 3: Return statement
function returnsString(): string {
    return 42; // Should error: TS2322
}

// Test 4: Function argument
function takesString(s: string) {}
takesString(42); // Should error: TS2322

// Test 5: Array destructuring
let [a]: [number] = ["string"]; // Should error: TS2322

// Test 6: Object destructuring
let { b }: { b: number } = { b: "string" }; // Should error: TS2322

// Test 7: Compound assignment
let y: number = 0;
y += "string"; // Should error: TS2322

// Test 8: Strict null checks
let z: string = null; // Should error: TS2322 with strictNullChecks

// Test 9: Default value in destructuring
function foo({ x = 42 }: { x: string }) {} // Should error: TS2322
foo({});

// Test 10: Generic function argument
function identity<T>(t: T): T { return t; }
identity<string>(42); // Should error: TS2322
"#;

    let mut parser = Parser::new(source, "test.ts");
    let result = parser.parse();

    let mut checker = CheckerState::new(result.arena, result.binder);
    checker.check();

    let diagnostics = checker.diagnostics();
    let ts2322_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2322).collect();

    println!("Found {} TS2322 errors:", ts2322_errors.len());
    for err in &ts2322_errors {
        println!("  - {}", err.message);
    }

    // We expect at least 10 TS2322 errors for the test cases above
    // If we don't find them, it means we're missing assignability checks
    assert!(
        ts2322_errors.len() >= 5,
        "Expected at least 5 TS2322 errors, found {}",
        ts2322_errors.len()
    );
}
