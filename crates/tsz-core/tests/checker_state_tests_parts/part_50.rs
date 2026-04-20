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
fn test_ts2695_comma_operator_edge_cases() {
    use crate::checker::diagnostics::diagnostic_codes;
    use crate::parser::ParserState;

    let source = r#"
declare function eval(input: string): any;
let a = 1;
let b = 2;
const obj = { method() {} };

a + b, b;
!a, b;
a ? b : 3, b;
a!, b;
typeof a, b;
`template`, b;

void a, b;
(a as any), b;
(0, eval)("1");
(0, obj.method)();
(0, obj["method"])();
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

    let ts2695_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| {
            d.code
                == diagnostic_codes::LEFT_SIDE_OF_COMMA_OPERATOR_IS_UNUSED_AND_HAS_NO_SIDE_EFFECTS
        })
        .collect();

    assert_eq!(
        ts2695_errors.len(),
        6,
        "Expected six TS2695 errors, got: {:?}",
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>()
    );
    // Note: other diagnostics (e.g. TS1100 for eval in strict mode) may also be emitted.
    // We only verify the TS2695 count above.
}

#[test]
fn test_variadic_tuple_rest_param_no_ts2769() {
    use crate::parser::ParserState;

    // Regression test for TS2769 false positives with variadic tuple rest parameters
    // https://github.com/microsoft/TypeScript/issues/...
    // For signature: foo<T extends unknown[]>(x: number, ...args: [...T, number]): T
    // Call foo(1, 2) should infer T = [], not emit TS2769
    let source = r#"
        declare function foo3<T extends unknown[]>(x: number, ...args: [...T, number]): T;

        // These should all be valid calls (no TS2769)
        foo3(1, 2);  // T = [], args = [2]
        foo3(1, 'hello', true, 2);  // T = ['hello', true], args = ['hello', true, 2]

        function test<U extends unknown[]>(u: U) {
            foo3(1, ...u, 'hi', 2);  // Should work with spread
        }
    "#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
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

    let ts2769_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2769)
        .collect();
    assert!(
        ts2769_errors.is_empty(),
        "Should not emit TS2769 for variadic tuple rest parameters, got {} TS2769 errors: {:?}",
        ts2769_errors.len(),
        ts2769_errors
    );
}

#[test]
fn test_variadic_tuple_optional_tail_inference_no_ts2769() {
    use crate::parser::ParserState;

    let source = r#"
        declare function ft3<T extends unknown[]>(t: [...T]): T;
        declare function f20<T extends unknown[] = []>(args: [...T, number?]): T;
        declare function f22<T extends unknown[] = []>(args: [...T, number]): T;
        declare function f22<T extends unknown[] = []>(args: [...T]): T;

        ft3(['hello', 42]);
        f20(["foo", "bar"]);
        f20(["foo", 42]);

        function f21<U extends string[]>(args: [...U, number?]) {
            f20(args);
            f20(["foo", "bar"]);
            f20(["foo", 42]);
        }

        function f23<U extends string[]>(args: [...U, number]) {
            f22(args);
            f22(["foo", "bar"]);
            f22(["foo", 42]);
        }
    "#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
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

    let ts2769_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2769)
        .collect();
    assert!(
        ts2769_errors.is_empty(),
        "Should not emit TS2769 for optional variadic tuple tails, got {} TS2769 errors: {:?}",
        ts2769_errors.len(),
        ts2769_errors
    );
}

#[test]
fn test_recursive_mapped_types_no_crash() {
    use crate::parser::ParserState;

    // Regression test for recursive mapped type stack overflow
    // Tests that simple recursive mapped types don't cause infinite loops or crashes
    let code = r#"
// Direct recursion
type Recurse = {
    [K in keyof Recurse]: Recurse[K]
}

// Mutual recursion
type Recurse1 = {
    [K in keyof Recurse2]: Recurse2[K]
}

type Recurse2 = {
    [K in keyof Recurse1]: Recurse1[K]
}

// Generic recursive mapped type
type Circular<T> = {[P in keyof T]: Circular<T>};
type tup = [number, number];

declare var x: Circular<tup>;
"#;

    let mut parser = ParserState::new("test.ts".to_string(), code.to_string());
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

    // Should complete without crashing or hanging
    checker.check_source_file(root);

    // May have errors, but should not crash
    // The recursion guard should prevent infinite loops
    // If we get here without panicking, the test passed
    let _ = checker.ctx.diagnostics.len();
}

