//! Tests for TS1013 (trailing comma after rest) specifically in function
//! parameter lists vs binding patterns.
//!
//! ECMAScript 2017+ allows trailing commas after rest parameters in function
//! signatures. TypeScript inherits this rule: `f(...args,)` is valid.
//! Binding pattern rests (`{...x,}`, `[...x,]`) remain illegal per spec.

use crate::parser::test_fixture::parse_source;

fn error_codes(source: &str) -> Vec<u32> {
    let (parser, _root) = parse_source(source);
    parser.get_diagnostics().iter().map(|d| d.code).collect()
}

fn has_code(source: &str, code: u32) -> bool {
    error_codes(source).contains(&code)
}

// --- function parameter rest: trailing comma is VALID ---

#[test]
fn test_function_rest_trailing_comma_no_ts1013() {
    // Plain function signature
    assert!(
        !has_code("function f(...args: number[],): void {}", 1013),
        "trailing comma after rest param must NOT be TS1013 in function"
    );
}

#[test]
fn test_arrow_rest_trailing_comma_no_ts1013() {
    // Arrow function
    assert!(
        !has_code("const f = (...args: string[],) => {};", 1013),
        "trailing comma after rest param must NOT be TS1013 in arrow"
    );
}

#[test]
fn test_call_signature_rest_trailing_comma_no_ts1013() {
    // Call signature in type / declare
    assert!(
        !has_code("declare function g(...xs: any[],): void;", 1013),
        "trailing comma after rest param must NOT be TS1013 in declare function"
    );
}

#[test]
fn test_method_rest_trailing_comma_no_ts1013() {
    // Class method
    assert!(
        !has_code("class C { m(...a: boolean[],): void {} }", 1013),
        "trailing comma after rest param must NOT be TS1013 in class method"
    );
}

#[test]
fn test_no_rest_regular_trailing_comma_no_ts1013() {
    // Sanity: regular trailing comma (non-rest) must not produce TS1013 either
    assert!(
        !has_code("function f(a: number, b: string,): void {}", 1013),
        "trailing comma after regular param must NOT be TS1013"
    );
}

// --- binding pattern rest: trailing comma is INVALID ---

#[test]
fn test_object_binding_rest_trailing_comma_ts1013() {
    assert!(
        has_code("const { ...rest, } = { a: 1 };", 1013),
        "trailing comma after object binding rest MUST be TS1013"
    );
}

#[test]
fn test_array_binding_rest_trailing_comma_ts1013() {
    assert!(
        has_code("const [...rest, ] = [1, 2, 3];", 1013),
        "trailing comma after array binding rest MUST be TS1013"
    );
}
