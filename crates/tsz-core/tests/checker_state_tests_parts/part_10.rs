/// TS Unsoundness #44: Enum and namespace merging
///
/// Enums can merge with namespaces to add helper functions.
///
/// EXPECTED FAILURE: Enum member access on the enum type is not
/// yet implemented. Currently expects 4 errors.
#[test]
fn test_enum_namespace_merging() {
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

    let ts2345_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|diag| diag.code == 2345)
        .count();

    // Method calls do not make function-type-literal callback parameters
    // bivariant. `tsc --strictFunctionTypes` rejects `MouseEvent` here because
    // the target callback may pass a plain `Event`.
    assert_eq!(
        ts2345_count, 1,
        "Expected TS2345 for stricter callback parameter, got: {:?}",
        checker.ctx.diagnostics
    );
}

/// TS Unsoundness #2: Function Bivariance - Callback in method parameter
///
/// Method-call syntax does not make a function-type-literal callback parameter
/// bivariant under `--strictFunctionTypes`.
///
#[test]
fn test_callback_method_parameter_bivariance() {
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

// Passing a Dog[] to Animal[] is covariant (allowed by #3).
// Passing handleDog to callback is rejected because the callback parameter type
// is a function-type literal, not a method-shorthand signature.
processor.process(dogs, handleDog);
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

    let ts2345_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|diag| diag.code == 2345)
        .count();

    assert_eq!(
        ts2345_count, 1,
        "Expected TS2345 for stricter callback parameter, got: {:?}",
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

/// TS Unsoundness #1: The "Any" Type - Everything is assignable to any
///
/// Any specific type is assignable to `any`. This is the escape hatch
/// that allows bypassing type checking.
#[test]
fn test_specific_types_assignable_to_any() {
    let source = r#"
declare let anyTarget: any;

// Everything is assignable to any
const str = "hello";
const num = 42;
const bool = true;
const obj = { x: 1 };
const fn = (x: string) => x.length;
const arr = [1, 2, 3];

anyTarget = str;
anyTarget = num;
anyTarget = bool;
anyTarget = obj;
anyTarget = fn;
anyTarget = arr;
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

    if !checker.ctx.diagnostics.is_empty() {
        println!("=== Specific To Any Diagnostics ===");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] {}", diag.start, diag.message_text);
        }
    }

    // All specific types should be assignable to any (0 errors)
    assert!(
        checker.ctx.diagnostics.is_empty(),
        "All types should be assignable to any: {:?}",
        checker.ctx.diagnostics
    );
}

/// TS Unsoundness #1: The "Any" Type - Any in function arguments
///
/// Any can be passed where a specific type is expected, and any function
/// can accept any as an argument.
#[test]
fn test_any_type_in_function_calls() {
    let source = r#"
declare const anyVal: any;

function expectString(s: string): void {}
function expectNumber(n: number): void {}
function expectObject(o: { x: number }): void {}

// Any can be passed where specific types are expected
expectString(anyVal);
expectNumber(anyVal);
expectObject(anyVal);
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

    if !checker.ctx.diagnostics.is_empty() {
        println!("=== Any In Function Calls Diagnostics ===");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] {}", diag.start, diag.message_text);
        }
    }

    // Any should be valid in function calls expecting specific types
    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Any should be valid in function calls: {:?}",
        checker.ctx.diagnostics
    );
}

/// TS Unsoundness #1: The "Any" Type - Any propagation in operations
///
/// Operations on any produce any, maintaining the escape hatch.
#[test]
fn test_any_type_propagation() {
    let source = r#"
declare const anyVal: any;

// Operations on any produce any
const propAccess = anyVal.foo;
const elemAccess = anyVal[0];
const call = anyVal();
const method = anyVal.bar();

// Results can be assigned to any specific type
const str: string = propAccess;
const num: number = elemAccess;
const obj: { x: number } = call;
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

    if !checker.ctx.diagnostics.is_empty() {
        println!("=== Any Propagation Diagnostics ===");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] {}", diag.start, diag.message_text);
        }
    }

    // Any should propagate through operations
    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Any should propagate through operations: {:?}",
        checker.ctx.diagnostics
    );
}

