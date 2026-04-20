// Tests for Checker - Type checker using `NodeArena` and Solver
//
// This module contains comprehensive type checking tests organized into categories:
// - Basic type checking (creation, intrinsic types, type interning)
// - Type compatibility and assignability
// - Excess property checking
// - Function overloads and call resolution
// - Generic types and type inference
// - Control flow analysis
// - Error diagnostics
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
fn test_class_constructor_without_new_emits_ts2348() {
    use crate::parser::ParserState;

    // Regression test for TS2348: calling class constructor without 'new'
    // When a class constructor is called without 'new', should emit TS2348
    // (Cannot invoke an expression whose type lacks a call signature)
    // instead of TS2769 (No overload matches this call)
    let code = r#"
namespace Tools {
    export class NullLogger { }
}

// Calling class constructor without 'new' - should emit TS2348
var logger = Tools.NullLogger();

// Another case with a class that has a constructor
class MyClass {
    constructor(x: string) { }
}

// Should also emit TS2348
var instance = MyClass();
"#;

    let mut parser = ParserState::new("test.ts".to_string(), code.to_string());
    let root = parser.parse_source_file();
    assert!(parser.get_diagnostics().is_empty());

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

    let ts2348_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2348)
        .collect();
    let ts2769_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2769)
        .collect();

    // Should have TS2348 errors
    assert!(
        ts2348_errors.len() >= 2,
        "Should emit TS2348 for class constructor without new, got {} TS2348 errors: {:?}",
        ts2348_errors.len(),
        ts2348_errors
            .iter()
            .map(|d| &d.message_text)
            .collect::<Vec<_>>()
    );

    // Should NOT have TS2769 errors
    assert!(
        ts2769_errors.is_empty(),
        "Should not emit TS2769 for class constructor without new, got {} TS2769 errors: {:?}",
        ts2769_errors.len(),
        ts2769_errors
            .iter()
            .map(|d| &d.message_text)
            .collect::<Vec<_>>()
    );

    // Verify the message contains helpful text
    let first_error_msg = &ts2348_errors[0].message_text;
    assert!(
        first_error_msg.contains("is not callable")
            || first_error_msg.contains("Did you mean to include 'new'"),
        "TS2348 message should mention 'is not callable' or 'Did you mean to include new', got: {first_error_msg}"
    );
}

