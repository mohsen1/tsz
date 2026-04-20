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