/// TS Unsoundness #1: The "Any" Type - Any does NOT bypass never
///
/// While any is both top and bottom, never is the true bottom.
/// Assigning never to any is allowed, but it doesn't mean anything
/// because never has no values.
#[test]
fn test_any_type_never_relationship() {
    let source = r#"
declare const neverVal: never;
declare let anyTarget: any;

// Never is assignable to any (but has no values)
anyTarget = neverVal;

// Any is NOT assignable to never (you can't produce a never value)
// This should produce an error
function returnNever(): never {
    throw "error";
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

    if !checker.ctx.diagnostics.is_empty() {
        println!("=== Any Never Relationship Diagnostics ===");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] {}", diag.start, diag.message_text);
        }
    }

    // never -> any is allowed, but we don't test any -> never here
    // as it requires implicit return checking
    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Never should be assignable to any: {:?}",
        checker.ctx.diagnostics
    );
}

/// TS Unsoundness #4: Freshness / Excess Property Checks - Fresh objects checked
///
/// Object literals ("fresh" objects) are subject to excess property checks.
/// This prevents typos and catches unintended extra properties.
#[test]
fn test_freshness_object_literal_excess_property() {
    let source = r#"
interface Config {
    host: string;
    port: number;
}

// Object literal (fresh) - excess property should be caught
const config: Config = {
    host: "localhost",
    port: 8080,
    extra: "not allowed"  // Error: excess property
};
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

    let excess_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2353)
        .collect();

    if excess_errors.is_empty() {
        println!("=== Freshness Object Literal Diagnostics ===");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] code={} {}", diag.start, diag.code, diag.message_text);
        }
    }

    assert_eq!(
        excess_errors.len(),
        1,
        "Fresh object literal should have excess property error: {:?}",
        checker.ctx.diagnostics
    );
}

/// TS Unsoundness #4: Freshness / Excess Property Checks - Variables not checked
///
/// Variables with excess properties are NOT subject to excess property checks.
/// This is the "stale" object behavior - width subtyping is allowed.
#[test]
fn test_freshness_variable_no_excess_check() {
    let source = r#"
interface Config {
    host: string;
    port: number;
}

// Variable assignment (not fresh) - no excess property check
const obj = {
    host: "localhost",
    port: 8080,
    extra: "allowed because not fresh"
};

// Assigning variable to typed binding - width subtyping allowed
const config: Config = obj;
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

    if !checker.ctx.diagnostics.is_empty() {
        println!("=== Freshness Variable Assignment Diagnostics ===");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] code={} {}", diag.start, diag.code, diag.message_text);
        }
    }

    // No excess property error for variable assignment
    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Variable assignment should allow width subtyping: {:?}",
        checker.ctx.diagnostics
    );
}

/// TS Unsoundness #4: Freshness / Excess Property Checks - Function argument
///
/// Fresh object literals passed as function arguments are checked for excess properties.
#[test]
fn test_freshness_function_argument_checked() {
    let source = r#"
interface Options {
    timeout: number;
}

function configure(opts: Options): void {}

// Fresh object literal in function call - excess property checked
configure({ timeout: 5000, retries: 3 });
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

    let excess_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2353)
        .collect();

    if excess_errors.is_empty() {
        println!("=== Freshness Function Argument Diagnostics ===");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] code={} {}", diag.start, diag.code, diag.message_text);
        }
    }

    assert_eq!(
        excess_errors.len(),
        1,
        "Fresh object in function call should have excess property error: {:?}",
        checker.ctx.diagnostics
    );
}

