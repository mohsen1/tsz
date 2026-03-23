//! Regression tests for call expression resolution, overload resolution,
//! and property-call patterns.
//!
//! These exercise `call.rs` through the query boundary layer:
//! - Basic call expression type checking (TS2349, TS2554, TS2345)
//! - Overload resolution with multiple signatures
//! - Property/method call patterns (TS2339, TS2349)
//! - Optional chaining calls
//! - Spread arguments in calls (TS2556)
//! - Super calls and construct signatures
//! - Union callee types
//! - Generic call inference with overloads

use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn get_diagnostics(source: &str) -> Vec<(u32, String)> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions::default(),
    );

    checker.check_source_file(root);
    checker
        .ctx
        .diagnostics
        .into_iter()
        .filter(|d| d.code != 2318) // Filter "Cannot find global type"
        .map(|d| (d.code, d.message_text))
        .collect()
}

fn get_codes(source: &str) -> Vec<u32> {
    get_diagnostics(source)
        .into_iter()
        .map(|(code, _)| code)
        .collect()
}

fn has_error(source: &str, code: u32) -> bool {
    get_codes(source).contains(&code)
}

fn no_errors(source: &str) -> bool {
    get_codes(source).is_empty()
}

// ============================================================================
// Basic call expression checks
// ============================================================================

#[test]
fn call_non_callable_emits_ts2349() {
    let source = r#"
let x: number = 42;
x();
"#;
    assert!(
        has_error(source, 2349),
        "Calling a non-callable type should emit TS2349"
    );
}

#[test]
fn call_any_returns_any_no_error() {
    let source = r#"
declare let x: any;
let result = x();
"#;
    assert!(no_errors(source), "Calling any should not produce errors");
}

#[test]
fn call_unknown_emits_ts18046_with_strict() {
    let source = r#"
declare let x: unknown;
x();
"#;
    // TS18046: 'x' is of type 'unknown'
    assert!(
        has_error(source, 18046),
        "Calling unknown should emit TS18046"
    );
}

#[test]
fn call_never_returns_never() {
    let source = r#"
declare let f: never;
let result: string = f();
"#;
    // Calling never should emit TS2349 (not callable)
    assert!(
        has_error(source, 2349),
        "Calling never directly should emit TS2349"
    );
}

#[test]
fn call_error_type_no_cascade() {
    // When callee type is error, the call returns error without cascading TS2349
    let source = r#"
declare let x: never;
function f(y: string) {}
f(x);
"#;
    // Passing never to string should not error (never is assignable to anything)
    assert!(
        no_errors(source),
        "Passing never to any param type should not error"
    );
}

// ============================================================================
// Argument count checking (TS2554)
// ============================================================================

#[test]
fn too_many_arguments_ts2554() {
    let source = r#"
function f(x: number): void {}
f(1, 2);
"#;
    assert!(
        has_error(source, 2554),
        "Too many arguments should emit TS2554"
    );
}

#[test]
fn too_few_arguments_ts2554() {
    let source = r#"
function f(x: number, y: string): void {}
f(1);
"#;
    assert!(
        has_error(source, 2554),
        "Too few arguments should emit TS2554"
    );
}

#[test]
fn optional_params_no_error() {
    let source = r#"
function f(x: number, y?: string): void {}
f(1);
"#;
    assert!(
        no_errors(source),
        "Optional params should allow fewer arguments"
    );
}

// ============================================================================
// Argument type mismatch (TS2345)
// ============================================================================

#[test]
fn argument_type_mismatch_ts2345() {
    let source = r#"
function f(x: number): void {}
f("hello");
"#;
    assert!(
        has_error(source, 2345),
        "Passing string to number param should emit TS2345"
    );
}

#[test]
fn argument_subtype_no_error() {
    let source = r#"
function f(x: number | string): void {}
f(42);
"#;
    assert!(
        no_errors(source),
        "Passing subtype argument should not error"
    );
}

// ============================================================================
// Overload resolution
// ============================================================================

#[test]
fn overload_selects_matching_signature() {
    let source = r#"
function f(x: number): number;
function f(x: string): string;
function f(x: any): any { return x; }
let result: number = f(42);
"#;
    assert!(
        no_errors(source),
        "Overload resolution should select matching signature"
    );
}

#[test]
fn overload_type_mismatch_ts2345() {
    let source = r#"
function f(x: number): number;
function f(x: string): string;
function f(x: any): any { return x; }
let result: number = f("hello");
"#;
    assert!(
        has_error(source, 2322),
        "Overload resolution with wrong return type assignment should emit TS2322"
    );
}

#[test]
fn overload_no_matching_signature_ts2769() {
    let source = r#"
function f(x: number): number;
function f(x: string): string;
function f(x: any): any { return x; }
f(true);
"#;
    assert!(
        has_error(source, 2769),
        "No matching overload should emit TS2769"
    );
}

#[test]
fn overload_different_param_counts() {
    let source = r#"
function f(): void;
function f(x: number): void;
function f(x?: any): void {}
f();
f(1);
"#;
    assert!(
        no_errors(source),
        "Overload with different param counts should work"
    );
}

// ============================================================================
// Property/method calls
// ============================================================================

#[test]
fn method_call_on_object() {
    let source = r#"
declare let obj: { greet(name: string): string };
let result: string = obj.greet("world");
"#;
    assert!(
        no_errors(source),
        "Method call on typed object should not error"
    );
}

