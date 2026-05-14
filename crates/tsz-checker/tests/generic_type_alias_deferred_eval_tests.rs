use tsz_checker::diagnostics::diagnostic_codes;
use tsz_checker::test_utils::check_source_diagnostics;

fn diagnostic_codes_for(source: &str) -> Vec<u32> {
    check_source_diagnostics(source)
        .into_iter()
        .map(|diag| diag.code)
        .collect()
}

#[test]
fn generic_alias_declaration_does_not_expand_type_parameter_constraints() {
    let codes = diagnostic_codes_for(
        r#"
type Digit = "0" | "1" | "2" | "3" | "4" | "5" | "6";
type Deferred<T extends Digit> = `${T}${T}${T}${T}${T}${T}`;
"#,
    );

    assert!(
        !codes.contains(
            &diagnostic_codes::EXPRESSION_PRODUCES_A_UNION_TYPE_THAT_IS_TOO_COMPLEX_TO_REPRESENT
        ),
        "generic alias declaration should not eagerly expand the constrained type parameter; got {codes:?}"
    );
}

#[test]
fn concrete_generic_alias_instantiation_still_reports_too_complex_union() {
    let codes = diagnostic_codes_for(
        r#"
type Digit = "0" | "1" | "2" | "3" | "4" | "5" | "6";
type Deferred<T extends Digit> = `${T}${T}${T}${T}${T}${T}`;
type Use = Deferred<Digit>;
"#,
    );

    assert!(
        codes.contains(
            &diagnostic_codes::EXPRESSION_PRODUCES_A_UNION_TYPE_THAT_IS_TOO_COMPLEX_TO_REPRESENT
        ),
        "concrete generic alias instantiation should still report TS2590; got {codes:?}"
    );
}

#[test]
fn renamed_generic_alias_declaration_keeps_the_same_deferred_behavior() {
    let codes = diagnostic_codes_for(
        r#"
type Letter = "a" | "b" | "c" | "d" | "e" | "f" | "g";
type Boxed<X extends Letter> = { [Key in X]: `${Key}${Key}${Key}${Key}${Key}` };
"#,
    );

    assert!(
        !codes.contains(
            &diagnostic_codes::EXPRESSION_PRODUCES_A_UNION_TYPE_THAT_IS_TOO_COMPLEX_TO_REPRESENT
        ),
        "renamed generic alias declaration should not eagerly expand; got {codes:?}"
    );
}
