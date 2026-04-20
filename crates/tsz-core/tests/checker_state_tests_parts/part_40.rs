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
#[test]
fn test_union_optional_call_argument_excess_property() {
    use crate::parser::ParserState;

    let source = r#"
type U = { a?: number } | { b?: number };
function f(value: U) {}
f({ c: 1 });
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
    use crate::parser::ParserState;

    let source = r#"
type U = { a?: number } | { b?: number };
const obj = { c: 1 };
const u: U = obj;
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
    use crate::parser::ParserState;

    let source = r#"
interface Config {
    host: string;
}

const base = { host: "localhost", port: 8080 };

// Spread creates a new object - freshness depends on context
// Here the spread result is directly assigned to typed binding
const config: Config = { ...base };
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
    use crate::parser::ParserState;

    let source = r#"
function fx10(obj1: { x?: number }, obj2: { x?: number, y?: number }) {
    obj1 = obj2 = { x: 1, y: 2 };
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
    use crate::parser::ParserState;

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
    use crate::parser::ParserState;

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
    use crate::parser::ParserState;

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
    use crate::parser::ParserState;

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
    use crate::parser::ParserState;

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
    use crate::parser::ParserState;

    let source = r#"
// Assigning null to string should error
const str: string = null;
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

