//! Tests for trailing void parameter optionality.
//!
//! In TypeScript, a parameter of type `void` (or a union containing `void`) can
//! be omitted at the call site when it is a trailing parameter. This is analogous
//! to optional parameters — `f(x: number, y: void)` can be called as `f(42)`.
//! However, `void` params that precede required non-void params are NOT optional:
//! `f(x: void, y: number)` requires two arguments.

use crate::test_utils::check_source_codes as get_error_codes;

/// A single void parameter can be omitted.
#[test]
fn test_single_void_param_is_optional() {
    let codes = get_error_codes(
        r#"
declare function f(p: void): void;
f();
"#,
    );
    assert!(
        !codes.contains(&2554),
        "Should not emit TS2554 when omitting a void parameter, got: {codes:?}"
    );
}

/// A trailing void parameter after required params can be omitted.
#[test]
fn test_trailing_void_param_optional() {
    let codes = get_error_codes(
        r#"
declare function a(x: number, y: string, z: void): void;
a(4, "hello");
"#,
    );
    assert!(
        !codes.contains(&2554),
        "Should not emit TS2554 when omitting trailing void param, got: {codes:?}"
    );
}

/// Providing the trailing void argument explicitly is also OK.
#[test]
fn test_trailing_void_param_explicit_ok() {
    let codes = get_error_codes(
        r#"
declare function a(x: number, y: string, z: void): void;
a(4, "hello", undefined);
"#,
    );
    assert!(
        !codes.contains(&2554),
        "Should not emit TS2554 when providing void param explicitly, got: {codes:?}"
    );
}

/// A void param before a required non-void param is NOT optional.
#[test]
fn test_non_trailing_void_param_required() {
    let codes = get_error_codes(
        r#"
declare function b(x: number, y: string, z: void, what: number): void;
b(4, "hello");
"#,
    );
    assert!(
        codes.contains(&2554),
        "Should emit TS2554 when omitting non-trailing void param, got: {codes:?}"
    );
}

/// Multiple trailing void params can all be omitted.
#[test]
fn test_multiple_trailing_void_params() {
    let codes = get_error_codes(
        r#"
declare function c(x: number | void, y: void, z: void | string | number): void;
c(3);
c();
"#,
    );
    assert!(
        !codes.contains(&2554),
        "Should not emit TS2554 when omitting multiple trailing void params, got: {codes:?}"
    );
}

/// Inferred tuple rest elements that are all void-like can also be omitted.
#[test]
fn test_generic_rest_tuple_with_trailing_void_elements() {
    let codes = get_error_codes(
        r#"
declare function call<TS extends unknown[]>(
    handler: (...args: TS) => unknown,
    ...args: TS): void;

call((x: number, y: void) => x, 4);
call((x: void, y: void) => 42);
call((x: number | void, y: number | void) => 42);
call((x: number | void, y: number | void) => 42, 4);
"#,
    );
    assert!(
        !codes.contains(&2554),
        "Should not emit TS2554 when inferred rest tuple has trailing void-like elements, got: {codes:?}"
    );
}

/// Union containing void also counts — `number | void` is effectively optional.
#[test]
fn test_union_with_void_param_optional() {
    let codes = get_error_codes(
        r#"
declare function f(x: number | void): void;
f();
"#,
    );
    assert!(
        !codes.contains(&2554),
        "Should not emit TS2554 for param typed as `number | void`, got: {codes:?}"
    );
}

/// Generic class with void type argument — method param becomes void.
#[test]
fn test_generic_class_void_type_arg() {
    let codes = get_error_codes(
        r#"
class X<T> {
    f(t: T): void {}
}
declare const x: X<void>;
x.f();
"#,
    );
    assert!(
        !codes.contains(&2554),
        "Should not emit TS2554 for generic method with void type arg, got: {codes:?}"
    );
}

/// `any` and `unknown` params should NOT be optional (unlike void).
#[test]
fn test_any_param_not_optional() {
    let codes = get_error_codes(
        r#"
declare function f(p: any): void;
f();
"#,
    );
    assert!(
        codes.contains(&2554),
        "Should emit TS2554 when omitting `any` param (any is not void), got: {codes:?}"
    );
}

/// `unknown` params should NOT be optional (unlike void).
#[test]
fn test_unknown_param_not_optional() {
    let codes = get_error_codes(
        r#"
declare function f(p: unknown): void;
f();
"#,
    );
    assert!(
        codes.contains(&2554),
        "Should emit TS2554 when omitting `unknown` param, got: {codes:?}"
    );
}
