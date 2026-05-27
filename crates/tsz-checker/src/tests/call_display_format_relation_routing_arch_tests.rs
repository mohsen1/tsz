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