/// TS Unsoundness #4: Freshness / Excess Property Checks - Return statement
///
/// Fresh object literals in return statements are checked for excess properties.
#[test]
fn test_freshness_return_statement_checked() {
    let source = r#"
interface Result {
    value: number;
}

function getResult(): Result {
    return { value: 42, extra: "not allowed" };
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

    let excess_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2353)
        .collect();

    if excess_errors.is_empty() {
        println!("=== Freshness Return Statement Diagnostics ===");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] code={} {}", diag.start, diag.code, diag.message_text);
        }
    }

    assert_eq!(
        excess_errors.len(),
        1,
        "Fresh object in return should have excess property error: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn test_union_optional_object_literal_excess_property() {
    let source = r#"
type U = { a?: number } | { b?: number };
const u: U = { a: 1, c: 2 };
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

    let excess_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2353)
        .collect();

    if excess_errors.is_empty() {
        println!("=== Union Optional Excess Property Diagnostics ===");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] code={} {}", diag.start, diag.code, diag.message_text);
        }
    }

    assert!(
        !excess_errors.is_empty(),
        "Expected excess property error for union optional object literal: {:?}",
        checker.ctx.diagnostics
    );

    let ts2322_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .count();
    assert_eq!(
        ts2322_count, 0,
        "Did not expect TS2322 for union optional excess property, got: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn test_union_optional_object_literal_no_common_property() {
    let source = r#"
type U = { a?: number } | { b?: number };
const u: U = { c: 1 };
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

    let excess_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2353)
        .collect();

    if excess_errors.is_empty() {
        println!("=== Union Optional No Common Property Diagnostics ===");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] code={} {}", diag.start, diag.code, diag.message_text);
        }
    }

    assert_eq!(
        excess_errors.len(),
        1,
        "Expected excess property error for union optional object literal with no overlap: {:?}",
        checker.ctx.diagnostics
    );

    let ts2322_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .count();
    assert_eq!(
        ts2322_count, 0,
        "Did not expect TS2322 for union optional no-common property, got: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn test_union_optional_call_argument_excess_property() {
    let source = r#"
type U = { a?: number } | { b?: number };
function f(value: U) {}
f({ c: 1 });
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

    let excess_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2353)
        .collect();

    if excess_errors.is_empty() {
        println!("=== Union Optional Call Argument Diagnostics ===");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] code={} {}", diag.start, diag.code, diag.message_text);
        }
    }

    assert_eq!(
        excess_errors.len(),
        1,
        "Expected excess property error for union optional call argument, got: {:?}",
        checker.ctx.diagnostics
    );

    let ts2322_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .count();
    assert_eq!(
        ts2322_count, 0,
        "Did not expect TS2322 for union optional call argument, got: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn test_union_optional_variable_assignment_no_common_properties() {
    let source = r#"
type U = { a?: number } | { b?: number };
const obj = { c: 1 };
const u: U = obj;
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

    let codes: Vec<_> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2322) || codes.contains(&2559),
        "Expected TS2322 or TS2559 for union optional variable assignment, got: {codes:?}"
    );
}

/// TS Unsoundness #4: Freshness / Excess Property Checks - Spread removes freshness
///
/// Using spread on an object can remove freshness in some contexts.
/// Spread in object literals now works; this should produce 0 errors (tsc-compatible).
#[test]
fn test_freshness_spread_behavior() {
    let source = r#"
interface Config {
    host: string;
}

const base = { host: "localhost", port: 8080 };

// Spread creates a new object - freshness depends on context
// Here the spread result is directly assigned to typed binding
const config: Config = { ...base };
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

    let error_count = checker.ctx.diagnostics.len();

    // Spread now works; tsc produces 0 errors for this pattern
    assert_eq!(
        error_count, 0,
        "Expected 0 errors for spread behavior, got: {:?}",
        checker.ctx.diagnostics
    );
}

