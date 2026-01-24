//! Parser error tests for TS1005 and TS2300 false positive reduction

use crate::checker::state::CheckerState;
use crate::Parser;

/// Test ASI (Automatic Semicolon Insertion) edge cases
#[test]
fn test_asi_return_statements() {
    let source = r#"
// Test 1: Return with line break should NOT error TS1005
function foo1() {
    return
    42
}

// Test 2: Return with semicolon should NOT error
function foo2() {
    return 42;
}

// Test 3: Return on same line with expression should NOT error
function foo3() {
    return 42
}

// Test 4: Multiple returns with line breaks
function foo4() {
    return
    1
    return
    2
}
"#;

    let mut parser = Parser::new(source, "test.ts");
    let result = parser.parse();

    let mut checker = CheckerState::new(result.arena, result.binder);
    checker.check();

    let diagnostics = checker.diagnostics();
    let ts1005_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 1005).collect();

    println!(
        "Found {} TS1005 errors for ASI return statements:",
        ts1005_errors.len()
    );
    for err in &ts1005_errors {
        println!("  - {} at line {}", err.message, err.span.start);
    }

    // We expect NO TS1005 errors for these valid ASI patterns
    assert_eq!(
        ts1005_errors.len(),
        0,
        "Expected 0 TS1005 errors for valid ASI return statements, found {}",
        ts1005_errors.len()
    );
}

/// Test trailing commas in various contexts
#[test]
fn test_trailing_commas() {
    let source = r#"
// Test 1: Trailing comma in array literal - should NOT error
const arr1 = [1, 2, 3,];

// Test 2: Trailing comma in object literal - should NOT error
const obj1 = { a: 1, b: 2, };

// Test 3: Trailing comma in parameter list - should NOT error
function foo(a, b, c,) {}

// Test 4: Trailing comma in function arguments - should NOT error
foo(1, 2, 3,);

// Test 5: Trailing comma in destructuring - should NOT error
const [x, y,] = [1, 2];

// Test 6: Trailing comma in object destructuring - should NOT error
const { a, b, } = { a: 1, b: 2 };

// Test 7: Trailing comma in enum - should NOT error
enum E {
    A,
    B,
    C,
}
"#;

    let mut parser = Parser::new(source, "test.ts");
    let result = parser.parse();

    let mut checker = CheckerState::new(result.arena, result.binder);
    checker.check();

    let diagnostics = checker.diagnostics();
    let ts1005_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 1005).collect();

    println!(
        "Found {} TS1005 errors for trailing commas:",
        ts1005_errors.len()
    );
    for err in &ts1005_errors {
        println!("  - {} at line {}", err.message, err.span.start);
    }

    // We expect NO TS1005 errors for valid trailing commas
    assert_eq!(
        ts1005_errors.len(),
        0,
        "Expected 0 TS1005 errors for valid trailing commas, found {}",
        ts1005_errors.len()
    );
}

/// Test function overloads are NOT duplicates (TS2300)
#[test]
fn test_function_overloads_not_duplicates() {
    let source = r#"
// Test 1: Function overloads - should NOT error TS2300
function foo(x: string): void;
function foo(x: number): void;
function foo(x: string | number): void {
    foo(x);
}

// Test 2: Method overloads - should NOT error TS2300
class MyClass {
    method(x: string): void;
    method(x: number): void;
    method(x: string | number): void {
        this.method(x);
    }
}
"#;

    let mut parser = Parser::new(source, "test.ts");
    let result = parser.parse();

    let mut checker = CheckerState::new(result.arena, result.binder);
    checker.check();

    let diagnostics = checker.diagnostics();
    let ts2300_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2300).collect();

    println!(
        "Found {} TS2300 errors for function overloads:",
        ts2300_errors.len()
    );
    for err in &ts2300_errors {
        println!("  - {} at line {}", err.message, err.span.start);
    }

    // We expect NO TS2300 errors for valid function overloads
    assert_eq!(
        ts2300_errors.len(),
        0,
        "Expected 0 TS2300 errors for valid function overloads, found {}",
        ts2300_errors.len()
    );
}