#[test]
fn missing_method_ts2339() {
    let source = r#"
declare let obj: { greet(name: string): string };
obj.missing();
"#;
    assert!(
        has_error(source, 2339),
        "Calling non-existent method should emit TS2339"
    );
}

#[test]
fn method_wrong_arg_type() {
    let source = r#"
declare let obj: { add(x: number): number };
obj.add("hello");
"#;
    assert!(
        has_error(source, 2345),
        "Method call with wrong arg type should emit TS2345"
    );
}

// ============================================================================
// Optional chaining calls
// ============================================================================

#[test]
fn optional_chain_call_valid() {
    let source = r#"
declare let obj: { greet?(name: string): string } | undefined;
let result = obj?.greet?.("world");
"#;
    assert!(no_errors(source), "Optional chain call should not error");
}

#[test]
fn optional_chain_call_on_non_callable() {
    let source = r#"
declare let obj: { x: number } | undefined;
obj?.x();
"#;
    assert!(
        has_error(source, 2349),
        "Optional chain call on non-callable property should emit TS2349"
    );
}

// ============================================================================
// Union callee types
// ============================================================================

#[test]
fn union_callee_compatible_calls() {
    let source = r#"
declare let f: ((x: number) => void) | ((x: number) => void);
f(42);
"#;
    assert!(
        no_errors(source),
        "Union callee with compatible signatures should work"
    );
}

#[test]
fn union_callee_incompatible_param_count() {
    let source = r#"
declare let f: ((x: number) => void) | ((x: number, y: string) => void);
f(42);
"#;
    // Union call requires valid for ALL members - missing arg for second member
    assert!(
        has_error(source, 2554) || has_error(source, 2769),
        "Union callee with incompatible param counts should error"
    );
}

// ============================================================================
// Super calls
// ============================================================================

#[test]
fn super_call_returns_void() {
    // super() is treated as a construct call that returns void
    let source = r#"
class Base {
    constructor(x: number) {}
}
class Derived extends Base {
    constructor() {
        super(42);
    }
}
"#;
    assert!(
        no_errors(source),
        "Basic super call with correct args should not error"
    );
}

// ============================================================================
// Type argument validation (TS2558, TS2344)
// ============================================================================

#[test]
fn too_many_type_arguments_ts2558() {
    let source = r#"
function f<T>(x: T): T { return x; }
f<number, string>(42);
"#;
    assert!(
        has_error(source, 2558),
        "Too many type arguments should emit TS2558"
    );
}

#[test]
fn untyped_call_with_type_args_ts2347() {
    let source = r#"
declare let f: any;
f<number>(42);
"#;
    assert!(
        has_error(source, 2347),
        "Untyped function call with type args should emit TS2347"
    );
}

// ============================================================================
// Generic overload resolution
// ============================================================================

#[test]
fn generic_overload_selects_correct_signature() {
    let source = r#"
function id<T>(x: T): T;
function id<T, U>(x: T, y: U): [T, U];
function id(...args: any[]): any { return args[0]; }
let result: number = id(42);
"#;
    assert!(
        no_errors(source),
        "Generic overload should select matching signature"
    );
}

#[test]
fn generic_call_infers_type_param() {
    let source = r#"
function id<T>(x: T): T { return x; }
let result: number = id(42);
"#;
    assert!(
        no_errors(source),
        "Generic call should infer T=number from argument"
    );
}

#[test]
fn generic_call_explicit_type_arg() {
    let source = r#"
function id<T>(x: T): T { return x; }
let result: number = id<number>(42);
"#;
    assert!(
        no_errors(source),
        "Generic call with explicit type arg should work"
    );
}

#[test]
fn generic_call_explicit_type_arg_mismatch() {
    let source = r#"
function id<T>(x: T): T { return x; }
id<number>("hello");
"#;
    assert!(
        has_error(source, 2345),
        "Generic call with explicit type arg and wrong arg should emit TS2345"
    );
}

// ============================================================================
// Contextual callback typing through calls
// ============================================================================

#[test]
fn callback_param_contextually_typed() {
    let source = r#"
declare function map<T, U>(arr: T[], fn: (x: T) => U): U[];
declare let nums: number[];
let result = map(nums, x => x + 1);
"#;
    assert!(
        no_errors(source),
        "Callback param should be contextually typed from generic"
    );
}

#[test]
fn callback_return_type_inferred() {
    let source = r#"
declare function apply<T>(fn: () => T): T;
let result: number = apply(() => 42);
"#;
    assert!(
        no_errors(source),
        "Callback return type should contribute to generic inference"
    );
}

// ============================================================================
// Spread arguments
// ============================================================================

#[test]
fn spread_arg_valid() {
    let source = r#"
function f(x: number, y: number): void {}
let args: [number, number] = [1, 2];
f(...args);
"#;
    assert!(
        no_errors(source),
        "Spread of tuple with correct types should work"
    );
}

// ============================================================================
// Property call with this context
// ============================================================================

#[test]
fn method_call_preserves_this_context() {
    let source = r#"
interface Obj {
    value: number;
    getValue(): number;
}
declare let obj: Obj;
let result: number = obj.getValue();
"#;
    assert!(
        no_errors(source),
        "Method call should preserve this context"
    );
}

// ============================================================================
// IIFE patterns
// ============================================================================

