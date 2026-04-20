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
fn test_binding_element_default_value_isolated_check() {
    use crate::parser::ParserState;

    let source = r#"
// The initializer {} is valid for { x?: number } (x is optional)
// But the default value "hello" (string) should NOT be assignable to number
// This should give TS2322: Type 'string' is not assignable to type 'number'
let { x = "hello" }: { x?: number } = {};
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

    println!(
        "[ISOLATED_DEFAULT] All diagnostics: {:?}",
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );

    let ts2322_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .collect();

    // EXPECTED: TS2322 for "hello" (string) not assignable to number
    // This test may currently fail if default values in binding elements aren't being checked
    assert!(
        !ts2322_errors.is_empty(),
        "Expected TS2322 error for binding element default value 'hello' (string) not assignable to number, got: {:?}",
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>()
    );
}

/// Test that recursive mapped types don't crash and circular type detection works
///
/// NOTE: Currently ignored - circular type alias detection in mapped types is not
/// fully implemented. The checker should detect circular type aliases and emit TS2456,
/// but this is not being detected correctly for recursive mapped types.
#[test]
fn test_recursive_mapped_type_no_crash_and_ts2456() {
    use crate::parser::ParserState;

    let source = r#"
// TS2456: Type alias 'DirectCircular' circularly references itself
type DirectCircular = DirectCircular;

// TS2456: Mutually circular type aliases
type MutualA = MutualB;
type MutualB = MutualA;

// Valid recursive mapped types (should NOT crash or error)
type Recurse = {
    [K in keyof Recurse]: Recurse[K]
}

type Recurse1 = {
    [K in keyof Recurse2]: Recurse2[K]
}

type Recurse2 = {
    [K in keyof Recurse1]: Recurse1[K]
}

// Property access on recursive mapped type (should not crash)
type Box<T> = { value: T };
type RecursiveBox = { [K in keyof Box<RecursiveBox>]: Box<RecursiveBox>[K] };

function test(r: RecursiveBox) {
    return r.value; // Should not crash
}

// Circular mapped type from #27881
export type Circular<T> = {[P in keyof T]: Circular<T>};
type tup = [number, number, number, number];

function foo(arg: Circular<tup>): tup {
  return arg;
}

// Deep recursive mapped type from #29442
type DeepMap<T extends unknown[], R> = {
  [K in keyof T]: T[K] extends unknown[] ? DeepMap<T[K], R> : R;
};

type tpl = [string, [string, [string]]];
type t1 = DeepMap<tpl, number>;
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

    // This should NOT crash even with recursive types
    checker.check_source_file(root);

    // Verify TS2456 is emitted for direct circular type alias
    let ts2456_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2456)
        .count();

    // We should have at least TS2456 errors for:
    // 1. DirectCircular
    // 2. MutualA
    // 3. MutualB
    // Note: Depending on implementation, we might get 2 (one per declaration) or 3
    assert!(
        ts2456_count >= 2,
        "Expected at least 2 TS2456 errors for circular type aliases, got {} - diagnostics: {:?}",
        ts2456_count,
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_type_parameter_in_function_body_no_ts2304() {
    use crate::parser::ParserState;

    let source = r#"
function identity<T>(x: T): T {
    const y: T = x;
    return y;
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

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&2304),
        "Should not report TS2304 for type parameter T in function body, got diagnostics: {:?}",
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_static_private_field_access_no_ts2339() {
    use crate::parser::ParserState;

    // Regression test for static private field access
    // Previously failed with TS2339 because static private members were excluded from constructor type
    let source = r#"
class C {
    static #x = 123;
    static {
        console.log(C.#x);
    }
    foo() {
        return C.#x;
    }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

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

    // Should have NO TS2339 errors for C.#x access
    let ts2339_count = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2339)
        .count();
    assert_eq!(
        ts2339_count,
        0,
        "Expected no TS2339 errors for static private field access, got {} - diagnostics: {:?}",
        ts2339_count,
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_static_private_accessor_access_no_ts2339() {
    use crate::parser::ParserState;

    // Regression test for static private accessor access
    let source = r#"
class A {
    static get #prop() { return ""; }
    static set #prop(param: string) { }

    static get #roProp() { return ""; }

    constructor(name: string) {
        A.#prop = "";
        console.log(A.#prop);
        console.log(A.#roProp);
    }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

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

    checker.check_source_file(root);

    // Filter out TS2540 for read-only property assignment (expected error)
    let ts2339_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2339)
        .collect();

    assert_eq!(
        ts2339_errors.len(),
        0,
        "Expected no TS2339 errors for static private accessor access, got {} - TS2339 diagnostics: {:?}",
        ts2339_errors.len(),
        ts2339_errors
            .iter()
            .map(|d| &d.message_text)
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_type_parameter_in_type_query() {
    use crate::parser::ParserState;

    let source = r#"
// Type parameters should be resolved in typeof type queries
function identity<T>(x: T): T {
    return x;
}

// typeof on type parameter should not error
type IdentityReturnType<T> = ReturnType<typeof identity<T>>;

// Type parameter in Extract with typeof
function extract<T>(x: Extract<T, typeof identity>): T {
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

    // Check that we don't have TS2304 for type parameter names (T, etc.)
    let ts2304_for_type_params: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2304)
        .filter(|d| d.message_text.contains("'T'") || d.message_text.contains("type parameter"))
        .map(|d| &d.message_text)
        .collect();

    assert!(
        ts2304_for_type_params.is_empty(),
        "Should not report TS2304 for type parameter T in type query. Found errors: {ts2304_for_type_params:?}"
    );
}

#[test]
fn test_constrained_type_parameter_in_types_no_ts2304() {
    use crate::parser::ParserState;

    let source = r#"
function f1<T extends string | undefined>(x: T, y: { a: T }, z: [T]): string {
    return "hello";
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

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
    use crate::parser::ParserState;

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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

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
    use crate::parser::ParserState;

    let source = r#"
function f1<T extends string | undefined>(x: T): string {
    if (x) {
        return x;
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

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

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