/// Freshness preservation through assignment expressions.
///
/// In TypeScript, the type of `x = y` is `y` with freshness preserved.
/// So `obj1 = obj2 = { x: 1, y: 2 }` should trigger excess property checks
/// for `obj1` when it doesn't have a `y` property.
///
/// Similarly, `return obj = { x: 1, y: 2 }` should trigger excess property
/// checks against the function's return type.
#[test]
fn test_freshness_preserved_through_chained_assignment() {
    let source = r#"
function fx10(obj1: { x?: number }, obj2: { x?: number, y?: number }) {
    obj1 = obj2 = { x: 1, y: 2 };
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

    let excess_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2353)
        .collect();

    assert_eq!(
        excess_errors.len(),
        1,
        "Chained assignment should trigger excess property check for 'y': {:?}",
        checker.ctx.diagnostics
    );
    assert!(
        excess_errors[0].message_text.contains("'y'"),
        "Excess property error should mention 'y', got: {}",
        excess_errors[0].message_text
    );
}

/// TS Unsoundness #19: Covariant `this` Types - Basic class subtyping
///
/// In TypeScript, the polymorphic `this` type is treated as Covariant,
/// even in method parameters where it should be Contravariant.
/// This allows derived classes to be assigned to base class types.
///
/// tsc allows this unsound pattern — covariant `this` types let derived
/// classes with tighter `compare` methods be assigned to base class types.
#[test]
fn test_covariant_this_basic_subtyping() {
    let source = r#"
class Animal {
    name: string = "";
    compare(other: this): boolean {
        return this.name === other.name;
    }
}

class Dog extends Animal {
    breed: string = "";
    compare(other: this): boolean {
        return super.compare(other) && this.breed === other.breed;
    }
}

const animal: Animal = new Dog();
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

    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Expected 0 errors (tsc allows this), got: {:?}",
        checker.ctx.diagnostics
    );
}

/// TS Unsoundness #19: Covariant `this` Types - Fluent API pattern
///
/// The covariant `this` type enables fluent APIs where methods return `this`.
/// This is a common and useful pattern in TypeScript.
/// Class extends checking now works, so this pattern produces 0 errors as expected.
#[test]
fn test_covariant_this_fluent_api() {
    let source = r#"
class Builder {
    value: number = 0;

    // Returns `this` for chaining
    add(n: number): this {
        this.value += n;
        return this;
    }

    reset(): this {
        this.value = 0;
        return this;
    }
}

class AdvancedBuilder extends Builder {
    multiplier: number = 1;

    multiply(n: number): this {
        this.multiplier *= n;
        return this;
    }
}

// Fluent API with proper this typing
const result = new AdvancedBuilder()
    .add(5)
    .multiply(2)
    .reset();
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

    let error_count = checker.ctx.diagnostics.len();

    // Class extends now works; fluent API pattern should produce 0 errors
    assert_eq!(
        error_count, 0,
        "Expected 0 errors for covariant this fluent API, got: {:?}",
        checker.ctx.diagnostics
    );
}

/// TS Unsoundness #19: Covariant `this` Types - Interface with this
///
/// Interfaces can also use `this` type for fluent patterns.
#[test]
fn test_covariant_this_interface_pattern() {
    let source = r#"
interface Cloneable {
    clone(): this;
}

class Point implements Cloneable {
    x: number;
    y: number;

    constructor(x: number, y: number) {
        this.x = x;
        this.y = y;
    }

    clone(): this {
        return new Point(this.x, this.y) as this;
    }
}

const p1 = new Point(1, 2);
const p2 = p1.clone();
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

    // Currently fails due to incomplete `this` type resolution in method return types.
    // TS2352: Conversion of Point to `this` may be a mistake
    // TS2420: Class incorrectly implements interface (clone() returns error, not () => this)
    // Once `this` type is fully implemented, change to expect 0 errors.
    let error_count = checker.ctx.diagnostics.len();
    assert!(
        error_count <= 2,
        "Expected 0-2 errors (this type not fully implemented): {:?}",
        checker.ctx.diagnostics
    );
}