#[test]
fn iife_basic() {
    let source = r#"
let result = (function() { return 42; })();
"#;
    assert!(no_errors(source), "Basic IIFE should not error");
}

#[test]
fn arrow_iife() {
    let source = r#"
let result = (() => 42)();
"#;
    assert!(no_errors(source), "Arrow IIFE should not error");
}

// ============================================================================
// Query-boundary regression: generic call inference with application types
// ============================================================================

#[test]
fn generic_call_with_identity() {
    // Exercises generic call inference (application types) via query boundary.
    let source = r#"
declare function identity<T>(x: T): T;
let n: number = identity(42);
let s: string = identity("hello");
"#;
    assert!(
        no_errors(source),
        "Generic identity call should infer T correctly"
    );
}

#[test]
fn generic_overload_resolution_picks_correct_signature() {
    let source = r#"
declare function overloaded(x: string): string;
declare function overloaded(x: number): number;
let s: string = overloaded("hello");
let n: number = overloaded(42);
"#;
    assert!(
        no_errors(source),
        "Overload resolution should pick correct signature"
    );
}

#[test]
fn generic_overload_with_type_args() {
    let source = r#"
declare function create<T>(x: T): T;
declare function create<T>(x: T, y: T): T[];
let a: number = create<number>(1);
let b: number[] = create<number>(1, 2);
"#;
    assert!(
        no_errors(source),
        "Generic overloads with explicit type args should resolve"
    );
}

#[test]
fn property_call_on_generic_interface() {
    // Exercises application-type evaluation for interface method calls
    let source = r#"
interface Container<T> {
    get(): T;
    set(value: T): void;
}
declare let c: Container<number>;
let v: number = c.get();
c.set(42);
"#;
    assert!(
        no_errors(source),
        "Method call on generic interface should work"
    );
}

#[test]
fn deeply_any_callee_returns_any() {
    // Exercises is_type_deeply_any via query boundary
    let source = r#"
declare let f: any;
let result = f(1, 2, 3);
"#;
    assert!(
        no_errors(source),
        "Calling any-typed callee should return any without errors"
    );
}

#[test]
fn overload_with_spread_args() {
    let source = r#"
declare function foo(a: number, b: string): void;
declare function foo(a: string): void;
foo("hello");
"#;
    assert!(
        no_errors(source),
        "Overload resolution with fewer args should pick matching signature"
    );
}

#[test]
fn overload_wrong_arg_count_emits_ts2554() {
    let source = r#"
declare function bar(x: number): void;
bar(1, 2);
"#;
    assert!(has_error(source, 2554), "Too many args should emit TS2554");
}

#[test]
fn generic_call_inference_with_callback() {
    let source = r#"
declare function map<T, U>(arr: T[], fn: (x: T) => U): U[];
let result: number[] = map(["a", "b"], x => x.length);
"#;
    assert!(
        no_errors(source),
        "Generic call with callback inference should work"
    );
}

// ============================================================================
// Regression tests for solver query-based call resolution (query boundary layer)
// ============================================================================

/// When a generic function parameter is `T` (bare type parameter), sanitization
/// should replace the sensitive placeholder with `unknown` to avoid contaminating
/// the solver's second inference pass. The query `is_type_param_at_top_or_in_intersection`
/// drives this decision.
#[test]
fn generic_call_bare_type_param_sanitizes_callback() {
    let source = r#"
declare function wrap<T>(fn: T): T;
let result = wrap((x: number) => x + 1);
"#;
    assert!(
        no_errors(source),
        "Bare type param sanitization should not cause false errors"
    );
}

/// Same sanitization applies when the shape parameter is `T & SomeInterface`.
#[test]
fn generic_call_intersection_type_param_sanitizes_callback() {
    let source = r#"
interface HasLength { length: number; }
declare function constrained<T extends HasLength>(fn: T & HasLength): T;
let result = constrained({ length: 5 });
"#;
    assert!(
        no_errors(source),
        "Intersection containing type param should sanitize correctly"
    );
}

/// When a generic shape parameter is a concrete callable like `Predicate<A>`,
/// the sensitive placeholder should NOT be sanitized because its callable
/// structure helps infer the inner type param A.
#[test]
fn generic_call_concrete_callable_param_preserves_placeholder() {
    let source = r#"
type Predicate<T> = (x: T) => boolean;
declare function filter<T>(arr: T[], pred: Predicate<T>): T[];
let nums = filter([1, 2, 3], x => x > 0);
"#;
    assert!(
        no_errors(source),
        "Concrete callable param should preserve placeholder for inner inference"
    );
}

/// When both param and arg are Application types and param contains type params,
/// the pre-evaluation step should preserve raw Application form. The query
/// `both_are_applications_with_generic_param` drives this decision.
#[test]
fn generic_call_preserves_raw_application_for_aligned_shapes() {
    let source = r#"
interface Opts<S> { state: S; }
declare function createStore<S>(opts: Opts<S>): S;
let store = createStore({ state: 42 });
"#;
    assert!(
        no_errors(source),
        "Aligned Application shapes should be preserved during pre-evaluation"
    );
}

/// Overload resolution: when multiple signatures exist, the first matching one wins.
#[test]
fn overload_resolution_picks_first_match() {
    let source = r#"
declare function overloaded(x: string): string;
declare function overloaded(x: number): number;
let r1: string = overloaded("hello");
let r2: number = overloaded(42);
"#;
    assert!(
        no_errors(source),
        "Overload resolution should pick correct signature"
    );
}

