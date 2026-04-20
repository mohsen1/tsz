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
/// TS Unsoundness #31: Generic constraint rejection - constraint not assignable to T
///
/// Verifies that while T is assignable to its constraint,
/// the constraint itself cannot be assigned back to T.
#[test]
fn test_generic_constraint_rejection() {
    use crate::parser::ParserState;

    let source = r#"
// Error case: string is not assignable to T (T could be "hello" or other literal)
function reject<T extends string>(): T {
    return "hello"; // ERROR: string is not assignable to T
}

// Similarly, the constraint type cannot be assigned to a constrained parameter
function reject2<T extends { name: string }>(obj: { name: string }): T {
    return obj; // ERROR: { name: string } is not assignable to T
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

    // Should have exactly 2 errors (one for each return statement)
    let error_count = checker.ctx.diagnostics.len();

    if error_count != 2 {
        println!("=== Generic Constraint Rejection Diagnostics ===");
        println!("Expected 2 errors, got {error_count}");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] {}", diag.start, diag.message_text);
        }
    }

    assert_eq!(
        error_count, 2,
        "Should reject constraint-to-T assignments (expected 2 errors): {:?}",
        checker.ctx.diagnostics
    );
}

/// TS Unsoundness #31: Generic parameter identity check
///
/// When checking T <: U where both are type parameters,
/// first check identity (T == U), then check Constraint(T) <: U.
#[test]
fn test_generic_param_identity() {
    use crate::parser::ParserState;

    let source = r#"
// Same type parameter is assignable to itself
function identity<T>(x: T): T {
    return x; // OK: T == T
}

// Different type parameters with compatible constraints
function compatible<T extends string, U extends string>(x: T): string {
    return x; // OK: T <: string
}

// Nested constraint: U extends T, so U <: T
function nested<T, U extends T>(x: U): T {
    return x; // OK: Constraint(U) = T, so U <: T
}

// Chain of constraints
function chain<A extends string, B extends A, C extends B>(x: C): string {
    // C <: B <: A <: string
    const a: A = x; // OK: C <: A via B
    const s: string = x; // OK: C <: string via chain
    return x;
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

    if !checker.ctx.diagnostics.is_empty() {
        println!("=== Generic Param Identity Diagnostics ===");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] {}", diag.start, diag.message_text);
        }
    }

    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Generic param identity check should work: {:?}",
        checker.ctx.diagnostics
    );
}

/// TS Unsoundness #31: Cross-file generic constraint resolution
///
/// This test verifies that generic constraints work correctly when
/// types are referenced across different "conceptual" modules.
/// Relates to the Application expansion issue in cross-file type resolution.
///
/// Property access on T where T extends `SomeType` should resolve properties
/// from the constraint during access.
///
/// Cross-scope generic constraint resolution: basic constraints, alias chains, and union constraints.
#[test]
fn test_cross_scope_generic_constraints() {
    use crate::parser::ParserState;

    let source = r#"
// Simulate cross-file scenario with type aliases
type Base = { id: number };
type Extended = Base & { name: string };

// Generic function with constraint referencing external type
function process<T extends Base>(item: T): number {
    return item.id; // Should work: T has .id because Constraint(T) = Base
}

// Constraint is a type alias to another type alias
type Identifiable = Base;
function identify<T extends Identifiable>(item: T): number {
    return item.id; // Should work: need to resolve Identifiable -> Base -> { id: number }
}

// Constraint is a union type
type Entity = { kind: "user"; name: string } | { kind: "bot"; version: number };
function getKind<T extends Entity>(entity: T): "user" | "bot" {
    return entity.kind; // Should work: both union members have .kind
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

    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Constraint property lookup should work: {:?}",
        checker.ctx.diagnostics
    );
}

/// Cross-scope generic constraint with conditional type using `infer`.
#[test]
fn test_cross_scope_generic_constraints_conditional_infer() {
    use crate::parser::ParserState;

    let source = r#"
type ExtractId<T> = T extends { id: infer I } ? I : never;
function extractId<T extends { id: number }>(item: T): ExtractId<T> {
    return item.id as ExtractId<T>;
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

    // Accept TS2352 as valid — tsc also emits this for conditional type assertions
    // when the type can't be proven to overlap with the conditional result.
    let non_ts2352: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code != 2352)
        .collect();
    assert!(
        non_ts2352.is_empty(),
        "Constraint property lookup with infer should only produce TS2352 (if any): {:?}",
        checker.ctx.diagnostics
    );
}

/// TS Unsoundness #26: Split Accessors (Getter/Setter Variance)
///
/// TypeScript allows a property to have different types for reading (Getter) vs writing (Setter).
/// - `get x(): string`
/// - `set x(v: string | number)`
///
/// The property `x` is effectively `string` (covariant) for reads, and `string | number` (contravariant) for writes.
///
/// Subtyping rules for split accessors:
/// - `Sub.read <: Sup.read` (Covariant)
/// - `Sup.write <: Sub.write` (Contravariant)
///
/// NOTE: Currently ignored - split accessor type checking is not fully implemented.
/// The property type should be derived from getter type for reads and setter type for writes.
#[test]
fn test_split_accessors_basic() {
    use crate::parser::ParserState;

    let source = r#"
class Box {
    private _value: string | number = "";

    get value(): string {
        return this._value as string;
    }

    set value(v: string | number) {
        this._value = v;
    }
}

const box = new Box();
const s: string = box.value; // OK: getter returns string
box.value = "hello"; // OK: setter accepts string
box.value = 42; // OK: setter accepts number
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

    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Split accessor basic usage should work: {:?}",
        checker.ctx.diagnostics
    );
}

