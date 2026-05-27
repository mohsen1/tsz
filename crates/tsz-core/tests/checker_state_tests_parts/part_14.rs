#[test]
fn test_constrained_type_parameter_in_types_no_ts2304() {
    let source = r#"
function f1<T extends string | undefined>(x: T, y: { a: T }, z: [T]): string {
    return "hello";
}
"#;

    let (parser, root) = parse_test_source(source);

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
    let ts2304_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2304)
        .map(|d| &d.message_text)
        .collect();

    assert!(
        !codes.contains(&2304),
        "Should not report TS2304 for constrained type parameter T. Found errors: {ts2304_errors:?}"
    );
}

#[test]
fn test_self_referential_type_constraint_no_ts2304() {
    let source = r#"
interface Box<T> {
    item: T;
}

declare function unbox<T>(x: Box<T>): T;

function g1<T extends Box<T> | undefined>(x: T) {
    if (x !== undefined) {
        unbox(x);
    }
}
"#;

    let (parser, root) = parse_test_source(source);

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
    let ts2304_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2304)
        .map(|d| &d.message_text)
        .collect();

    assert!(
        !codes.contains(&2304),
        "Should not report TS2304 for self-referential type constraint T extends Box<T>. Found errors: {ts2304_errors:?}"
    );
}