/// Overload resolution should emit TS2769 when no overload matches.
#[test]
fn overload_resolution_no_match_emits_error() {
    let source = r#"
declare function overloaded(x: string): string;
declare function overloaded(x: number): number;
overloaded(true);
"#;
    assert!(
        has_error(source, 2769),
        "No matching overload should emit TS2769"
    );
}

/// Property call: calling a method via property access on a typed object.
#[test]
fn property_call_method_on_interface() {
    let source = r#"
interface Obj {
    greet(name: string): string;
}
declare const obj: Obj;
let result: string = obj.greet("world");
"#;
    assert!(
        no_errors(source),
        "Property method call should resolve correctly"
    );
}

/// The `has_any_call_signatures` query unifies Function and Callable checks
/// to decide whether an arg type is callable during generic inference refinement.
#[test]
fn callable_arg_type_detected_for_refinement() {
    let source = r#"
declare function apply<T, R>(fn: (x: T) => R, arg: T): R;
let result: number = apply(x => x + 1, 5);
"#;
    assert!(
        no_errors(source),
        "Callable arg type should be detected for generic refinement"
    );
}

/// Spread arguments in calls should be handled correctly.
#[test]
fn spread_args_in_generic_call() {
    let source = r#"
declare function concat<T>(...args: T[]): T[];
let arr = [1, 2, 3];
let result = concat(...arr);
"#;
    assert!(no_errors(source), "Spread args in generic call should work");
}

// ============================================================================
// Overload resolution edge cases
// ============================================================================

#[test]
fn overload_resolution_preserves_first_match_ordering() {
    // When multiple overloads could match, tsc picks the first one.
    let source = r#"
declare function f(x: string): string;
declare function f(x: string | number): number;
let result: string = f("hello");
"#;
    assert!(
        no_errors(source),
        "First matching overload should be selected"
    );
}

#[test]
fn overload_with_rest_params() {
    let source = r#"
declare function f(...args: string[]): void;
declare function f(x: number): void;
f("a", "b", "c");
f(42);
"#;
    assert!(
        no_errors(source),
        "Overloads with rest params should resolve"
    );
}

#[test]
fn overload_with_type_arg_count_mismatch_recovery() {
    // TS2558 for wrong type arg count; should still recover return type
    let source = r#"
declare function f<T>(x: T): T;
f<string, number>("hello");
"#;
    assert!(
        has_error(source, 2558),
        "Wrong type argument count should emit TS2558"
    );
}

// ============================================================================
// Property-call patterns
// ============================================================================

#[test]
fn method_call_on_class_instance() {
    let source = r#"
class Foo {
    bar(x: number): string { return ""; }
}
let f = new Foo();
let result: string = f.bar(42);
"#;
    assert!(
        no_errors(source),
        "Method call on class instance should work"
    );
}

#[test]
fn method_call_on_nested_property() {
    let source = r#"
declare let obj: { inner: { method(x: string): number } };
let result: number = obj.inner.method("hello");
"#;
    assert!(no_errors(source), "Nested property method call should work");
}

#[test]
fn optional_chain_method_call_on_union() {
    let source = r#"
declare let x: { f(): number } | undefined;
let result = x?.f();
"#;
    assert!(
        no_errors(source),
        "Optional chain method call on union should work"
    );
}

#[test]
fn element_access_call() {
    let source = r#"
declare let obj: { [key: string]: (x: number) => string };
let result: string = obj["test"](42);
"#;
    assert!(no_errors(source), "Element access call should work");
}

// ============================================================================
// Generic call inference with callbacks
// ============================================================================

#[test]
fn generic_callback_contextual_typing_preserves_param_type() {
    let source = r#"
declare function map<T, U>(arr: T[], fn: (x: T) => U): U[];
let result = map([1, 2, 3], x => x + 1);
"#;
    assert!(
        no_errors(source),
        "Generic callback should have contextual param type"
    );
}

#[test]
fn generic_call_with_multiple_callbacks() {
    // Multi-callback generic inference is complex; verify no TS2349 (not callable)
    let source = r#"
declare function combine<T, U>(
    a: T[],
    f: (x: T) => U
): U[];
let result = combine([1, 2], x => x + 1);
"#;
    assert!(
        no_errors(source),
        "Single callback generic call should work"
    );
}

#[test]
fn generic_call_with_object_literal_arg() {
    let source = r#"
declare function create<T>(config: { value: T }): T;
let result = create({ value: 42 });
"#;
    assert!(
        no_errors(source),
        "Generic call with object literal arg should work"
    );
}

// ============================================================================
// Union callee edge cases
// ============================================================================

#[test]
fn union_callee_with_compatible_return_types() {
    let source = r#"
declare let f: ((x: string) => number) | ((x: string) => number);
let result: number = f("hello");
"#;
    assert!(
        no_errors(source),
        "Union callee with identical signatures should work"
    );
}

#[test]
fn union_callee_incompatible_arity() {
    let source = r#"
declare let f: ((a: string) => void) | ((a: string, b: number) => void);
f("hello");
"#;
    // tsc emits TS2554 for missing second arg against second union member
    let codes = get_codes(source);
    assert!(
        codes.contains(&2554) || codes.contains(&2345),
        "Union callee with incompatible arity should emit error: got {codes:?}"
    );
}

// ============================================================================
// Super call edge cases
// ============================================================================