/// Test interface merging is NOT duplicate (TS2300)
#[test]
fn test_interface_merging_not_duplicates() {
    let source = r#"
// Test 1: Interface merging - should NOT error TS2300
interface Foo {
    a: string;
}
interface Foo {
    b: number;
}

// Test 2: Interface extending interface - should NOT error TS2300
interface Base {
    x: number;
}
interface Extended extends Base {
    y: string;
}

// Test 3: Multiple interface declarations with methods - should NOT error TS2300
interface Greeter {
    greet(): string;
}
interface Greeter {
    farewell(): string;
}
"#;

    let mut parser = Parser::new(source, "test.ts");
    let result = parser.parse();

    let mut checker = CheckerState::new(result.arena, result.binder);
    checker.check();

    let diagnostics = checker.diagnostics();
    let ts2300_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2300).collect();

    println!(
        "Found {} TS2300 errors for interface merging:",
        ts2300_errors.len()
    );
    for err in &ts2300_errors {
        println!("  - {} at line {}", err.message, err.span.start);
    }

    // We expect NO TS2300 errors for valid interface merging
    assert_eq!(
        ts2300_errors.len(),
        0,
        "Expected 0 TS2300 errors for valid interface merging, found {}",
        ts2300_errors.len()
    );
}

/// Test namespace + function/class merging is allowed (TS2300)
#[test]
fn test_namespace_function_class_merging() {
    let source = r#"
// Test 1: Namespace + function merging - should NOT error TS2300
function Util() {}
namespace Util {
    export function helper() {}
}

// Test 2: Namespace + class merging - should NOT error TS2300
class Container {}
namespace Container {
    export class Inner {}
}

// Test 3: Function + namespace merging - should NOT error TS2300
namespace Model {
    export interface Options {}
}
function Model() {}
"#;

    let mut parser = Parser::new(source, "test.ts");
    let result = parser.parse();

    let mut checker = CheckerState::new(result.arena, result.binder);
    checker.check();

    let diagnostics = checker.diagnostics();
    let ts2300_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 2300).collect();

    println!(
        "Found {} TS2300 errors for namespace merging:",
        ts2300_errors.len()
    );
    for err in &ts2300_errors {
        println!("  - {} at line {}", err.message, err.span.start);
    }

    // We expect NO TS2300 errors for valid namespace merging
    assert_eq!(
        ts2300_errors.len(),
        0,
        "Expected 0 TS2300 errors for valid namespace merging, found {}",
        ts2300_errors.len()
    );
}

/// Test brace-less control structures with ASI
#[test]
fn test_asi_brace_less_control_structures() {
    let source = r#"
// Test 1: if statement without braces - should NOT error TS1005
if (true)
    console.log("yes")

// Test 2: while loop without braces - should NOT error TS1005
while (false)
    break

// Test 3: for loop without braces - should NOT error TS1005
for (let i = 0; i < 10; i++)
    console.log(i)
"#;

    let mut parser = Parser::new(source, "test.ts");
    let result = parser.parse();

    let mut checker = CheckerState::new(result.arena, result.binder);
    checker.check();

    let diagnostics = checker.diagnostics();
    let ts1005_errors: Vec<_> = diagnostics.iter().filter(|d| d.code == 1005).collect();

    println!(
        "Found {} TS1005 errors for brace-less structures:",
        ts1005_errors.len()
    );
    for err in &ts1005_errors {
        println!("  - {} at line {}", err.message, err.span.start);
    }

    // We expect NO TS1005 errors for valid brace-less control structures
    assert_eq!(
        ts1005_errors.len(),
        0,
        "Expected 0 TS1005 errors for valid brace-less structures, found {}",
        ts1005_errors.len()
    );
}