#[test]
fn test_generic_control_flow_narrowing() {
    let source = r#"
function f1<T extends string | undefined>(x: T): string {
    if (x) {
        return x;
    }
    return "hello";
}
"#;

    let (parser, root) = parse_test_source(source);

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

    // Should have no TS2322 errors - after narrowing, x should be assignable to string
    let ts2322_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .count();
    assert_eq!(
        ts2322_count,
        0,
        "Expected no TS2322 errors, got {} - diagnostics: {:?}",
        ts2322_count,
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_closure_captured_private_accessor_debug() {
    // Test case matching exact failing conformance test scenario
    let source = r#"
class A2 {
    get #prop() { return ""; }
    set #prop(param: string) { }

    constructor() {
        console.log(this.#prop); // Direct - should work
        let a: A2 = this;
        a.#prop; // Same context - should work
        function foo() {
            a.#prop; // Closure captured - currently fails but shouldn't
        }
    }
}
"#;

    let (parser, root) = parse_test_source(source);

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

    // TS2339 = "Property 'X' does not exist on type 'Y'"
    let ts2339_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2339)
        .collect();

    assert!(
        ts2339_errors.is_empty(),
        "Expected no TS2339 error for private accessor via local variable, got errors: {:?}",
        ts2339_errors
            .iter()
            .map(|d| &d.message_text)
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_class_constructor_without_new_emits_ts2348() {
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

    let (parser, root) = parse_test_source(code);
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
    let source = r#"
function f1<T extends string | undefined>(y: { a: T }): string {
    if (y.a) {
        return y.a;
    }
    return "hello";
}
"#;

    let (parser, root) = parse_test_source(source);

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
    // Optional chaining (?.) should NOT emit TS2339 when property might not exist
    let source = r#"
interface A { a: string; }
interface B { b: number; }

function test(obj: A | B | null) {
    // With optional chaining, this should NOT produce TS2339
    const result = obj?.a;
}
"#;

    let (parser, root) = parse_test_source(source);

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

    let (parser, root) = parse_test_source(source);

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

    let (parser, root) = parse_test_source(source);

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

    let (parser, root) = parse_test_source(source);

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
    // Property that exists on ALL union members should NOT produce TS2339
    let source = r#"
interface A { common: string; a: string; }
interface B { common: number; b: number; }

function test(obj: A | B) {
    // This should NOT produce TS2339 because 'common' exists on both A and B
    const result = obj.common;
}
"#;

    let (parser, root) = parse_test_source(source);

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

    let (parser, root) = parse_test_source(source);

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

    let (parser, root) = parse_test_source(source);

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

    let (parser, root) = parse_test_source(source);

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

#[test]
fn test_ts2339_intersection_property_access() {
    // Test property access on intersection types
    let source = r#"
type A = { a: string };
type B = { b: number };
type AB = A & B;

function test(obj: AB) {
    // These should NOT produce TS2339 - intersection has both properties
    const x = obj.a;
    const y = obj.b;
}

function test2(obj: A & { c: boolean }) {
    // These should NOT produce TS2339
    const x = obj.a;
    const y = obj.c;
}
"#;

    let (parser, root) = parse_test_source(source);

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
        "Expected no TS2339 errors for intersection property access, got {}: {:?}",
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
fn test_overload_arg_count_exceeds_all_only_ts2554_not_ts2769() {
    // Regression test for overload calls where argument count exceeds ALL signatures
    // When all overloads fail due to argument count mismatch, should emit TS2554 only, not TS2769
    let code = r#"
declare function mixed(x: string): void;
declare function mixed(x: number, y: number): void;

// This call has 3 arguments, which exceeds both overloads (1 param and 2 params)
// Should emit TS2554 (argument count mismatch) only, not TS2769
mixed(42, 99, 100);
"#;

    let (parser, root) = parse_test_source(code);
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

    let ts2554_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2554)
        .collect();
    let ts2769_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2769)
        .collect();

    // Should have TS2554 (argument count mismatch)
    assert!(
        !ts2554_errors.is_empty(),
        "Should emit TS2554 for argument count mismatch when all overloads fail due to arg count"
    );

    // Should NOT have TS2769 (No overload matches)
    assert!(
        ts2769_errors.is_empty(),
        "Should not emit TS2769 when all overloads fail due to argument count mismatch, got {} TS2769 errors: {:?}",
        ts2769_errors.len(),
        ts2769_errors
            .iter()
            .map(|d| &d.message_text)
            .collect::<Vec<_>>()
    );

    // Verify TS2554 message
    let first_error_msg = &ts2554_errors[0].message_text;
    assert!(
        first_error_msg.contains("Expected") && first_error_msg.contains("arguments"),
        "TS2554 message should mention expected arguments, got: {first_error_msg}"
    );
}

#[test]
fn test_ts2555_expected_at_least_arguments() {
    // Test TS2554 vs TS2555: tsc uses TS2554 ("Expected N-M arguments") for
    // functions with optional params, and TS2555 ("Expected at least N") only
    // for rest params. This test verifies that behavior.

    // Case 1: Optional params → TS2554
    let code = r#"
function foo(a: number, b: string, c?: boolean): void {}

// Too few arguments - should emit TS2554 (not TS2555) because tsc uses
// TS2554 with range format for optional params
foo(1);
"#;

    let (parser, root) = parse_test_source(code);
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

    let ts2554_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2554)
        .collect();

    // Should have TS2554 (not TS2555) for optional params
    assert!(
        !ts2554_errors.is_empty(),
        "Should emit TS2554 for too few args with optional params, got diagnostics: {:?}",
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );

    // Should NOT have TS2555
    let ts2555_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2555)
        .collect();
    assert!(
        ts2555_errors.is_empty(),
        "Should NOT emit TS2555 for optional params (only for rest params), got: {:?}",
        ts2555_errors
            .iter()
            .map(|d| &d.message_text)
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_ts2554_expected_exact_arguments() {
    // Test TS2554: Expected N arguments, but got M.
    // This error should be emitted when a function has no optional parameters
    // and the wrong number of arguments are provided.
    let code = r#"
function bar(a: number, b: string): void {}

// Wrong number of arguments - should emit TS2554 (not TS2555)
bar(1);
"#;

    let (parser, root) = parse_test_source(code);
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

    let ts2554_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2554)
        .collect();

    // Should have TS2554 (exact count expected)
    assert!(
        !ts2554_errors.is_empty(),
        "Should emit TS2554 when wrong number of arguments for function without optional params, got diagnostics: {:?}",
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );

    // Verify TS2554 message format (should NOT say "at least")
    let first_error_msg = &ts2554_errors[0].message_text;
    assert!(
        first_error_msg.contains("Expected") && first_error_msg.contains("arguments"),
        "TS2554 message should mention expected arguments, got: {first_error_msg}"
    );
    assert!(
        !first_error_msg.contains("at least"),
        "TS2554 message should NOT say 'at least', got: {first_error_msg}"
    );
}

#[test]
fn test_ts2345_argument_type_mismatch() {
    // Test TS2345: Argument of type 'X' is not assignable to parameter of type 'Y'.
    let code = r#"
function baz(a: number): void {}

// Type mismatch - should emit TS2345
baz("hello");
"#;

    let (parser, root) = parse_test_source(code);
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

    let ts2345_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2345)
        .collect();

    // Should have TS2345 (argument type mismatch)
    assert!(
        !ts2345_errors.is_empty(),
        "Should emit TS2345 when argument type doesn't match parameter type, got diagnostics: {:?}",
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );

    // Verify TS2345 message format
    let first_error_msg = &ts2345_errors[0].message_text;
    assert!(
        first_error_msg.contains("not assignable") || first_error_msg.contains("Argument"),
        "TS2345 message should mention 'not assignable' or 'Argument', got: {first_error_msg}"
    );
}

#[test]
fn test_ts2366_arrow_function_missing_return() {
    // Test error 2366 for arrow functions with explicit return type
    let source = r#"
// Arrow function with number return type that can fall through
const missingReturn = (): number => {
    if (Math.random() > 0.5) {
        return 1;
    }
};

// Arrow function that returns on all paths - no error
const allPathsReturn = (flag: boolean): number => {
    if (flag) {
        return 1;
    }
    return 2;
};

// Arrow function with void return - no error
const voidReturn = (): void => {
    console.log("ok");
};

// Arrow function without return type annotation - no error
const noAnnotation = () => {
    return 1;
};
"#;

    let (parser, root) = parse_test_source(source);
    assert!(parser.get_diagnostics().is_empty());

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let opts = crate::checker::context::CheckerOptions {
        jsx_factory: "React.createElement".to_string(),
        jsx_fragment_factory: "React.Fragment".to_string(),
        strict_null_checks: true,
        ..Default::default()
    }; // TS2366 requires strictNullChecks
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        opts,
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();

    // Should have exactly 1 error: 2366 for missingReturn
    assert_eq!(
        codes.iter().filter(|&&c| c == 2366).count(),
        1,
        "Expected 1 TS2366 error for arrow function missing return, got: {codes:?}"
    );
}

#[test]
fn test_ts2366_function_expression_missing_return() {
    // Test error 2366 for function expressions with explicit return type
    let source = r#"
// Function expression with string return type that can fall through
const missingReturn = function(): string {
    if (Math.random() > 0.5) {
        return "yes";
    }
};

// Function expression that returns on all paths - no error
const allPathsReturn = function(flag: boolean): string {
    if (flag) {
        return "yes";
    }
    return "no";
};

// Function expression without return type annotation - no error
const noAnnotation = function() {
    return 1;
};
"#;

    let (parser, root) = parse_test_source(source);
    assert!(parser.get_diagnostics().is_empty());

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let opts = crate::checker::context::CheckerOptions {
        jsx_factory: "React.createElement".to_string(),
        jsx_fragment_factory: "React.Fragment".to_string(),
        strict_null_checks: true,
        ..Default::default()
    }; // TS2366 requires strictNullChecks
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        opts,
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();

    // Should have exactly 1 error: 2366 for missingReturn
    assert_eq!(
        codes.iter().filter(|&&c| c == 2366).count(),
        1,
        "Expected 1 TS2366 error for function expression missing return, got: {codes:?}"
    );
}

#[test]
fn test_ts2366_nested_arrow_functions() {
    // Test error 2366 for nested arrow functions
    let source = r#"
function outer(): (x: number) => string {
    // Inner arrow function with return type that can fall through
    return (x: number): string => {
        if (x > 0) {
            return "positive";
        }
    };
}
"#;

    let (parser, root) = parse_test_source(source);
    assert!(parser.get_diagnostics().is_empty());

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let opts = crate::checker::context::CheckerOptions {
        jsx_factory: "React.createElement".to_string(),
        jsx_fragment_factory: "React.Fragment".to_string(),
        strict_null_checks: true,
        ..Default::default()
    }; // TS2366 requires strictNullChecks
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        opts,
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();

    // Should have exactly 1 error: 2366 for inner arrow function
    assert_eq!(
        codes.iter().filter(|&&c| c == 2366).count(),
        1,
        "Expected 1 TS2366 error for nested arrow function missing return, got: {codes:?}"
    );
}

#[test]
fn test_ts2366_arrow_function_switch_statement() {
    // Test error 2366 for arrow functions with switch statements
    let source = r#"
// Arrow function with switch missing default case
const switchNoDefault = (value: number): string => {
    switch (value) {
        case 1:
            return "one";
        case 2:
            return "two";
    }
};

// Arrow function with switch and default - no error
const switchWithDefault = (value: number): string => {
    switch (value) {
        case 1:
            return "one";
        default:
            return "other";
    }
};
"#;

    let (parser, root) = parse_test_source(source);
    assert!(parser.get_diagnostics().is_empty());

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let opts = crate::checker::context::CheckerOptions {
        jsx_factory: "React.createElement".to_string(),
        jsx_fragment_factory: "React.Fragment".to_string(),
        strict_null_checks: true,
        ..Default::default()
    }; // TS2366 requires strictNullChecks
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        opts,
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();

    // Should have exactly 1 error: 2366 for switchNoDefault
    assert_eq!(
        codes.iter().filter(|&&c| c == 2366).count(),
        1,
        "Expected 1 TS2366 error for arrow function with switch missing default, got: {codes:?}"
    );
}

#[test]
fn test_ts2366_arrow_function_switch_grouped_cases() {
    // Regression: grouped switch cases should not trigger TS2366 when all paths return.
    let source = r#"
const groupedSwitchReturns = (value: string | number): number => {
    switch (typeof value) {
        case "string":
        case "number":
            return 1;
        default:
            return 2;
    }
};
"#;

    let (parser, root) = parse_test_source(source);
    assert!(parser.get_diagnostics().is_empty());

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let opts = crate::checker::context::CheckerOptions {
        jsx_factory: "React.createElement".to_string(),
        jsx_fragment_factory: "React.Fragment".to_string(),
        strict_null_checks: true,
        ..Default::default()
    };
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        opts,
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert_eq!(
        codes.iter().filter(|&&c| c == 2366).count(),
        0,
        "Expected 0 TS2366 errors for grouped switch cases with full returns, got: {codes:?}"
    );
}

#[test]
fn test_ts2366_arrow_function_try_catch() {
    // Test error 2366 for arrow functions with try/catch
    let source = r#"
// Arrow function with try/catch - both branches can fall through
const tryCatchFallthrough = (): number => {
    try {
        if (Math.random() > 0.5) {
            return 1;
        }
    } catch (e) {
        console.log(e);
    }
};

// Arrow function with try/catch/finally - finally doesn't return but catch can fall through
const tryFinallyFallthrough = (): number => {
    try {
        if (Math.random() > 0.5) {
            return 1;
        }
    } catch (e) {
        console.log(e);
    } finally {
        console.log("cleanup");
    }
};
"#;

    let (parser, root) = parse_test_source(source);
    assert!(parser.get_diagnostics().is_empty());

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let opts = crate::checker::context::CheckerOptions {
        jsx_factory: "React.createElement".to_string(),
        jsx_fragment_factory: "React.Fragment".to_string(),
        strict_null_checks: true,
        ..Default::default()
    }; // TS2366 requires strictNullChecks
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        opts,
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();

    // Should have 2 errors: 2366 for both functions
    assert_eq!(
        codes.iter().filter(|&&c| c == 2366).count(),
        2,
        "Expected 2 TS2366 errors for arrow functions with try/catch fallthrough, got: {codes:?}"
    );
}

#[test]
fn test_ts7027_unreachable_code_after_return() {
    // Test TS7027 for unreachable code after return
    let source = r#"
function test1(): number {
    return 1;
    console.log("unreachable");  // Should error: TS7027
}

function test2(): void {
    return;
    const x = 5;  // Should error: TS7027
}

function test3(): string {
    if (true) {
        return "yes";
    }
    return "no";
    console.log("unreachable");  // Should error: TS7027
}
"#;

    let (parser, root) = parse_test_source(source);
    assert!(parser.get_diagnostics().is_empty());

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let opts = crate::checker::context::CheckerOptions {
        jsx_factory: "React.createElement".to_string(),
        jsx_fragment_factory: "React.Fragment".to_string(),
        allow_unreachable_code: Some(false),
        ..Default::default()
    };
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        opts,
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();

    // Should have 3 TS7027 errors
    assert_eq!(
        codes.iter().filter(|&&c| c == 7027).count(),
        3,
        "Expected 3 TS7027 errors for unreachable code after return, got: {codes:?}"
    );
}

#[test]
fn test_ts7027_unreachable_code_after_throw() {
    // Test TS7027 for unreachable code after throw
    let source = r#"
function test1(): never {
    throw new Error("error");
    console.log("unreachable");  // Should error: TS7027
}

function test2(): number {
    throw new Error("error");
    return 1;  // Should error: TS7027
}
"#;

    let (parser, root) = parse_test_source(source);
    assert!(parser.get_diagnostics().is_empty());

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let opts = crate::checker::context::CheckerOptions {
        jsx_factory: "React.createElement".to_string(),
        jsx_fragment_factory: "React.Fragment".to_string(),
        allow_unreachable_code: Some(false),
        ..Default::default()
    };
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        opts,
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();

    // Should have 2 TS7027 errors
    assert_eq!(
        codes.iter().filter(|&&c| c == 7027).count(),
        2,
        "Expected 2 TS7027 errors for unreachable code after throw, got: {codes:?}"
    );
}

#[test]
fn test_ts7027_unreachable_after_never_expression() {
    // Test TS7027 for unreachable code after never-type expressions
    let source = r#"
declare function fail(): never;

function test1(): number {
    fail();
    return 1;  // Should error: TS7027
}

function test2(): void {
    fail();
    console.log("unreachable");  // Should error: TS7027
}
"#;

    let (parser, root) = parse_test_source(source);
    assert!(parser.get_diagnostics().is_empty());

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let opts = crate::checker::context::CheckerOptions {
        jsx_factory: "React.createElement".to_string(),
        jsx_fragment_factory: "React.Fragment".to_string(),
        allow_unreachable_code: Some(false),
        ..Default::default()
    };
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        opts,
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();

    // Should have 2 TS7027 errors
    assert_eq!(
        codes.iter().filter(|&&c| c == 7027).count(),
        2,
        "Expected 2 TS7027 errors for unreachable code after never expression, got: {codes:?}"
    );
}

#[test]
fn test_ts2366_conditional_returns_all_paths() {
    // Test that functions with conditional returns that cover all paths don't error
    let source = r#"
function test1(flag: boolean): number {
    if (flag) {
        return 1;
    } else {
        return 2;
    }
}

function test2(x: number): string {
    if (x > 0) {
        return "positive";
    } else if (x < 0) {
        return "negative";
    } else {
        return "zero";
    }
}

function test3(x: number): number {
    switch (x) {
        case 1:
            return 1;
        case 2:
            return 2;
        default:
            return 0;
    }
}
"#;

    let (parser, root) = parse_test_source(source);
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

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();

    // Should have no TS2366 errors - all paths return
    assert_eq!(
        codes.iter().filter(|&&c| c == 2366).count(),
        0,
        "Expected 0 TS2366 errors when all paths return, got: {codes:?}"
    );
}

#[test]
fn test_ts2366_early_return() {
    // Test that early returns are handled correctly
    let source = r#"
function test1(x: number): number {
    if (x < 0) {
        return -1;
    }
    return x;  // OK - this is reached when x >= 0
}

function test2(x: number): number {
    if (x < 0) {
        return -1;
    }
    if (x > 0) {
        return 1;
    }
    return 0;  // OK - this is reached when x == 0
}
"#;

    let (parser, root) = parse_test_source(source);
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

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();

    // Should have no TS2366 errors - all paths return
    assert_eq!(
        codes.iter().filter(|&&c| c == 2366).count(),
        0,
        "Expected 0 TS2366 errors with early returns, got: {codes:?}"
    );
}

#[test]
fn test_ts2366_throw_as_exit() {
    // Test that throw statements are treated as exits
    let source = r#"
function test1(x: number): number {
    if (x < 0) {
        throw new Error("negative");
    }
    return x;
}

function test2(x: number): never {
    throw new Error("always throws");
}

function test3(x: number): number {
    if (x < 0) {
        throw new Error("negative");
    }
    if (x > 100) {
        throw new Error("too large");
    }
    return x;
}
"#;

    let (parser, root) = parse_test_source(source);
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

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();

    // Should have no TS2366 errors - throw exits the function
    assert_eq!(
        codes.iter().filter(|&&c| c == 2366).count(),
        0,
        "Expected 0 TS2366 errors when throw is used as exit, got: {codes:?}"
    );
}

