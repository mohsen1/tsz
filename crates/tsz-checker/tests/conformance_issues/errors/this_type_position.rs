use super::super::core::*;

/// TS2526: 'this' type appearing inside a nested type literal that is the
/// annotation of an interface property must be reported. Without explicit
/// resolution of the property's type annotation through the checker's
/// type-node entry point, the lowering pipeline silently maps `this` to
/// `ThisType` without invoking `is_this_type_allowed`, so the diagnostic
/// is dropped.
///
/// Mirrors `TypeScript/tests/cases/conformance/types/thisType/thisTypeErrors.ts`.
#[test]
fn ts2526_emitted_for_this_in_interface_property_type_literal() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
interface I1 {
    a: { x: this };
}
"#,
    );

    let ts2526_count = diagnostics.iter().filter(|(c, _)| *c == 2526).count();
    assert_eq!(
        ts2526_count, 1,
        "Expected exactly one TS2526 for 'this' inside a nested type literal on \
         an interface property. Got: {diagnostics:#?}"
    );
}

#[test]
fn ts2526_emitted_for_this_in_interface_call_signature_type_literal() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
interface I2 {
    b: { (): this };
}
"#,
    );

    assert!(
        has_error(&diagnostics, 2526),
        "Expected TS2526 for 'this' return type of a nested call signature on \
         an interface property. Got: {diagnostics:#?}"
    );
}

#[test]
fn ts2526_emitted_for_this_in_interface_construct_signature_type_literal() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
interface I3 {
    c: { new (): this };
}
"#,
    );

    assert!(
        has_error(&diagnostics, 2526),
        "Expected TS2526 for 'this' return type of a nested construct signature \
         on an interface property. Got: {diagnostics:#?}"
    );
}

#[test]
fn ts2526_emitted_for_this_in_interface_index_signature_type_literal() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
interface I4 {
    d: { [x: string]: this };
}
"#,
    );

    assert!(
        has_error(&diagnostics, 2526),
        "Expected TS2526 for 'this' as the value type of a nested index signature \
         on an interface property. Got: {diagnostics:#?}"
    );
}

#[test]
fn ts2526_emitted_for_this_in_interface_method_param_and_return() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
interface I5 {
    e: { f(x: this): this };
}
"#,
    );

    let ts2526_count = diagnostics.iter().filter(|(c, _)| *c == 2526).count();
    // Two `this` occurrences inside the nested type literal: parameter type
    // and return type. tsc reports both.
    assert_eq!(
        ts2526_count, 2,
        "Expected exactly two TS2526 for `this` parameter and return inside a \
         nested method signature on an interface property. Got: {diagnostics:#?}"
    );
}

/// `this` is allowed at the top level of an interface property annotation —
/// the interface declaration itself provides a `this` context.
#[test]
fn ts2526_not_emitted_for_top_level_this_on_interface_property() {
    let diagnostics = compile_and_get_diagnostics(
        r#"
interface I6 {
    self: this;
}
"#,
    );

    assert!(
        !has_error(&diagnostics, 2526),
        "Did not expect TS2526 for `this` directly as an interface property \
         type — the interface provides a `this` context. Got: {diagnostics:#?}"
    );
}
