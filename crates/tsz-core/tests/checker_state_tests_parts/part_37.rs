//! Tests for Checker - Type checker using `NodeArena` and Solver
//!
//! This module contains comprehensive type checking tests organized into categories:
//! - Basic type checking (creation, intrinsic types, type interning)
//! - Type compatibility and assignability
//! - Excess property checking
//! - Function overloads and call resolution
//! - Generic types and type inference
//! - Control flow analysis
//! - Error diagnostics
use crate::binder::BinderState;
use crate::checker::state::CheckerState;
use crate::parser::ParserState;
use crate::parser::node::NodeArena;
use crate::test_fixtures::{TestContext, merge_shared_lib_symbols, setup_lib_contexts};
use tsz_solver::{TypeId, TypeInterner, Visibility, types::RelationCacheKey, types::TypeData};

// =============================================================================
// Basic Type Checker Tests
// =============================================================================
/// TS Unsoundness #43: Concrete to abstract class assignment
///
/// A concrete class is a subtype of its abstract base class.
///
/// EXPECTED FAILURES: Instance to abstract class type assignability
/// has issues with class type comparison. Currently expects 3 errors.
#[test]
fn test_concrete_extends_abstract() {
    use crate::parser::ParserState;

    let source = r#"
abstract class Shape {
    abstract area(): number;
    describe(): string {
        return "I am a shape";
    }
}

class Circle extends Shape {
    constructor(public radius: number) {
        super();
    }
    area(): number {
        return 3.14 * this.radius * this.radius;
    }
}

class Square extends Shape {
    constructor(public side: number) {
        super();
    }
    area(): number {
        return this.side * this.side;
    }
}

// Concrete classes should be assignable to abstract type
const shape1: Shape = new Circle(5); // Should be OK
const shape2: Shape = new Square(4); // Should be OK

// Array of abstract type should hold concrete instances
const shapes: Shape[] = [new Circle(1), new Square(2)]; // Should be OK
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

    let error_count = checker.ctx.diagnostics.len();

    // Class inheritance type checking now works - expect 0 errors
    if error_count != 0 {
        println!("=== Concrete Extends Abstract Diagnostics ===");
        println!("Expected 0 errors (class inheritance fixed), got {error_count}");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] {}", diag.start, diag.message_text);
        }
    }

    assert_eq!(
        error_count, 0,
        "Expected 0 errors (class inheritance now works): {:?}",
        checker.ctx.diagnostics
    );
}

/// TS Unsoundness #29: The Global Function Type (The Untyped Callable)
///
/// The global `Function` interface behaves like an untyped supertype for all callables.
/// - Any arrow function/method is assignable to `Function`
/// - `Function` is NOT safe to call (effectively `(...args: any[]) => any`)
/// - It differs from `{}` or `object` because it allows bind/call/apply
///
/// Note: This test defines a local Function interface since the global
/// Function type requires lib.d.ts which isn't available in tests.
#[test]
fn test_global_function_type_callable_assignability() {
    use crate::parser::ParserState;

    let source = r#"
// Define a minimal Function-like interface for testing
interface FunctionLike {
    (...args: any[]): any;
    bind(thisArg: any): FunctionLike;
    call(thisArg: any, ...args: any[]): any;
    apply(thisArg: any, args: any[]): any;
}

// Various callable types
const arrow = (x: number) => x * 2;
const func = function(s: string): string { return s.toUpperCase(); };
function named(a: number, b: number): number { return a + b; }

// All callables should be assignable to the untyped callable interface
// (In real TS, these would be assignable to Function)
type AnyCallable = (...args: any[]) => any;

const c1: AnyCallable = arrow; // OK
const c2: AnyCallable = func; // OK
const c3: AnyCallable = named; // OK
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

    if !checker.ctx.diagnostics.is_empty() {
        println!("=== Global Function Type Callable Assignability Diagnostics ===");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] {}", diag.start, diag.message_text);
        }
    }

    assert!(
        checker.ctx.diagnostics.is_empty(),
        "All callables should be assignable to untyped callable: {:?}",
        checker.ctx.diagnostics
    );
}