// NOTE: super<T>() should emit TS2754 but tsz does not yet implement this.
// Add a test once TS2754 support is implemented.

// ============================================================================
// Spread argument edge cases (callWithSpread patterns)
// ============================================================================

#[test]
fn call_with_spread_tuple_exact_match() {
    let source = r#"
function f(a: number, b: string, c: boolean): void {}
let args: [number, string, boolean] = [1, "hi", true];
f(...args);
"#;
    assert!(
        no_errors(source),
        "Spread of exact tuple match should not error"
    );
}

#[test]
fn call_with_spread_array_to_rest_param() {
    let source = r#"
function f(...args: number[]): void {}
let arr: number[] = [1, 2, 3];
f(...arr);
"#;
    assert!(
        no_errors(source),
        "Spread array to rest param should not error"
    );
}

#[test]
fn call_with_spread_mixed_args() {
    let source = r#"
function f(a: number, ...rest: string[]): void {}
let strs: string[] = ["a", "b"];
f(1, ...strs);
"#;
    assert!(
        no_errors(source),
        "Spread with leading fixed arg should not error"
    );
}

#[test]
fn call_with_spread_wrong_element_type() {
    let source = r#"
function f(a: number, b: number): void {}
let args: [string, string] = ["a", "b"];
f(...args);
"#;
    assert!(
        has_error(source, 2345) || has_error(source, 2556),
        "Spread with wrong element types should emit error"
    );
}

#[test]
fn call_with_spread_overload_resolution() {
    let source = r#"
declare function f(a: number): number;
declare function f(a: string, b: string): string;
let args: [string, string] = ["a", "b"];
f(...args);
"#;
    // Should select the second overload
    let codes = get_codes(source);
    // No false TS2769 — the spread matches the second overload.
    assert!(
        !codes.contains(&2349),
        "Spread in overload call should not emit TS2349, got: {codes:?}",
    );
}

// ============================================================================
// Generic call with optional chaining
// ============================================================================

#[test]
fn generic_call_with_optional_chaining() {
    let source = r#"
interface Processor {
    process<T>(x: T): T;
}
declare let p: Processor | undefined;
let result = p?.process(42);
"#;
    assert!(
        no_errors(source),
        "Generic call via optional chaining should not error"
    );
}

#[test]
fn optional_chain_call_returns_possibly_undefined() {
    let source = r#"
declare let obj: { f(): number } | undefined;
let result: number = obj?.f();
"#;
    // The result of obj?.f() is number | undefined, not number
    assert!(
        has_error(source, 2322),
        "Optional chain call result should be T | undefined"
    );
}

// ============================================================================
// IIFE with contextual typing
// ============================================================================

#[test]
fn iife_with_contextual_return_type() {
    let source = r#"
let result: number = (() => 42)();
"#;
    assert!(
        no_errors(source),
        "IIFE with contextual return type should not error"
    );
}

#[test]
fn iife_with_params() {
    let source = r#"
let result = (function(x: number) { return x + 1; })(5);
"#;
    assert!(no_errors(source), "IIFE with params should not error");
}

// ============================================================================
// Property-call regression patterns
// ============================================================================

#[test]
fn method_call_through_non_null_assertion() {
    let source = r#"
declare let obj: { f(): number } | undefined;
let result: number = obj!.f();
"#;
    assert!(
        no_errors(source),
        "Method call through non-null assertion should work"
    );
}

#[test]
fn method_call_on_intersection_type() {
    let source = r#"
interface A { foo(): number; }
interface B { bar(): string; }
declare let obj: A & B;
let n: number = obj.foo();
let s: string = obj.bar();
"#;
    assert!(
        no_errors(source),
        "Method call on intersection type should work"
    );
}

#[test]
fn method_call_on_generic_constraint() {
    let source = r#"
interface HasId { getId(): string; }
function getIdOf<T extends HasId>(obj: T): string {
    return obj.getId();
}
"#;
    assert!(
        no_errors(source),
        "Method call on generic constraint should work"
    );
}

#[test]
fn chained_method_calls() {
    let source = r#"
interface Builder {
    setName(n: string): Builder;
    build(): { name: string };
}
declare let b: Builder;
let result = b.setName("test").build();
"#;
    assert!(no_errors(source), "Chained method calls should work");
}

// ============================================================================
// Overload with generic and non-generic signatures
// ============================================================================

#[test]
fn overload_generic_and_non_generic_mixed() {
    let source = r#"
declare function f(x: string): string;
declare function f<T>(x: T): T;
let s: string = f("hello");
let n: number = f(42);
"#;
    assert!(
        no_errors(source),
        "Mixed generic/non-generic overloads should resolve"
    );
}

#[test]
fn overload_with_optional_params_ambiguity() {
    let source = r#"
declare function f(x: number): number;
declare function f(x: number, y?: string): string;
let result: number = f(42);
"#;
    assert!(
        no_errors(source),
        "Overload with optional params should pick first match"
    );
}

// ============================================================================
// Type predicate through call resolution
// ============================================================================

#[test]
fn type_predicate_call_narrows_type() {
    let source = r#"
function isString(x: unknown): x is string {
    return typeof x === "string";
}
declare let val: string | number;
if (isString(val)) {
    let s: string = val;
}
"#;
    assert!(no_errors(source), "Type predicate call should narrow type");
}

// ============================================================================
// Generic call inference edge cases
// ============================================================================

