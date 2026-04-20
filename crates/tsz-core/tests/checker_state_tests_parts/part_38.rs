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
/// TS Unsoundness #44: Namespace and interface merging
///
/// Namespaces can merge with interfaces to add static members.
///
/// EXPECTED FAILURE: Namespace-interface merging for value-space access
/// is not yet implemented. Currently expects 2 errors.
#[test]
fn test_namespace_interface_merging() {
    use crate::parser::ParserState;

    let source = r##"
interface Color {
    r: number;
    g: number;
    b: number;
}

namespace Color {
    export function fromHex(hex: string): Color {
        return { r: 0, g: 0, b: 0 };
    }
    export const RED: Color = { r: 255, g: 0, b: 0 };
}

// Use as interface type
const myColor: Color = { r: 100, g: 150, b: 200 };

// Use namespace members (these should work but currently fail)
const red: Color = Color.RED;
const fromString: Color = Color.fromHex("#FF0000");
"##;

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

    // Now expects 0 errors: both interface member access (myColor.r, etc.) and
    // namespace value access (Color.RED, Color.fromHex) work correctly after
    // fixing interface+namespace merge type resolution.
    assert_eq!(
        error_count, 0,
        "Expected 0 errors for namespace-interface merging: {:?}",
        checker.ctx.diagnostics
    );
}

/// TS Unsoundness #44: Class and namespace merging
///
/// Classes can merge with namespaces to add static properties/methods.
///
/// NOTE: Currently ignored - class-namespace merging is not fully implemented.
/// The merging doesn't correctly handle type checking for merged static members.
#[test]
fn test_class_namespace_merging() {
    use crate::parser::ParserState;

    let source = r#"
class Album {
    title: string;
    constructor(title: string) {
        this.title = title;
    }
}

namespace Album {
    export interface Track {
        name: string;
        duration: number;
    }
    export function create(title: string): Album {
        return new Album(title);
    }
}

// Use class as type and constructor
const album: Album = new Album("Best Of");

// Use namespace members
const track: Album.Track = { name: "Song 1", duration: 180 };
const created: Album = Album.create("New Album");
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
        println!("=== Class Namespace Merging Diagnostics ===");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] {}", diag.start, diag.message_text);
        }
    }

    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Class and namespace merging should work: {:?}",
        checker.ctx.diagnostics
    );
}

/// TS Unsoundness #44: Enum and namespace merging
///
/// Enums can merge with namespaces to add helper functions.
///
/// EXPECTED FAILURE: Enum member access on the enum type is not
/// yet implemented. Currently expects 4 errors.
#[test]
fn test_enum_namespace_merging() {
    use crate::parser::ParserState;

    let source = r#"
enum Direction {
    Up = 1,
    Down = 2,
    Left = 3,
    Right = 4
}

namespace Direction {
    export function isVertical(dir: Direction): boolean {
        return dir === Direction.Up || dir === Direction.Down;
    }
}

// Use enum values
const dir: Direction = Direction.Up;

// Use namespace function
const vertical: boolean = Direction.isVertical(Direction.Up);
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

    // Enum member access now works! Changed from expecting 4 errors to 0 errors.
    if error_count != 0 {
        println!("=== Enum Namespace Merging Diagnostics ===");
        println!("Expected 0 errors (enum member access working), got {error_count}");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] {}", diag.start, diag.message_text);
        }
    }

    assert_eq!(
        error_count, 0,
        "Expected 0 errors (enum member access working): {:?}",
        checker.ctx.diagnostics
    );
}

/// TS Unsoundness #2: Function Bivariance - Methods are bivariant
///
/// Methods defined using method shorthand syntax are always bivariant,
/// meaning they accept both narrower AND wider argument types.
/// This allows common patterns like event handlers to work.
///
/// EXPECTED FAILURE: Method bivariance is not yet implemented. Methods are
/// currently checked with strictFunctionTypes semantics. Once method bivariance
/// is implemented, change to expect 0 errors.
#[test]
fn test_method_bivariance_wider_argument() {
    use crate::parser::ParserState;

    // Animal is wider than Dog
    // A method handler(dog: Dog) should be assignable to handler(animal: Animal)
    // because methods are bivariant
    let source = r#"
interface Animal { name: string }
interface Dog extends Animal { breed: string }

interface HandlerWithAnimal {
    handle(animal: Animal): void;
}

interface HandlerWithDog {
    handle(dog: Dog): void;
}

// Method bivariance: handler with narrower param type can be assigned to wider
// This is unsound but intentionally allowed
declare const dogHandler: HandlerWithDog;
const animalHandler: HandlerWithAnimal = dogHandler;
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

    // Currently expects 1 error: method bivariance not implemented
    // Once method bivariance works, change to expect 0 errors
    if error_count != 1 {
        println!("=== Method Bivariance Wider Arg Diagnostics ===");
        println!("Expected 1 error (method bivariance not implemented), got {error_count}");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] {}", diag.start, diag.message_text);
        }
    }

    assert_eq!(
        error_count, 0,
        "Expected 0 errors after method bivariance implementation: {:?}",
        checker.ctx.diagnostics
    );
}