/// TS Unsoundness #29: Function type is not assignable to specific callable
///
/// The untyped `Function` cannot be safely assigned to a specific function type
/// because we don't know its actual signature.
#[test]
fn test_function_not_assignable_to_specific() {
    use crate::parser::ParserState;

    let source = r#"
// Untyped callable (simulating Function)
type AnyCallable = (...args: any[]) => any;

// Specific function type
type SpecificFn = (x: number, y: number) => number;

declare const untyped: AnyCallable;

// Untyped should NOT be directly assignable to specific
// (unless the target is `any`)
const specific: SpecificFn = untyped; // This is actually allowed in TS due to any
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

    // In TypeScript, (...args: any[]) => any IS assignable to specific functions
    // because `any` disables type checking. This is intentional unsoundness.
    if !checker.ctx.diagnostics.is_empty() {
        println!("=== Function Not Assignable Diagnostics ===");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] {}", diag.start, diag.message_text);
        }
    }

    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Untyped callable with any is assignable due to any unsoundness: {:?}",
        checker.ctx.diagnostics
    );
}

/// TS Unsoundness #29: Function type hierarchy
///
/// Tests that callable types form a proper hierarchy:
/// - Specific callable <: (...args: any[]) => any
/// - Object types without call signatures are NOT callable
#[test]
fn test_function_type_hierarchy() {
    use crate::parser::ParserState;

    let source = r#"
// Various function types in the hierarchy
type VoidFn = () => void;
type NumberFn = (x: number) => number;
type StringFn = (s: string) => string;
type GenericFn = <T>(x: T) => T;

// Untyped callable at the top
type AnyCallable = (...args: any[]) => any;

// Specific functions are assignable to untyped
declare const voidFn: VoidFn;
declare const numberFn: NumberFn;
declare const stringFn: StringFn;

const a1: AnyCallable = voidFn; // OK: VoidFn <: AnyCallable
const a2: AnyCallable = numberFn; // OK: NumberFn <: AnyCallable
const a3: AnyCallable = stringFn; // OK: StringFn <: AnyCallable

// Non-callable object is NOT assignable to function type
interface NotCallable {
    value: number;
}
declare const obj: NotCallable;
// const bad: AnyCallable = obj; // This would be an error
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

    if !checker.ctx.diagnostics.is_empty() {
        println!("=== Function Type Hierarchy Diagnostics ===");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] {}", diag.start, diag.message_text);
        }
    }

    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Function type hierarchy should work: {:?}",
        checker.ctx.diagnostics
    );
}

/// TS Unsoundness #32: Best Common Type (BCT) Inference
///
/// When inferring an array literal `[1, "a"]`, TS creates `(number | string)[]`
/// not a tuple. The algorithm gathers all element types and finds a common supertype,
/// or creates a union if none exists.
#[test]
fn test_best_common_type_array_literal() {
    use crate::parser::ParserState;

    let source = r#"
// Mixed array literal becomes union type
const mixed = [1, "hello", 2, "world"];
// Type should be (number | string)[]

// Accessing elements returns the union
const elem = mixed[0]; // number | string

// Can push either type
mixed.push(3);
mixed.push("test");

// Homogeneous array stays as single type
const numbers = [1, 2, 3, 4];
const n: number = numbers[0]; // OK

const strings = ["a", "b", "c"];
const s: string = strings[0]; // OK
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

    if !checker.ctx.diagnostics.is_empty() {
        println!("=== Best Common Type Array Literal Diagnostics ===");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] {}", diag.start, diag.message_text);
        }
    }

    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Best common type inference should work: {:?}",
        checker.ctx.diagnostics
    );
}

/// TS Unsoundness #32: BCT with class hierarchy
///
/// When array elements share a common base class, the array type
/// should use the common base (if annotated) or union of concrete types.
///
/// EXPECTED FAILURE: Class instance to base class type assignability
/// has issues. Currently expects 1 error.
#[test]
fn test_best_common_type_class_hierarchy() {
    use crate::parser::ParserState;

    let source = r#"
class Animal {
    name: string = "";
}

class Dog extends Animal {
    bark() { return "woof"; }
}

class Cat extends Animal {
    meow() { return "meow"; }
}

// Without annotation: union of concrete types
const pets = [new Dog(), new Cat()];
// Type is (Dog | Cat)[]

// With annotation: should use the annotated type
const animals: Animal[] = [new Dog(), new Cat()];
// Type should be Animal[]

// Can access common properties on union
const pet = pets[0];
const name = pet.name; // OK: both Dog and Cat have name
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

    let error_count = checker.ctx.diagnostics.len();

    // Class inheritance now works - expect 0 errors
    if error_count != 0 {
        println!("=== Best Common Type Class Hierarchy Diagnostics ===");
        println!("Expected 0 errors (class inheritance fixed), got {error_count}");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] {}", diag.start, diag.message_text);
        }
    }

    assert_eq!(
        error_count, 0,
        "Expected 0 errors (class inheritance now works): {:?}",
        checker.ctx.diagnostics
    );
}

