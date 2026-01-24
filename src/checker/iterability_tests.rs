//! Tests for TS2488 iterability checking

use crate::Parser;
use crate::checker::state::CheckerState;

#[test]
fn test_array_destructuring_non_iterable() {
    let source = r#"
// Test 1: Array destructuring of number should emit TS2488
const [a] = 123;

// Test 2: Array destructuring of boolean should emit TS2488
const [b] = true;

// Test 3: Array destructuring of object should emit TS2488
const [c] = {};

// Test 4: Array destructuring of undefined should emit TS2488
const [d] = undefined;

// Test 5: Array destructuring of null should emit TS2488
const [e] = null;

// Test 6: Array destructuring of void should emit TS2488
const [f] = void 0;

// Test 7: Array destructuring of function should emit TS2488
const [g] = function() {};

// Test 8: Nested array destructuring of non-iterable
const [[h]] = 123;

// Test 9: Array destructuring with rest element of non-iterable
const [...i] = 123;
"#;

    let mut parser = Parser::new(source, "test.ts");
    let result = parser.parse();

    let mut checker = CheckerState::new(result.arena, result.binder);
    checker.check();

    let diagnostics = checker.diagnostics();
    let ts2488_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2488).collect();

    println!("Found {} TS2488 errors:", ts2488_errors.len());
    for err in &ts2488_errors {
        println!("  - {}", err.message);
    }

    // We expect at least 9 TS2488 errors for the test cases above
    assert!(
        ts2488_errors.len() >= 9,
        "Expected at least 9 TS2488 errors, found {}",
        ts2488_errors.len()
    );
}

#[test]
fn test_array_destructuring_iterable_types() {
    let source = r#"
// Test 1: Array destructuring of array - should NOT error
const [a] = [1, 2, 3];

// Test 2: Array destructuring of string - should NOT error
const [b] = "hello";

// Test 3: Array destructuring of tuple - should NOT error
type Tuple = [number, string];
const [c, d] = [1, "test"] as Tuple;

// Test 4: Array destructuring of custom iterable - should NOT error
interface Iterable {
    [Symbol.iterator](): Iterator<number>;
}
interface Iterator<T> {
    next(): { value: T; done: boolean };
}
const iterable: Iterable = {
    [Symbol.iterator]: () => ({
        next: () => ({ value: 1, done: true })
    })
};
const [e] = iterable;
"#;

    let mut parser = Parser::new(source, "test.ts");
    let result = parser.parse();

    let mut checker = CheckerState::new(result.arena, result.binder);
    checker.check();

    let diagnostics = checker.diagnostics();
    let ts2488_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2488).collect();

    println!(
        "Found {} TS2488 errors for valid iterables:",
        ts2488_errors.len()
    );
    for err in &ts2488_errors {
        println!("  - {}", err.message);
    }

    // We expect NO TS2488 errors for these valid iterable types
    assert_eq!(
        ts2488_errors.len(),
        0,
        "Expected 0 TS2488 errors for valid iterables, found {}",
        ts2488_errors.len()
    );
}

#[test]
fn test_for_of_non_iterable() {
    let source = r#"
// Test 1: for-of loop with number should emit TS2488
for (const x of 123) {}

// Test 2: for-of loop with boolean should emit TS2488
for (const x of true) {}

// Test 3: for-of loop with object should emit TS2488
for (const x of {}) {}
"#;

    let mut parser = Parser::new(source, "test.ts");
    let result = parser.parse();

    let mut checker = CheckerState::new(result.arena, result.binder);
    checker.check();

    let diagnostics = checker.diagnostics();
    let ts2488_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2488).collect();

    println!("Found {} TS2488 errors in for-of:", ts2488_errors.len());
    for err in &ts2488_errors {
        println!("  - {}", err.message);
    }

    // We expect at least 3 TS2488 errors for the test cases above
    assert!(
        ts2488_errors.len() >= 3,
        "Expected at least 3 TS2488 errors, found {}",
        ts2488_errors.len()
    );
}

#[test]
fn test_spread_non_iterable() {
    let source = r#"
// Test 1: Spread of number in array literal should emit TS2488
const arr = [...123];

// Test 2: Spread of boolean in array literal should emit TS2488
const arr2 = [...true];

// Test 3: Spread of object in array literal should emit TS2488
const arr3 = [...{}];

// Test 4: Spread in function call of non-iterable should emit TS2488
function foo(...args: number[]) {}
foo(123);
"#;

    let mut parser = Parser::new(source, "test.ts");
    let result = parser.parse();

    let mut checker = CheckerState::new(result.arena, result.binder);
    checker.check();

    let diagnostics = checker.diagnostics();
    let ts2488_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2488).collect();

    println!("Found {} TS2488 errors for spread:", ts2488_errors.len());
    for err in &ts2488_errors {
        println!("  - {}", err.message);
    }

    // We expect at least 4 TS2488 errors for the test cases above
    assert!(
        ts2488_errors.len() >= 4,
        "Expected at least 4 TS2488 errors, found {}",
        ts2488_errors.len()
    );
}

#[test]
fn test_union_iterability() {
    let source = r#"
// Test 1: Union with all iterable members - should NOT error
const [a] = Math.random() < 0.5 ? [1] : "hello";

// Test 2: Union with non-iterable member should emit TS2488
const [b] = Math.random() < 0.5 ? 123 : "hello";

// Test 3: Union with multiple non-iterable members should emit TS2488
const [c] = Math.random() < 0.5 ? 123 : true;
"#;

    let mut parser = Parser::new(source, "test.ts");
    let result = parser.parse();

    let mut checker = CheckerState::new(result.arena, result.binder);
    checker.check();

    let diagnostics = checker.diagnostics();
    let ts2488_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2488).collect();

    println!("Found {} TS2488 errors for unions:", ts2488_errors.len());
    for err in &ts2488_errors {
        println!("  - {}", err.message);
    }

    // We expect at least 2 TS2488 errors for unions with non-iterable members
    assert!(
        ts2488_errors.len() >= 2,
        "Expected at least 2 TS2488 errors, found {}",
        ts2488_errors.len()
    );
}