/// TS Unsoundness #2: Function Bivariance - Methods accept narrower too
///
/// Due to method bivariance, a method with WIDER argument type
/// is also assignable to one with NARROWER argument type.
/// This is the contravariant direction which should work even without bivariance.
///
/// EXPECTED FAILURE: Interface inheritance (Dog extends Animal) is not correctly
/// resolved during parameter contravariance checks. The solver doesn't recognize
/// that Animal (wider) params can satisfy Dog (narrower) param requirements.
/// Once interface inheritance is properly handled, expect 0 errors.
#[test]
fn test_method_bivariance_narrower_argument() {
    use crate::parser::ParserState;

    let source = r#"
interface Animal { name: string }
interface Dog extends Animal { breed: string }

interface HandlerWithAnimal {
    handle(animal: Animal): void;
}

interface HandlerWithDog {
    handle(dog: Dog): void;
}

// Contravariant direction: wider param -> narrower param target
// This should work even with strictFunctionTypes
declare const animalHandler: HandlerWithAnimal;
const dogHandler: HandlerWithDog = animalHandler;
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

    // Currently expects 1 error: interface inheritance not correctly resolved
    // Once interface extends is properly handled, expect 0 errors
    if error_count != 1 {
        println!("=== Method Bivariance Narrower Arg Diagnostics ===");
        println!("Expected 1 error (interface inheritance not resolved), got {error_count}");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] {}", diag.start, diag.message_text);
        }
    }

    assert_eq!(
        error_count, 0,
        "Expected 0 errors for contravariant assignment (method bivariance makes this work): {:?}",
        checker.ctx.diagnostics
    );
}

/// TS Unsoundness #2: Function Bivariance - Function properties are contravariant
///
/// Unlike methods, function properties (arrow function syntax) are checked
/// contravariantly under strictFunctionTypes. A function with wider parameter
/// can be assigned to one with narrower parameter, but NOT vice versa.
///
/// EXPECTED FAILURE: Interface inheritance (Dog extends Animal) is not correctly
/// resolved during parameter contravariance checks. Once interface extends is
/// properly handled, expect 0 errors.
#[test]
fn test_function_property_contravariance() {
    use crate::parser::ParserState;

    let source = r#"
interface Animal { name: string }
interface Dog extends Animal { breed: string }

interface HandlerWithAnimalProp {
    handle: (animal: Animal) => void;
}

interface HandlerWithDogProp {
    handle: (dog: Dog) => void;
}

// Function property: wider param -> narrower is allowed (contravariance)
declare const animalHandler: HandlerWithAnimalProp;
const dogHandler: HandlerWithDogProp = animalHandler;
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

    // Interface extends is now properly handled - expect 0 errors
    if error_count != 0 {
        println!("=== Function Property Contravariance Diagnostics ===");
        println!("Expected 0 errors (interface inheritance fixed), got {error_count}");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] {}", diag.start, diag.message_text);
        }
    }

    assert_eq!(
        error_count, 0,
        "Expected 0 errors (interface extends now works, contravariance allows wider param): {:?}",
        checker.ctx.diagnostics
    );
}