/// TS Unsoundness #32: BCT type widening behavior
///
/// Literal types in array literals get widened to their base types
/// unless the array is const or has a specific annotation.
#[test]
fn test_best_common_type_literal_widening() {
    use crate::parser::ParserState;

    let source = r#"
// Literal types widen in mutable arrays
const nums = [1, 2, 3]; // number[] not (1 | 2 | 3)[]
nums.push(4); // OK because it's number[]

const strs = ["a", "b"]; // string[] not ("a" | "b")[]
strs.push("c"); // OK

// Const assertion preserves literals (as readonly tuple)
const literalNums = [1, 2, 3] as const; // readonly [1, 2, 3]
// literalNums.push(4); // Would error: readonly

// Boolean literal widening
const bools = [true, false]; // boolean[]
const b: boolean = bools[0]; // OK
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

    if !checker.ctx.diagnostics.is_empty() {
        println!("=== Best Common Type Literal Widening Diagnostics ===");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] {}", diag.start, diag.message_text);
        }
    }

    assert!(
        checker.ctx.diagnostics.is_empty(),
        "BCT literal widening should work: {:?}",
        checker.ctx.diagnostics
    );
}

/// TS Unsoundness #44: Module Augmentation Merging - Interface Merging
///
/// Interfaces with the same name in the same scope merge.
/// Multiple interface declarations combine their members.
#[test]
fn test_interface_merging_basic() {
    use crate::parser::ParserState;

    let source = r#"
// First interface declaration
interface Box {
    width: number;
    height: number;
}

// Second declaration merges with first
interface Box {
    depth: number;
    label: string;
}

// The merged interface has all properties
const box: Box = {
    width: 10,
    height: 20,
    depth: 30,
    label: "Storage"
};

// Can access all merged properties
const w: number = box.width;
const h: number = box.height;
const d: number = box.depth;
const l: string = box.label;
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

    if !checker.ctx.diagnostics.is_empty() {
        println!("=== Interface Merging Basic Diagnostics ===");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] {}", diag.start, diag.message_text);
        }
    }

    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Interface merging should work: {:?}",
        checker.ctx.diagnostics
    );
}

/// TS Unsoundness #44: Interface merging with method overloads
///
/// When interfaces merge, methods with the same name become overloads.
#[test]
fn test_interface_merging_method_overloads() {
    use crate::parser::ParserState;

    let source = r#"
interface Calculator {
    add(a: number, b: number): number;
}

interface Calculator {
    add(a: string, b: string): string;
    multiply(a: number, b: number): number;
}

// Merged interface has both overloads of add and multiply
declare const calc: Calculator;

const numResult: number = calc.add(1, 2);
const strResult: string = calc.add("a", "b");
const product: number = calc.multiply(3, 4);
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

    if !checker.ctx.diagnostics.is_empty() {
        println!("=== Interface Merging Method Overloads Diagnostics ===");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] {}", diag.start, diag.message_text);
        }
    }

    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Interface merging with overloads should work: {:?}",
        checker.ctx.diagnostics
    );
}

/// TS Unsoundness #44: Interface extending and merging
///
/// Interfaces can both extend other interfaces and merge with
/// other declarations of the same name.
///
/// NOTE: Currently ignored - interface extending and merging is not fully implemented.
#[test]
fn test_interface_extend_and_merge() {
    use crate::parser::ParserState;

    let source = r#"
interface Named {
    name: string;
}

interface Person extends Named {
    age: number;
}

// Merge more properties into Person
interface Person {
    email: string;
}

// Person now has name (from Named), age, and email
const person: Person = {
    name: "Alice",
    age: 30,
    email: "alice@example.com"
};

const n: string = person.name;
const a: number = person.age;
const e: string = person.email;
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

    if !checker.ctx.diagnostics.is_empty() {
        println!("=== Interface Extend and Merge Diagnostics ===");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] {}", diag.start, diag.message_text);
        }
    }

    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Interface extend and merge should work: {:?}",
        checker.ctx.diagnostics
    );
}

