use tsz_checker::test_utils::check_source_code_messages;

#[test]
fn bare_scoped_type_parameters_do_not_emit_missing_name_diagnostics() {
    let diagnostics = check_source_code_messages(
        r#"
type Pair<Alpha, Beta> = Alpha | Beta | { left: Alpha; right: Beta };
type Other<X, Y> = { first: X; second: Y } | X | Y;
"#,
    );

    assert!(
        !diagnostics.iter().any(|(code, _)| *code == 2304),
        "Expected scoped type parameters to avoid TS2304, got {diagnostics:?}"
    );
}

#[test]
fn scoped_type_parameter_reference_with_type_arguments_still_validates() {
    let diagnostics = check_source_code_messages(
        r#"
type Bad<T, U> = T<U>;
"#,
    );

    assert!(
        diagnostics.iter().any(|(code, _)| *code == 2315),
        "Expected TS2315 for generic use of a type parameter, got {diagnostics:?}"
    );
}
