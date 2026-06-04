//! Tests for TS2370: A rest parameter must be of an array type

use crate::diagnostics::diagnostic_codes;

fn has_error_ts2370(source: &str) -> bool {
    crate::test_utils::check_source_codes(source)
        .contains(&diagnostic_codes::A_REST_PARAMETER_MUST_BE_OF_AN_ARRAY_TYPE)
}

#[test]
fn test_rest_parameter_non_array_type_emits_ts2370() {
    let source = r"
        function f(x: string, ...rest: number) {
        }
    ";

    assert!(has_error_ts2370(source));
}

#[test]
fn test_rest_parameter_array_type_ok() {
    let source = r"
        function f(x: string, ...rest: number[]) {
        }
    ";

    assert!(!has_error_ts2370(source));
}

#[test]
fn test_rest_parameter_tuple_type_ok() {
    let source = r"
        function f(...rest: [string, number]) {
        }
    ";

    assert!(!has_error_ts2370(source));
}

#[test]
fn test_rest_parameter_union_of_array_types_ok() {
    let source = r"
        type someArray = string[] | number[];
        function f(...rest: someArray) {
        }
    ";

    assert!(!has_error_ts2370(source));
}

#[test]
fn test_rest_parameter_no_type_annotation_ok() {
    let source = r"
        function f(...rest) {
        }
    ";

    assert!(!has_error_ts2370(source));
}

#[test]
fn test_rest_parameter_array_generic_ok() {
    let source = r"
        function f<T>(...rest: T[]) {
        }
    ";

    assert!(!has_error_ts2370(source));
}

#[test]
fn test_rest_parameter_in_method() {
    let source = r"
        class C {
            method(...rest: string) {
            }
        }
    ";

    assert!(has_error_ts2370(source));
}

#[test]
fn test_rest_parameter_in_constructor() {
    let source = r"
        class C {
            constructor(...rest: boolean) {
            }
        }
    ";

    assert!(has_error_ts2370(source));
}

#[test]
fn test_rest_parameter_property_without_type_does_not_emit_ts2370() {
    let source = r"
        class C {
            constructor(public ...rest) {
            }
        }
    ";

    assert!(!has_error_ts2370(source));
}

#[test]
fn test_rest_parameter_in_arrow_function() {
    let source = r"
        const f = (...rest: number) => {};
    ";

    assert!(has_error_ts2370(source));
}

#[test]
fn test_optional_rest_parameter_without_type_emits_ts2370() {
    let source = r"
        (...arg?) => 102;
    ";

    assert!(has_error_ts2370(source));
}

// TS1013 was incorrectly emitted for trailing commas after rest parameters.
// tsc has accepted trailing commas in parameter lists since TS 4.0 (TS#40707).
// Regression tests ensuring TS1013 is NOT emitted.

#[test]
fn trailing_comma_after_rest_in_declare_function_is_accepted() {
    let codes =
        crate::test_utils::check_source_codes(r"declare function f(...args: number[],): void;");
    assert!(
        !codes.contains(&1013),
        "TS1013 must not fire for trailing comma after rest parameter: {codes:?}"
    );
}

#[test]
fn trailing_comma_after_rest_with_preceding_params_is_accepted() {
    let codes = crate::test_utils::check_source_codes(
        r"declare function g(a: string, ...rest: boolean[],): void;",
    );
    assert!(
        !codes.contains(&1013),
        "TS1013 must not fire for trailing comma after rest (with leading params): {codes:?}"
    );
}

#[test]
fn trailing_comma_after_rest_in_arrow_function_is_accepted() {
    let codes = crate::test_utils::check_source_codes(r"const f = (...args: string[],) => args;");
    assert!(
        !codes.contains(&1013),
        "TS1013 must not fire for trailing comma after rest in arrow function: {codes:?}"
    );
}

#[test]
fn trailing_comma_after_rest_in_method_signature_is_accepted() {
    let codes =
        crate::test_utils::check_source_codes(r"interface I { method(...args: number[],): void; }");
    assert!(
        !codes.contains(&1013),
        "TS1013 must not fire for trailing comma after rest in method signature: {codes:?}"
    );
}
