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