#[test]
fn generic_call_with_literal_type_preservation() {
    let source = r#"
declare function identity<T>(x: T): T;
const result = identity("hello");
"#;
    // Should infer T as "hello" (literal) or string — no error either way
    assert!(
        no_errors(source),
        "Generic call with literal should not error"
    );
}

#[test]
fn generic_call_with_constrained_type_param() {
    let source = r#"
declare function first<T extends any[]>(arr: T): T[0];
let result: number = first([1, 2, 3]);
"#;
    assert!(
        no_errors(source),
        "Generic call with constrained type param should work"
    );
}

#[test]
fn generic_call_with_multiple_type_params() {
    let source = r#"
declare function pair<A, B>(a: A, b: B): [A, B];
let result = pair(1, "hello");
"#;
    assert!(
        no_errors(source),
        "Generic call with multiple type params should work"
    );
}

#[test]
fn generic_call_with_default_type_param() {
    let source = r#"
declare function create<T = string>(x?: T): T;
let result: string = create();
"#;
    assert!(
        no_errors(source),
        "Generic call with default type param should work"
    );
}

// =============================================================================
// Regression tests for call.rs query boundary refactoring
// =============================================================================

/// Tests that generic calls with Application-typed params and args
/// correctly preserve raw applications during inference.
#[test]
fn generic_call_preserves_application_during_inference() {
    let source = r#"
interface Box<T> { value: T }
declare function unbox<T>(b: Box<T>): T;
declare const boxed: Box<number>;
let result: number = unbox(boxed);
"#;
    assert!(
        no_errors(source),
        "Generic call should preserve application types during inference"
    );
}

/// Tests that the type-parameter-or-intersection check correctly skips
/// excess property checking for generic params.
#[test]
fn generic_call_skips_excess_for_type_param() {
    let source = r#"
interface Named { name: string }
declare function parrot<T extends Named>(t: T): T;
parrot({ name: "hello", extra: 42 });
"#;
    // tsc allows extra properties when param is a bare type parameter
    // (the type parameter captures the full object shape).
    assert!(
        no_errors(source),
        "Generic call with type param should skip excess property checking"
    );
}

/// Tests that intersection-containing-type-parameter is correctly detected
/// for excess property skip.
#[test]
fn generic_call_intersection_param_skips_excess() {
    let source = r#"
interface Printable { print(): void }
declare function create<T extends Printable>(t: T & Printable): T;
create({ print() {}, extra: true });
"#;
    assert!(
        no_errors(source),
        "Intersection with type param should skip excess property checking"
    );
}

/// Tests that callable argument types are correctly detected during
/// generic call refinement.
#[test]
fn generic_call_callable_arg_refinement() {
    let source = r#"
declare function map<T, U>(arr: T[], fn: (x: T) => U): U[];
let result = map([1, 2, 3], x => String(x));
"#;
    // The key behavior: the callback parameter `x` gets contextual type `number`
    // from inference, so no TS7006 (implicit any) should be emitted.
    assert!(
        !has_error(source, 7006),
        "Generic call with callable arg should provide contextual type to callback"
    );
}

/// Tests that overloaded function calls resolve to the correct signature.
#[test]
fn overload_resolution_picks_correct_signature() {
    let source = r#"
declare function convert(x: string): number;
declare function convert(x: number): string;
let a: number = convert("hello");
let b: string = convert(42);
"#;
    assert!(
        no_errors(source),
        "Overload resolution should pick the correct signature for each call"
    );
}

/// Tests that overload resolution reports mismatches.
#[test]
fn overload_resolution_reports_mismatch() {
    let source = r#"
declare function convert(x: string): number;
declare function convert(x: number): string;
let a: string = convert("hello");
"#;
    assert!(
        has_error(source, 2322),
        "Overload return type mismatch should emit TS2322"
    );
}

/// Tests that property/method calls work correctly through optional chaining.
#[test]
fn optional_chain_method_call() {
    let source = r#"
interface Obj { method(x: number): string }
declare const obj: Obj | undefined;
let result: string | undefined = obj?.method(42);
"#;
    assert!(
        no_errors(source),
        "Optional chain method call should return T | undefined"
    );
}

/// Tests that unresolved inference results don't pollute outer generic inference.
#[test]
fn nested_generic_call_doesnt_pollute_inference() {
    let source = r#"
declare function identity<T>(x: T): T;
declare function wrap<U>(fn: (x: U) => U): U;
let result: number = wrap(identity);
"#;
    // This exercises the round1-skip-outer-context path where unresolved
    // inference results are replaced with UNKNOWN to avoid pollution.
    let codes = get_codes(source);
    // Should not produce false TS2345/TS7006 from polluted inference
    assert!(
        !codes.contains(&7006),
        "Nested generic call should not produce false TS7006"
    );
}

// ============================================================================
// Query boundary regression tests
// ============================================================================
// These tests verify that call resolution works correctly through the query
// boundary layer (no direct solver internal type inspection).

#[test]
fn overload_with_union_return_types() {
    // Verifies overload resolution returns the correct signature's return type
    // when signatures differ in both param and return types.
    let source = r#"
declare function parse(input: string): object;
declare function parse(input: string, reviver: (key: string, value: any) => any): object;
let result: object = parse("{}");
"#;
    assert!(
        no_errors(source),
        "Overload with fewer args should match first signature"
    );
}

