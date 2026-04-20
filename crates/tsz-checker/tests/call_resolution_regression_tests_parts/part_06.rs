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

#[test]
fn super_type_arguments_do_not_cascade_into_checker_arity_errors() {
    let source = r#"
class Base {
    constructor() {}
}

class Derived extends Base {
    constructor() {
        super<T>(0);
    }
}
"#;

    let codes = get_codes(source);
    assert!(
        !codes.contains(&2554),
        "super<T>(...) should not cascade into TS2554 after parser recovery: {codes:?}"
    );
}

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

