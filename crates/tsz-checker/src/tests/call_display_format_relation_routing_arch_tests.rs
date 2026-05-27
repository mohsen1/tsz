use std::fs;

#[test]
fn call_display_overlap_uses_relation_outcome_boundary() {
    let source =
        fs::read_to_string("src/error_reporter/call_errors/display_formatting_parameters.rs")
            .expect("failed to read display_formatting_parameters.rs");
    let start = source
        .find("fn types_overlap_for_diagnostic_display")
        .expect("missing display overlap helper");
    let helper = &source[start..];

    assert_eq!(
        helper.matches("assign_relation_outcome(").count(),
        2,
        "display overlap helper should route both relation directions through assign_relation_outcome"
    );
    assert!(
        helper.matches(".related").count() >= 2,
        "display overlap helper should use relation outcome decisions"
    );
    assert!(
        !helper.contains("diagnostic_relation_boolean_guard("),
        "display overlap helper should not regress to raw boolean relation guards"
    );
}

#[test]
fn generator_never_yield_display_uses_env_relation_outcome_boundary() {
    let source = fs::read_to_string("src/types/computation/call_display.rs")
        .expect("failed to read call_display.rs");
    let start = source
        .find("pub(crate) fn is_assignable_via_generator_never_yield_callback")
        .expect("missing generator never-yield display helper");
    let end = source[start..]
        .find("fn generic_arg_refresh_context_is_concrete")
        .expect("missing next call display helper")
        + start;
    let helper = &source[start..end];

    assert_eq!(
        helper.matches("assign_relation_outcome_with_env(").count(),
        2,
        "generator never-yield display fallback should route env-aware relation probes through RelationOutcome"
    );
    assert!(
        helper.matches(".related").count() >= 2,
        "generator never-yield display fallback should use relation outcome decisions"
    );
    assert!(
        !helper.contains("diagnostic_relation_boolean_guard_with_env("),
        "generator never-yield display fallback should not regress to raw env boolean guards"
    );
}