#[test]
fn test_recursive_mapped_property_access_no_crash() {
    use crate::parser::ParserState;

    // Regression test for recursive mapped type property access
    let code = r#"
type Transform<T> = { [K in keyof T]: Transform<T[K]> };

interface Product {
    users: string[];
}

declare var product: Transform<Product>;
product.users;
"#;

    let mut parser = ParserState::new("test.ts".to_string(), code.to_string());
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

    // Should complete without crashing or hanging
    checker.check_source_file(root);

    // If we get here without panicking, the test passed
    let _ = checker.ctx.diagnostics.len();
}
#[test]
fn test_object_destructuring_assignability() {
    use crate::parser::ParserState;

    let source = r#"
let obj: { x: number, y: string } = { x: 10, y: "hello" };

// Should trigger TS2322: Type 'number' is not assignable to type 'string'
let { x, y }: { x: string, y: string } = obj;
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

    let ts2322_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .collect();

    println!(
        "All diagnostics: {:?}",
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
    println!("TS2322 count: {}", ts2322_errors.len());

    assert!(
        !ts2322_errors.is_empty(),
        "Expected TS2322 error for object destructuring type mismatch, got: {:?}",
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_array_destructuring_assignability() {
    use crate::parser::ParserState;

    let source = r#"
let arr: [number, string] = [10, "hello"];

// Should trigger TS2322: Type 'string' is not assignable to type 'number'
let [a, b]: [number, number] = arr;
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

    let ts2322_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .collect();

    println!(
        "[ARRAY] All diagnostics: {:?}",
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
    println!("[ARRAY] TS2322 count: {}", ts2322_errors.len());

    assert!(
        !ts2322_errors.is_empty(),
        "Expected TS2322 error for array destructuring type mismatch, got: {:?}",
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_destructuring_with_default_values_assignability() {
    use crate::parser::ParserState;

    let source = r#"
let obj: { x?: number } = {};

// Should trigger TS2322: Type 'number' is not assignable to type 'string'
// (The default value type should be checked against the declared type)
let { x = 42 }: { x: string } = obj;
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

    let ts2322_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .collect();

    println!(
        "[DEFAULT] All diagnostics: {:?}",
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
    println!("[DEFAULT] TS2322 count: {}", ts2322_errors.len());

    assert!(
        !ts2322_errors.is_empty(),
        "Expected TS2322 error for destructuring default value type mismatch, got: {:?}",
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_nested_destructuring_assignability() {
    use crate::parser::ParserState;

    let source = r#"
let obj: { a: { b: number } } = { a: { b: 10 } };

// Should trigger TS2322 for nested property mismatch
let { a: { b } }: { a: { b: string } } = obj;
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

    let ts2322_errors: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .collect();

    println!(
        "[NESTED] All diagnostics: {:?}",
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
    println!("[NESTED] TS2322 count: {}", ts2322_errors.len());

    assert!(
        !ts2322_errors.is_empty(),
        "Expected TS2322 error for nested destructuring type mismatch, got: {:?}",
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_destructuring_binding_element_default_value_mismatch() {
    use crate::parser::ParserState;

    let source = r#"
// The default value 42 (number) should trigger TS2322: Type 'number' is not assignable to type 'string'
let obj: { x?: string } = {};
let { x = 42 }: { x: string } = obj;
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
        "[BINDING_DEFAULT] All diagnostics: {:?}",
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

    // This should find TS2322 for the default value 42 (number) not being assignable to string
    assert!(
        !ts2322_errors.is_empty(),
        "Expected TS2322 error for binding element default value type mismatch, got: {:?}",
        checker
            .ctx
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>()
    );
}