#[test]
fn test_generic_control_flow_narrowing_property_access() {
    use crate::parser::ParserState;

    let source = r#"
function f1<T extends string | undefined>(y: { a: T }): string {
    if (y.a) {
        return y.a;
    }
    return "hello";
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena,
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    // Should have no TS2322 errors - after narrowing, y.a should be assignable to string
    let ts2322_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .count();

    // Property access narrowing now works! y.a should be narrowed from T to T & string
    assert_eq!(
        ts2322_count,
        0,
        "Expected no TS2322 errors for property access, got {}: {:?}",
        ts2322_count,
        checker
            .ctx
            .diagnostics
            .iter()
            .filter(|d| d.code == 2322)
            .map(|d| (&d.message_text, &d.start))
            .collect::<Vec<_>>()
    );
}

// =============================================================================
// TS2339 Specific Tests: Optional Chaining, Unions, Index Signatures
// =============================================================================

#[test]
fn test_ts2339_optional_chaining_no_error() {
    use crate::parser::ParserState;

    // Optional chaining (?.) should NOT emit TS2339 when property might not exist
    let source = r#"
interface A { a: string; }
interface B { b: number; }

function test(obj: A | B | null) {
    // With optional chaining, this should NOT produce TS2339
    const result = obj?.a;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena,
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let ts2339_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2339)
        .count();
    assert_eq!(
        ts2339_count,
        0,
        "Expected no TS2339 errors for optional chaining, got {}: {:?}",
        ts2339_count,
        checker
            .ctx
            .diagnostics
            .iter()
            .filter(|d| d.code == 2339)
            .map(|d| &d.message_text)
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_ts2339_union_all_members_need_property() {
    use crate::parser::ParserState;

    // For union types, property must exist on ALL non-nullable members
    let source = r#"
interface A { a: string; }
interface B { b: number; }

function test(obj: A | B) {
    // This SHOULD produce TS2339 because 'c' doesn't exist on either A or B
    const result = obj.c;
}

function test2(obj: A | B) {
    // This SHOULD produce TS2339 because 'a' doesn't exist on B
    const result = obj.a;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena,
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let ts2339_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2339)
        .collect();

    // Should have 2 TS2339 errors: one for obj.c, one for obj.a
    assert_eq!(
        ts2339_errors.len(),
        2,
        "Expected 2 TS2339 errors for union property access, got {}: {:?}",
        ts2339_errors.len(),
        ts2339_errors
            .iter()
            .map(|d| &d.message_text)
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_ts2339_private_accessor_in_closure() {
    use crate::parser::ParserState;

    // Test that private accessors are accessible from closures in the class
    let source = r#"
class C {
    private get #prop(): string { return ""; }
    private set #prop(value: string) { }

    private get #roProp(): string { return ""; }

    constructor(name: string) {
        // Private accessor access in closure - should work
        const fn = () => {
            this.#prop = "";
            console.log(this.#prop);
            console.log(this.#roProp);
        };
        fn();
    }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena,
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    let ts2339_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2339)
        .collect();

    // All accesses should work - they're all from within the class
    assert_eq!(
        ts2339_errors.len(),
        0,
        "Expected no TS2339 errors for private accessor access (including in closures), got {} - errors: {:?}",
        ts2339_errors.len(),
        ts2339_errors
            .iter()
            .map(|d| &d.message_text)
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_ts2339_static_private_accessor_access() {
    use crate::parser::ParserState;

    // Test that static private accessors are accessible through the class name
    let source = r#"
class C {
    static private get #prop(): string { return ""; }
    static private set #prop(value: string) { }

    static private get #roProp(): string { return ""; }

    constructor(name: string) {
        // Static private accessor access in closure - should work
        const fn = () => {
            C.#prop = "";
            console.log(C.#prop);
            console.log(C.#roProp);
        };
        fn();
    }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena,
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );

    checker.check_source_file(root);

    let ts2339_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2339)
        .collect();

    // All accesses should work - they're all from within the class
    assert_eq!(
        ts2339_errors.len(),
        0,
        "Expected no TS2339 errors for static private accessor access (including in closures), got {} - errors: {:?}",
        ts2339_errors.len(),
        ts2339_errors
            .iter()
            .map(|d| &d.message_text)
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_ts2339_union_shared_property_no_error() {
    use crate::parser::ParserState;

    // Property that exists on ALL union members should NOT produce TS2339
    let source = r#"
interface A { common: string; a: string; }
interface B { common: number; b: number; }

function test(obj: A | B) {
    // This should NOT produce TS2339 because 'common' exists on both A and B
    const result = obj.common;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena,
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let ts2339_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2339)
        .count();
    assert_eq!(
        ts2339_count,
        0,
        "Expected no TS2339 errors for shared union property, got {}: {:?}",
        ts2339_count,
        checker
            .ctx
            .diagnostics
            .iter()
            .filter(|d| d.code == 2339)
            .map(|d| &d.message_text)
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_ts2339_index_signature_allows_any_property() {
    use crate::parser::ParserState;

    // String/number index signatures should allow any property access
    let source = r#"
interface StringIndexed {
    [key: string]: number;
    a: number; // explicit property
}

interface NumberIndexed {
    [key: number]: string;
}

function test1(obj: StringIndexed) {
    // These should NOT produce TS2339 - index signature allows any string property
    const x = obj.anyProp;
    const y = obj.anotherProp;
    const z = obj.a; // explicit property
}

function test2(obj: NumberIndexed) {
    // This should NOT produce TS2339 - number index signature allows numeric access
    const x = obj[0];
    const y = obj[42];
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena,
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let ts2339_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2339)
        .count();
    assert_eq!(
        ts2339_count,
        0,
        "Expected no TS2339 errors for index signature access, got {}: {:?}",
        ts2339_count,
        checker
            .ctx
            .diagnostics
            .iter()
            .filter(|d| d.code == 2339)
            .map(|d| &d.message_text)
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_ts2339_no_index_signature_error() {
    use crate::parser::ParserState;

    // Without index signature, accessing non-existent property should produce TS2339
    let source = r#"
interface NoIndex {
    a: string;
}

function test(obj: NoIndex) {
    // This SHOULD produce TS2339 - property 'b' doesn't exist
    const result = obj.b;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena,
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let ts2339_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2339)
        .count();
    assert_eq!(
        ts2339_count,
        1,
        "Expected 1 TS2339 error for missing property without index signature, got {}: {:?}",
        ts2339_count,
        checker
            .ctx
            .diagnostics
            .iter()
            .filter(|d| d.code == 2339)
            .map(|d| &d.message_text)
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_ts2339_nullable_union_with_optional_chaining() {
    use crate::parser::ParserState;

    // Test union with null/undefined using optional chaining
    let source = r#"
interface A { a: string; }

function test(obj: A | null) {
    // With optional chaining, this should NOT produce TS2339
    const result = obj?.a;

    // Without optional chaining, this SHOULD produce TS2339 for non-null property access
    // (though it might produce a different error about possible null)
    const result2 = obj.a;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena,
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    // obj?.a should NOT produce TS2339
    let ts2339_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2339)
        .collect();

    // The optional chaining case should not have TS2339
    // obj.a might have other diagnostics but not TS2339 for property access
    assert!(
        ts2339_errors.is_empty(),
        "Expected no TS2339 errors, got {}: {:?}",
        ts2339_errors.len(),
        ts2339_errors
            .iter()
            .map(|d| &d.message_text)
            .collect::<Vec<_>>()
    );
}
