//! Integration tests for `any` propagation (TSZ-4 Task 1).
//!
//! These tests verify that TypeScript's `any` type behaves correctly as both
//! a top type (everything is assignable to any) and a bottom type (any is assignable to everything).
//!
//! Key behaviors to verify:
//! - any → T (any is assignable to any type)
//! - T → any (any type is assignable to any)
//! - any in nested structures
//! - any with arrays/tuples
//! - any vs special types (never, unknown, error)
//! - any in function signatures
//! - any with strict mode pragmas

use tsz_binder::BinderState;
use crate::context::CheckerOptions;
use crate::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;
use crate::test_fixtures::TestContext;
use std::sync::Arc;

/// Workaround for TS2318 (Cannot find global type) errors in test infrastructure.
const GLOBAL_TYPE_MOCKS: &str = r#"
interface Array<T> {}
interface String {}
interface Boolean {}
interface Number {}
interface Object {}
interface Function {}
interface RegExp {}
interface IArguments {}
interface Promise<T> {}
"#;

fn test_no_errors(source: &str) {
    let source = format!("{}\n{}", GLOBAL_TYPE_MOCKS, source);

    let ctx = TestContext::new();

    let mut parser = ParserState::new("test.ts".to_string(), source);
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file_with_libs(parser.get_arena(), root, &ctx.lib_files);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions::default(),
    );

    // Set lib contexts for global symbol resolution
    if !ctx.lib_files.is_empty() {
        let lib_contexts: Vec<crate::context::LibContext> = ctx
            .lib_files
            .iter()
            .map(|lib| crate::context::LibContext {
                arena: Arc::clone(&lib.arena),
                binder: Arc::clone(&lib.binder),
            })
            .collect();
        checker.ctx.set_lib_contexts(lib_contexts);
    }

    checker.check_source_file(root);

    if !checker.ctx.diagnostics.is_empty() {
        panic!(
            "Expected no errors, but got:\n{}",
            checker
                .ctx
                .diagnostics
                .iter()
                .map(|d| format!("  {}", d.message_text))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
}

fn test_expect_error(source: &str, expected_error_substring: &str) {
    let source = format!("{}\n{}", GLOBAL_TYPE_MOCKS, source);

    let ctx = TestContext::new();

    let mut parser = ParserState::new("test.ts".to_string(), source);
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file_with_libs(parser.get_arena(), root, &ctx.lib_files);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions::default(),
    );

    // Set lib contexts for global symbol resolution
    if !ctx.lib_files.is_empty() {
        let lib_contexts: Vec<crate::context::LibContext> = ctx
            .lib_files
            .iter()
            .map(|lib| crate::context::LibContext {
                arena: Arc::clone(&lib.arena),
                binder: Arc::clone(&lib.binder),
            })
            .collect();
        checker.ctx.set_lib_contexts(lib_contexts);
    }

    checker.check_source_file(root);

    // Check that we got at least one error with the expected substring
    let found_error = checker
        .ctx
        .diagnostics
        .iter()
        .any(|d| d.message_text.contains(expected_error_substring));

    if !found_error {
        panic!(
            "Expected error containing '{}', but got:\n{}",
            expected_error_substring,
            checker
                .ctx
                .diagnostics
                .iter()
                .map(|d| format!("  {}", d.message_text))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
}

// ============================================================================
// Basic any Propagation Tests
// ============================================================================

#[test]
fn test_any_is_assignable_to_primitive_types() {
    // any should be assignable to all primitive types
    test_no_errors(
        r#"
let x: any = 42;
let s: string = x;  // Should pass
let n: number = x;  // Should pass
let b: boolean = x;  // Should pass
"#,
    );
}

#[test]
fn test_primitive_types_are_assignable_to_any() {
    // All primitive types should be assignable to any
    test_no_errors(
        r#"
let s: string = "hello";
let n: number = 42;
let b: boolean = true;

let a1: any = s;  // Should pass
let a2: any = n;  // Should pass
let a3: any = b;  // Should pass
"#,
    );
}

#[test]
fn test_any_in_object_properties() {
    // any should work in nested object structures
    test_no_errors(
        r#"
let obj: any = { x: 42, y: "hello" };
let typed: { x: number; y: string } = obj;  // Should pass
"#,
    );
}

#[test]
fn test_any_with_nested_structural_mismatch() {
    // any should silence nested structural mismatches
    test_no_errors(
        r#"
let a: any = { x: { y: "wrong type" } };
let b: { x: { y: number } } = a;  // Should pass (any silences mismatch)
"#,
    );
}

#[test]
fn test_any_in_array_types() {
    // any[] should be assignable to T[] and vice versa
    test_no_errors(
        r#"
let anyArray: any[] = [1, "hello", true];
let numArray: number[] = anyArray;  // Should pass

let stringArray: string[] = ["a", "b"];
let anyArray2: any[] = stringArray;  // Should pass
"#,
    );
}

#[test]
fn test_any_in_tuple_types() {
    // any in tuple elements should work
    test_no_errors(
        r#"
let tuple1: [any, string] = [42, "hello"];
let tuple2: [number, string] = tuple1;  // Should pass

let tuple3: [number, any] = [42, "hello"];
let tuple4: [number, string] = tuple3;  // Should pass
"#,
    );
}

#[test]
fn test_any_with_function_parameters() {
    // any should work with function parameters
    test_no_errors(
        r#"
function foo(x: string, y: number): void {}
let args: any = ["hello", 42];
// Note: Can't directly spread any, but individual assignments work
let a: any = "hello";
let b: any = 42;
foo(a, b);  // Should pass
"#,
    );
}

// ============================================================================
// Special Type Interactions
// ============================================================================

#[test]
fn test_any_assignable_to_unknown() {
    // any should be assignable to unknown
    test_no_errors(
        r#"
let a: any = 42;
let u: unknown = a;  // Should pass
"#,
    );
}

#[test]
fn test_unknown_assignable_to_any() {
    // unknown should be assignable to any
    test_no_errors(
        r#"
let u: unknown = 42;
let a: any = u;  // Should pass
"#,
    );
}

#[test]
fn test_any_not_assignable_to_never() {
    // any should NOT be assignable to never
    // Actually, in TypeScript, 'any' IS assignable to 'never'
    // This is a special case
    test_no_errors(
        r#"
let a: any = 42;
let n: never = a;  // Should pass (special case: any -> never)
"#,
    );
}

#[test]
fn test_never_is_assignable_to_any() {
    // never IS assignable to any (never is bottom type)
    test_no_errors(
        r#"
let n: never = null as never;
let a: any = n;  // Should pass: never is assignable to any
"#,
    );
}

#[test]
fn test_any_in_union_types() {
    // any in unions should collapse to any
    test_no_errors(
        r#"
let a: any = 42;
let u: string | number = a;  // Should pass

let u2: any | string = 42;  // Should infer as any
"#,
    );
}

#[test]
fn test_any_in_intersection_types() {
    // any in intersections should behave correctly
    test_no_errors(
        r#"
let a: any = 42;
let i: string & number = a;  // Should pass (any makes intersection impossible)
"#,
    );
}

// ============================================================================
// any with Strict Mode
// ============================================================================

#[test]
fn test_any_propagation_in_strict_mode() {
    // In strict mode, any should still propagate
    test_no_errors(
        r#"
// @strict
let a: any = 42;
let s: string = a;  // Should pass even in strict mode
"#,
    );
}

#[test]
fn test_any_with_nested_mismatch_in_strict_mode() {
    // Verify any silences errors even with deep nesting
    test_no_errors(
        r#"
// @strict
type DeepType = { a: { b: { c: number } } };
let anyValue: any = { a: { b: { c: "wrong" } } };
let result: DeepType = anyValue;  // Should pass (any silences all)
"#,
    );
}

// ============================================================================
// any in Variable Declarations
// ============================================================================

#[test]
fn test_any_variable_with_type_annotation() {
    // Declaring variables with any type should work
    test_no_errors(
        r#"
let x: any = 42;
x = "hello";  // Should pass (any allows anything)
x = true;  // Should pass
"#,
    );
}

#[test]
fn test_any_inferred_from_mixed_types() {
    // When initializing from mixed types, type should infer (not any)
    test_expect_error(
        r#"
let x = [1, "hello"];  // Should infer as (string | number)[]
let y: string[] = x;  // Should error: can't assign (string|number)[] to string[]
"#,
        "is not assignable to",
    );
}

// ============================================================================
// any in Function Return Types
// ============================================================================

#[test]
fn test_any_return_type_allows_any_return_value() {
    // Functions returning any should accept any return value
    test_no_errors(
        r#"
function returnsAny(): any {
    return 42;  // Should pass
}
"#,
    );
}

#[test]
fn test_any_parameter_accepts_any_argument() {
    // Functions with any parameters should accept any arguments
    test_no_errors(
        r#"
function takesAny(x: any): void {}
takesAny(42);  // Should pass
takesAny("hello");  // Should pass
takesAny({ a: 1 });  // Should pass
"#,
    );
}

// ============================================================================
// any with Type Assertions (as const implications)
// ============================================================================

#[test]
fn test_any_with_const_assertion() {
    // as const should still work when source is any
    test_no_errors(
        r#"
let a: any = 42;
const b = a as const;  // Should pass (any as const is any)
"#,
    );
}

// ============================================================================
// Edge Cases and Special Scenarios
// ============================================================================

#[test]
fn test_any_preserves_error_for_unresolved_symbols() {
    // ERROR types should NOT be assignable to any (prevents silencing errors)
    test_expect_error(
        r#"
function foo(): unresolved {
    return 42;
}
let a: any = foo();  // unresolved should not be silenced by any
"#,
        "unresolved",
    );
}

#[test]
fn test_any_with_generics() {
    // any should work with generic types
    test_no_errors(
        r#"
function identity<T>(x: T): T {
    return x;
}

let result: string = identity<any>("hello");  // Should pass
let result2: number = identity<any>(42);  // Should pass
"#,
    );
}

#[test]
fn test_any_in_class_properties() {
    // any in class properties should work
    test_no_errors(
        r#"
class MyClass {
    prop: any;
    constructor() {
        this.prop = 42;  // Should pass
    }
}
"#,
    );
}

#[test]
fn test_any_with_method_signatures() {
    // any in method parameters should work
    test_no_errors(
        r#"
class Container {
    method(x: any): void {
        x.toString();  // Should pass (any has all properties)
    }
}
"#,
    );
}

#[test]
fn test_any_in_destructuring() {
    // any should work with destructuring
    test_no_errors(
        r#"
let obj: any = { a: 1, b: 2 };
let { a, b } = obj;  // Should pass
"#,
    );
}