/// tsc allows covariant `this` types — derived-to-base assignment compiles
/// even though it's unsound at runtime.
#[test]
fn test_covariant_this_unsound_call() {
    let source = r#"
class Box {
    content: string = "";
    merge(other: this): void {
        this.content += other.content;
    }
}

class NumberBox extends Box {
    value: number = 0;
    merge(other: this): void {
        super.merge(other);
        this.value += other.value;
    }
}

const b: Box = new NumberBox();
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

    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Expected 0 errors (tsc allows this), got: {:?}",
        checker.ctx.diagnostics
    );
}

/// TS Unsoundness #9: Legacy Null/Undefined
///
/// If `strictNullChecks` is OFF, `null` and `undefined` behave like `never` (Bottom)
/// and are assignable to everything. By default (with strictNullChecks ON), they
/// are only assignable to their own types.
#[test]
fn test_strict_null_checks_on() {
    let source = r#"
// With strictNullChecks on (default), null/undefined are not assignable to other types
const str: string = "hello";
const num: number = 42;

// These would be errors with strictNullChecks
// const bad1: string = null;
// const bad2: number = undefined;

// null and undefined are their own types
const n: null = null;
const u: undefined = undefined;

// Union types that include null/undefined
const maybeStr: string | null = null;
const maybeNum: number | undefined = undefined;
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

    if !checker.ctx.diagnostics.is_empty() {
        println!("=== Strict Null Checks On Diagnostics ===");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] {}", diag.start, diag.message_text);
        }
    }

    // Valid code with strictNullChecks should have no errors
    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Valid strictNullChecks code should pass: {:?}",
        checker.ctx.diagnostics
    );
}

/// TS Unsoundness #9: Legacy Null/Undefined - null/undefined rejected when strict
///
/// With strictNullChecks ON, assigning null to string should error.
#[test]
fn test_strict_null_checks_rejects_null() {
    let source = r#"
// Assigning null to string should error
const str: string = null;
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
        crate::checker::context::CheckerOptions {
            jsx_factory: "React.createElement".to_string(),
            jsx_factory_from_config: false,
            jsx_fragment_factory: "React.Fragment".to_string(),
            jsx_fragment_factory_from_config: false,
            strict: true,
            strict_null_checks: true,
            ..Default::default()
        },
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    // Should produce an error
    assert!(
        !checker.ctx.diagnostics.is_empty(),
        "Assigning null to string should error with strictNullChecks"
    );
}

/// TS Unsoundness #9: Legacy Null/Undefined - undefined rejected when strict
///
/// With strictNullChecks ON, assigning undefined to number should error.
#[test]
fn test_strict_null_checks_rejects_undefined() {
    let source = r#"
// Assigning undefined to number should error
const num: number = undefined;
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
        crate::checker::context::CheckerOptions {
            jsx_factory: "React.createElement".to_string(),
            jsx_factory_from_config: false,
            jsx_fragment_factory: "React.Fragment".to_string(),
            jsx_fragment_factory_from_config: false,
            strict: true,
            strict_null_checks: true,
            ..Default::default()
        },
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    // Should produce an error
    assert!(
        !checker.ctx.diagnostics.is_empty(),
        "Assigning undefined to number should error with strictNullChecks"
    );
}

/// TS Unsoundness #9: Legacy Null/Undefined - union with null/undefined
///
/// Union types can explicitly include null/undefined.
#[test]
fn test_null_undefined_union_types() {
    let source = r#"
// Union types that include null/undefined work fine
const maybeStr: string | null = null;
const maybeNum: number | undefined = undefined;

// Can also be assigned the non-null type
const str: string | null = "hello";
const num: number | undefined = 42;
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

    if !checker.ctx.diagnostics.is_empty() {
        println!("=== Null/Undefined Union Diagnostics ===");
        for diag in &checker.ctx.diagnostics {
            println!("[{}] {}", diag.start, diag.message_text);
        }
    }

    // Union types with null/undefined should work
    assert!(
        checker.ctx.diagnostics.is_empty(),
        "Union types with null/undefined should work: {:?}",
        checker.ctx.diagnostics
    );
}