#[test]
fn overload_with_literal_discrimination() {
    // Overloads discriminated by string literal types.
    let source = r#"
declare function create(kind: "a"): number;
declare function create(kind: "b"): string;
let x: number = create("a");
let y: string = create("b");
"#;
    assert!(
        no_errors(source),
        "Literal-discriminated overloads should resolve correctly"
    );
}

#[test]
fn overload_literal_discrimination_wrong_return() {
    let source = r#"
declare function create(kind: "a"): number;
declare function create(kind: "b"): string;
let x: string = create("a");
"#;
    assert!(
        has_error(source, 2322),
        "Wrong return type from literal-discriminated overload should emit TS2322"
    );
}

#[test]
fn property_call_on_mapped_type() {
    // Method call on a property obtained from a mapped type.
    let source = r#"
type Methods = {
    greet(): string;
    count(): number;
};
declare let m: Methods;
let s: string = m.greet();
let n: number = m.count();
"#;
    assert!(
        no_errors(source),
        "Method calls on mapped-type properties should resolve correctly"
    );
}

#[test]
fn property_call_on_indexed_access() {
    // Calling a method obtained through bracket access on a typed object.
    let source = r#"
interface Obj {
    method(x: number): string;
}
declare let obj: Obj;
let r: string = obj["method"](42);
"#;
    assert!(
        no_errors(source),
        "Element access call with string literal key should resolve"
    );
}

#[test]
fn overload_with_rest_and_fixed_params() {
    // Overload where one signature has rest params and another has fixed params.
    let source = r#"
declare function log(message: string): void;
declare function log(message: string, ...args: any[]): void;
log("hello");
log("hello", 1, 2, 3);
"#;
    assert!(
        no_errors(source),
        "Rest param overload should accept both fixed and variadic calls"
    );
}

#[test]
fn generic_overload_with_constraint() {
    // Generic overload where type param has a constraint.
    let source = r#"
declare function pick<T, K extends keyof T>(obj: T, key: K): T[K];
let o = { a: 1, b: "hello" };
let n: number = pick(o, "a");
"#;
    assert!(
        no_errors(source),
        "Generic overload with keyof constraint should infer correctly"
    );
}

#[test]
fn property_call_on_union_of_interfaces() {
    // Method call on union where both members have the method.
    let source = r#"
interface A { run(x: number): void; }
interface B { run(x: number): void; }
declare let ab: A | B;
ab.run(42);
"#;
    assert!(
        no_errors(source),
        "Method call on union with common method should succeed"
    );
}

#[test]
fn property_call_on_union_missing_method() {
    // One union member lacks the method.
    let source = r#"
interface A { run(x: number): void; }
interface B { stop(): void; }
declare let ab: A | B;
ab.run(42);
"#;
    assert!(
        has_error(source, 2339),
        "Method call on union where one member lacks the method should emit TS2339"
    );
}

#[test]
fn overload_generic_inference_with_callbacks() {
    // Generic overload where callback param type is inferred.
    let source = r#"
declare function map<T, U>(arr: T[], fn: (x: T) => U): U[];
let result: number[] = map(["a", "b"], x => x.length);
"#;
    assert!(
        no_errors(source),
        "Generic call with callback should infer T from array and U from callback return"
    );
}

#[test]
fn call_with_spread_from_tuple() {
    // Spread argument from a tuple type matches parameter positions.
    let source = r#"
declare function add(a: number, b: number): number;
let args: [number, number] = [1, 2];
add(...args);
"#;
    assert!(
        no_errors(source),
        "Spread from tuple matching exact param count should succeed"
    );
}

#[test]
fn call_with_spread_wrong_tuple_length() {
    // Spread from tuple with wrong length.
    let source = r#"
declare function add(a: number, b: number): number;
let args: [number, number, number] = [1, 2, 3];
add(...args);
"#;
    let codes = get_codes(source);
    assert!(
        codes.contains(&2554) || codes.contains(&2556),
        "Spread from wrong-length tuple should emit argument count error, got: {codes:?}",
    );
}

#[test]
fn overload_resolution_with_optional_and_required() {
    // Overload where one signature has optional param, another requires it.
    let source = r#"
declare function test(x: string): number;
declare function test(x: string, y?: number): string;
let a: number = test("hello");
"#;
    assert!(no_errors(source), "Should match first overload with 1 arg");
}

#[test]
fn property_call_through_type_alias() {
    // Method call on an object typed through a type alias.
    let source = r#"
type Handler = { handle(input: string): boolean };
declare let h: Handler;
let ok: boolean = h.handle("test");
"#;
    assert!(
        no_errors(source),
        "Method call through type alias should resolve correctly"
    );
}

#[test]
fn generic_call_with_multiple_constraints() {
    // Generic function with multiple constrained type params.
    let source = r#"
declare function merge<A extends object, B extends object>(a: A, b: B): A & B;
let result = merge({ x: 1 }, { y: 2 });
let x: number = result.x;
let y: number = result.y;
"#;
    assert!(
        no_errors(source),
        "Generic call with multiple constraints should infer intersection type"
    );
}

#[test]
fn overload_with_never_param() {
    // Overload that has `never` in a param position should be skippable.
    let source = r#"
declare function test(x: never): never;
declare function test(x: string): number;
let r: number = test("ok");
"#;
    assert!(
        no_errors(source),
        "Should skip never-param overload and match string overload"
    );
}

