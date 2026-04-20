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