/// TS Unsoundness #2: Function Bivariance - Function property rejects unsound direction
///
/// With strictFunctionTypes, function properties reject the unsound
/// covariant direction (narrower param -> wider param).
#[test]
fn test_function_property_rejects_covariant() {
    use crate::parser::ParserState;

    let source = r#"
interface Animal { name: string }
interface Dog extends Animal { breed: string }

interface HandlerWithAnimalProp {
    handle: (animal: Animal) => void;
}

interface HandlerWithDogProp {
    handle: (dog: Dog) => void;
}

// Function property: narrower param -> wider should be REJECTED
// This would be unsound and strictFunctionTypes catches it
declare const dogHandler: HandlerWithDogProp;
const animalHandler: HandlerWithAnimalProp = dogHandler;
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
        println!("=== Function Property Covariant Rejection Diagnostics ===");
        println!("Expected 1 error, got {error_count}");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] {}", diag.start, diag.message_text);
        }
    }

    // strictFunctionTypes should reject the unsound direction (1 error)
    assert_eq!(
        error_count, 1,
        "Function property should reject narrower->wider param assignment: {:?}",
        checker.ctx.diagnostics
    );
}

/// TS Unsoundness #2: Function Bivariance - Event handler pattern
///
/// The classic use case: event handlers with specific event types
/// must be assignable to generic event handlers.
///
/// This test verifies that method bivariance is working correctly, allowing
/// a `MouseEvent` handler to be passed to a function expecting an Event handler.
/// This relies on methods being bivariant (not contravariant) in TypeScript.
#[test]
fn test_method_bivariance_event_handler_pattern() {
    use crate::parser::ParserState;

    let source = r#"
declare const console: { log: (...args: any[]) => void };

interface Event { type: string }
interface MouseEvent extends Event { x: number; y: number }

interface Element {
    addEventListener(handler: (e: Event) => void): void;
}

// Should be able to pass a MouseEvent handler to addEventListener
// This relies on method bivariance
function handleMouse(e: MouseEvent): void {
    const _ = e.x + e.y; // Use e.x and e.y without console.log
}

declare const elem: Element;
elem.addEventListener(handleMouse);
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

    // Method bivariance is implemented! This test now passes with 0 errors.
    // The event handler pattern relies on method bivariance to allow passing
    // a MouseEvent handler to a function expecting an Event handler.
    if error_count != 0 {
        println!("=== Event Handler Pattern Diagnostics ===");
        println!("Expected 0 errors (method bivariance implemented), got {error_count}");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] {}", diag.start, diag.message_text);
        }
    }

    assert_eq!(
        error_count, 0,
        "Expected 0 errors - method bivariance allows event handler pattern: {:?}",
        checker.ctx.diagnostics
    );
}

/// TS Unsoundness #2: Function Bivariance - Callback in method parameter
///
/// When a callback is passed as a method parameter, the callback itself
/// benefits from method bivariance rules.
///
#[test]
fn test_callback_method_parameter_bivariance() {
    use crate::parser::ParserState;

    let source = r#"
interface Animal { name: string }
interface Dog extends Animal { breed: string }

interface Processor {
    process(items: Animal[], callback: (item: Animal) => void): void;
}

function handleDog(dog: Dog): void {
    dog.breed;
}

declare const processor: Processor;
declare const dogs: Dog[];

// Passing a Dog[] to Animal[] is covariant (allowed by #3)
// Passing handleDog to callback is bivariant (should be allowed)
processor.process(dogs, handleDog);
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

    // Method bivariance now implemented - callback parameters benefit from bivariance
    if error_count != 0 {
        println!("=== Callback Method Parameter Diagnostics ===");
        println!("Expected 0 errors (method bivariance implemented), got {error_count}");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] {}", diag.start, diag.message_text);
        }
    }

    assert_eq!(
        error_count, 0,
        "Expected 0 errors - callback bivariance works: {:?}",
        checker.ctx.diagnostics
    );
}

/// TS Unsoundness #1: The "Any" Type - Any is assignable to everything
///
/// `any` acts as both Top (unknown) and Bottom (never). It is assignable
/// to everything and everything is assignable to it. This is the fundamental
/// escape hatch in TypeScript.
#[test]
fn test_any_type_assignable_to_specific() {
    use crate::parser::ParserState;

    let source = r#"
declare const anyVal: any;

// Any is assignable to any specific type
const str: string = anyVal;
const num: number = anyVal;
const bool: boolean = anyVal;
const obj: { x: number } = anyVal;
const fn: (x: string) => number = anyVal;
const arr: number[] = anyVal;
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
        println!("=== Any Assignable To Specific Diagnostics ===");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] {}", diag.start, diag.message_text);
        }
    }

    // Any should be assignable to any specific type (0 errors)
    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Any should be assignable to all specific types: {:?}",
        checker.ctx.diagnostics
    );
}