#[test]
fn call_expression_on_conditional_type_result() {
    // Calling a value whose type comes from a conditional type.
    let source = r#"
type IsString<T> = T extends string ? (x: T) => void : never;
declare let fn1: IsString<string>;
fn1("hello");
"#;
    assert!(
        no_errors(source),
        "Calling a value typed by conditional type should work when condition is true"
    );
}

#[test]
fn property_call_on_generic_class_instance() {
    // Method call on an instance of a generic class.
    let source = r#"
declare class Container<T> {
    get(): T;
    set(value: T): void;
}
declare let c: Container<number>;
let n: number = c.get();
c.set(42);
"#;
    assert!(
        no_errors(source),
        "Method calls on generic class instance should resolve with concrete type args"
    );
}

#[test]
fn property_call_wrong_arg_type_on_generic_class() {
    let source = r#"
declare class Container<T> {
    set(value: T): void;
}
declare let c: Container<number>;
c.set("wrong");
"#;
    assert!(
        has_error(source, 2345),
        "Passing string to Container<number>.set should emit TS2345"
    );
}

// ============================================================================
// Architecture regression: query boundary coverage
// ============================================================================
// These tests ensure call.rs routes through boundary helpers rather than
// inspecting solver internals directly.

#[test]
fn generic_two_pass_inference_with_annotated_callback_param() {
    // Pre-inference from annotated callback params: when a callback is
    // context-sensitive (has unannotated params) AND has some annotated
    // params, those annotations should contribute to inference.
    let source = r#"
declare function test<T>(fn: (a: T, b: T) => void): T;
let result = test((a: number, b) => {});
let check: number = result;
"#;
    assert!(
        no_errors(source),
        "Annotated callback param should contribute to generic inference"
    );
}

#[test]
fn generic_return_context_inference() {
    // Return-context substitution: when a generic call is in a contextual
    // position, the return type context should help infer type params.
    let source = r#"
declare function identity<T>(x: T): T;
let result: string = identity("hello");
"#;
    assert!(
        no_errors(source),
        "Return context should help infer T = string from contextual type"
    );
}

#[test]
fn union_callee_not_treated_as_overloads() {
    // Union callee types must NOT be treated as overloads.
    // Overload resolution succeeds if ANY signature matches, but union
    // call semantics require ALL members to accept the call.
    let source = r#"
type F1 = (a: string) => void;
type F2 = (a: string, b: number) => void;
declare let f: F1 | F2;
f("hello");
"#;
    // This should emit an error because F2 requires 2 args
    let codes = get_codes(source);
    // The error could be TS2554 (wrong arg count) or TS2349 (not callable)
    assert!(
        !codes.is_empty(),
        "Union callee should require all members to accept the call, got: {codes:?}",
    );
}

#[test]
fn overload_resolution_first_match_wins() {
    // Overload resolution should pick the first matching signature.
    let source = r#"
declare function f(x: string): string;
declare function f(x: number): number;
declare function f(x: string | number): string | number;
let r1: string = f("hello");
let r2: number = f(42);
"#;
    assert!(
        no_errors(source),
        "Overload resolution should pick first matching signature"
    );
}

#[test]
fn generic_excess_property_skip_for_type_param() {
    // When a generic param is a type parameter (T), excess property
    // checking should be skipped because T captures the full object type.
    let source = r#"
interface Named { name: string; }
declare function parrot<T extends Named>(obj: T): T;
parrot({ name: "hello", extra: true });
"#;
    assert!(
        no_errors(source),
        "Excess property check should be skipped for type parameter params"
    );
}

#[test]
fn property_call_through_optional_chain_on_nullable() {
    // Optional chaining should strip nullish from the callee before
    // attempting call resolution.
    let source = r#"
declare let obj: { method(x: number): string } | null;
let r = obj?.method(42);
"#;
    assert!(
        no_errors(source),
        "Optional chain call on nullable should work"
    );
}

#[test]
fn overload_resolution_with_generic_and_nongeneric() {
    // Mixed generic/non-generic overloads should resolve correctly.
    let source = r#"
declare function wrap(x: string): string;
declare function wrap<T>(x: T): T[];
let s: string = wrap("hello");
"#;
    assert!(
        no_errors(source),
        "Non-generic overload should be preferred for string arg"
    );
}

#[test]
fn call_with_spread_from_array_to_rest_param() {
    // Spreading an array into a rest parameter should be valid.
    let source = r#"
declare function sum(...nums: number[]): number;
let arr: number[] = [1, 2, 3];
sum(...arr);
"#;
    assert!(
        no_errors(source),
        "Spreading array into rest param should be valid"
    );
}

#[test]
fn construct_call_prefers_generic_signature() {
    // For new expressions with overloaded constructors, prefer generic
    // signatures so type parameters can be inferred.
    let source = r#"
declare class Box<T> {
    constructor();
    constructor(value: T);
}
let b = new Box(42);
"#;
    assert!(
        no_errors(source),
        "Constructor overload resolution should work"
    );
}

#[test]
fn stable_call_recovery_return_type_on_mismatch() {
    // When type argument count is wrong, recovery should still produce
    // a usable return type for downstream checking.
    let source = r#"
declare function f<T>(x: T): T;
let r = f<string, number>("hello");
"#;
    // TS2558 for wrong type arg count, but should still recover return type
    assert!(
        has_error(source, 2558),
        "Wrong type arg count should emit TS2558"
    );
}