/// TS Unsoundness #26: Split Accessors - read type mismatch should error
#[test]
fn test_split_accessors_read_error() {
    use crate::parser::ParserState;

    let source = r#"
class Box {
    get value(): string {
        return "hello";
    }
    set value(v: string | number) {}
}

const box = new Box();
const n: number = box.value; // ERROR: string not assignable to number
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
    if error_count != 1 {
        println!("=== Split Accessors Read Error Diagnostics ===");
        println!("Expected 1 error, got {error_count}");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] {}", diag.start, diag.message_text);
        }
    }

    assert_eq!(
        error_count, 1,
        "Should error when reading getter returns incompatible type: {:?}",
        checker.ctx.diagnostics
    );
}

/// TS Unsoundness #26: Split Accessors - write type mismatch should error
///
/// Setter assignment type checking verifies that the value being assigned
/// matches the setter parameter type.
#[test]
fn test_split_accessors_write_error() {
    use crate::parser::ParserState;

    let source = r#"
class Box {
    get value(): string {
        return "hello";
    }
    set value(v: string) {} // Setter only accepts string
}

const box = new Box();
box.value = true; // Should ERROR: boolean not assignable to string
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

    assert_eq!(
        error_count, 1,
        "Expected 1 error for boolean assigned to string setter: {:?}",
        checker.ctx.diagnostics
    );
}

/// TS Unsoundness #43: Abstract Class Instantiation
///
/// Abstract classes cannot be instantiated directly.
/// - `new AbstractClass()` -> Error
/// - But `AbstractClass` is a subtype of `Function` (it has a prototype)
/// - You can define types that accept abstract constructors: `abstract new () => any`
#[test]
fn test_abstract_class_instantiation_error() {
    use crate::parser::ParserState;

    let source = r#"
declare const console: { log: (message: string) => void };

abstract class Animal {
    abstract speak(): void;
}

class Dog extends Animal {
    speak() {}
}

const dog = new Dog(); // OK: Dog is concrete
const animal = new Animal(); // ERROR: Cannot create instance of abstract class
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
    if error_count != 1 {
        println!("=== Abstract Class Instantiation Diagnostics ===");
        println!("Expected 1 error, got {error_count}");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] {}", diag.start, diag.message_text);
        }
    }

    assert_eq!(
        error_count, 1,
        "Should error on abstract class instantiation: {:?}",
        checker.ctx.diagnostics
    );
}

/// TS Unsoundness #43: Abstract constructor type assignability
///
/// `ConcreteConstructor` <: `AbstractConstructor` -> True
/// `AbstractConstructor` <: `ConcreteConstructor` -> False
///
/// EXPECTED FAILURES: typeof class and constructor type assignability
/// has issues with type resolution. Currently expects 4 errors.
#[test]
fn test_abstract_constructor_assignability() {
    use crate::parser::ParserState;

    let source = r#"
abstract class Animal {
    abstract speak(): void;
}

class Dog extends Animal {
    speak() {}
}

class Cat extends Animal {
    speak() {}
}

// Using typeof to get constructor types
type AnimalCtor = typeof Animal;
type DogCtor = typeof Dog;

// Concrete class constructor can be used where abstract is expected (via type alias)
const ctor1: AnimalCtor = Dog; // Should be OK: Dog extends Animal

// But we cannot instantiate the abstract class via its constructor type
function createAnimal(Ctor: typeof Animal): Animal {
    // This would be: return new Ctor(); // ERROR if Ctor is abstract
    return new Dog(); // Workaround for test
}

const animal = createAnimal(Animal); // Passing abstract class as value should be OK
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

    // Fixed: Abstract constructor assignability now works correctly
    // Concrete class constructors can be assigned to abstract class constructor types
    if error_count != 0 {
        println!("=== Abstract Constructor Assignability Diagnostics ===");
        println!("Expected 0 errors, got {error_count}");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] {}", diag.start, diag.message_text);
        }
    }

    assert_eq!(
        error_count, 0,
        "Expected 0 errors (abstract constructor assignability fixed): {:?}",
        checker.ctx.diagnostics
    );
}

/// Test abstract to concrete constructor type assignability
///
/// Abstract constructor types should NOT be assignable to concrete constructor types.
/// This matches TypeScript's behavior.
///
/// NOTE: Currently ignored - the checker doesn't emit TS2322 errors for abstract to
/// concrete constructor assignments. The assignability check exists but doesn't
/// properly detect this case or emit the expected diagnostic.
#[test]
fn test_abstract_to_concrete_constructor_not_assignable() {
    use crate::parser::ParserState;

    let source = r#"
class A {}

abstract class B extends A {}

class C extends B {}

// Test 1: Abstract B to Concrete A - Should error (TS2322)
var AA: typeof A = B;

// Test 2: Concrete A to Abstract B - Should be OK (no error)
var BB: typeof B = A;

// Test 3: Abstract B to Concrete C - Should error (TS2322)
var CC: typeof C = B;
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

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    let not_assignable_count = codes.iter().filter(|&&code| code == 2322).count();

    // Debug: print all diagnostics
    println!("=== Abstract to Concrete Constructor Diagnostics ===");
    println!("Total diagnostics: {}", checker.ctx.diagnostics.len());
    for diag in &checker.ctx.diagnostics {
        println!("[{}] Code {}: {}", diag.start, diag.code, diag.message_text);
    }
    println!(
        "Abstract constructor types in context: {:?}",
        checker.ctx.abstract_constructor_types
    );

    // Should have 2 TS2322 errors:
    // - Line 8: typeof B (abstract) to typeof A (concrete)
    // - Line 14: typeof B (abstract) to typeof C (concrete)
    assert_eq!(
        not_assignable_count, 2,
        "Expected 2 TS2322 errors for abstract to concrete constructor assignment, got: {:?}\nDiagnostics: {:?}",
        codes, checker.ctx.diagnostics
    );
}

