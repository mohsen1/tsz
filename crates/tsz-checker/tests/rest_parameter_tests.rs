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

// An optional rest parameter's effective type includes `| undefined` under
// strictNullChecks (tsc's `addOptionality`), so `...p?: T[]` has type
// `T[] | undefined`, which is not an array type. tsc reports TS2370 for it in
// every container. The binder names below vary so the rule cannot be a
// name-driven special case.

#[test]
fn test_optional_rest_array_in_function_declaration_emits_ts2370() {
    assert!(has_error_ts2370("function collect(...items?: number[]) {}"));
}

#[test]
fn test_optional_rest_array_in_class_method_emits_ts2370() {
    assert!(has_error_ts2370(
        "class Bucket { drain(...entries?: string[]) {} }"
    ));
}

#[test]
fn test_optional_rest_array_in_arrow_emits_ts2370() {
    assert!(has_error_ts2370(
        "const gather = (...vals?: boolean[]) => {};"
    ));
}

#[test]
fn test_optional_rest_tuple_emits_ts2370() {
    assert!(has_error_ts2370(
        "function pair(...tail?: [number, string]) {}"
    ));
}

#[test]
fn test_optional_rest_any_array_emits_ts2370() {
    // `any[] | undefined` is still not an array type, so tsc reports TS2370.
    assert!(has_error_ts2370("function forward(...rest?: any[]) {}"));
}

#[test]
fn test_optional_rest_plain_any_is_accepted() {
    // `any | undefined` collapses to `any`, which is a valid rest type.
    assert!(!has_error_ts2370("function forward(...rest?: any) {}"));
}

#[test]
fn test_optional_rest_array_in_interface_method_emits_ts2370() {
    assert!(has_error_ts2370(
        "interface Sink { push(...records?: number[]): void; }"
    ));
}

#[test]
fn test_optional_rest_array_in_interface_call_signature_emits_ts2370() {
    assert!(has_error_ts2370(
        "interface Invoker { (...args?: string[]): void; }"
    ));
}

#[test]
fn test_optional_rest_array_in_interface_construct_signature_emits_ts2370() {
    assert!(has_error_ts2370(
        "interface Factory { new (...deps?: object[]): Factory; }"
    ));
}

#[test]
fn test_optional_rest_array_in_type_literal_method_alias_emits_ts2370() {
    assert!(has_error_ts2370(
        "type Handler = { handle(...events?: number[]): void; };"
    ));
}

#[test]
fn test_optional_rest_array_in_type_literal_annotation_emits_ts2370() {
    assert!(has_error_ts2370(
        "let node: { visit(...children?: string[]): void };"
    ));
}

#[test]
fn test_optional_rest_array_in_function_type_emits_ts2370() {
    assert!(has_error_ts2370(
        "type Callback = (...payloads?: number[]) => void;"
    ));
}

// Non-array rest parameters were previously unchecked for interface and
// type-literal signatures (they never flowed through the function/method
// checking paths). They must report TS2370 regardless of optionality.

#[test]
fn test_non_array_rest_in_interface_call_signature_emits_ts2370() {
    assert!(has_error_ts2370(
        "interface Runner { (...code: number): void; }"
    ));
}

#[test]
fn test_non_array_rest_in_type_literal_annotation_emits_ts2370() {
    assert!(has_error_ts2370(
        "let sink: { emit(...code: number): void };"
    ));
}

// Valid array rest parameters stay clean in every container — the fix must not
// over-fire.

#[test]
fn test_required_rest_array_in_interface_method_is_clean() {
    assert!(!has_error_ts2370(
        "interface Sink { push(...records: number[]): void; }"
    ));
}

#[test]
fn test_required_rest_array_in_type_literal_annotation_is_clean() {
    assert!(!has_error_ts2370(
        "let node: { visit(...children: string[]): void };"
    ));
}

#[test]
fn test_unannotated_optional_rest_in_function_declaration_is_clean() {
    // A function-declaration rest with no annotation is implicitly `any`, not
    // `any[]`, so tsc reports the implicit-any grammar (TS7019) but not TS2370.
    assert!(!has_error_ts2370("function forward(...rest?) {}"));
}
