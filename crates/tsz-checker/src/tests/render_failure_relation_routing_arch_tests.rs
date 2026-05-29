use std::fs;

#[test]
fn strict_callback_inner_parameter_mismatch_uses_relation_outcome_boundary() {
    let source = fs::read_to_string("src/error_reporter/render_failure.rs")
        .expect("failed to read render failure source");
    let start = source
        .find("fn strict_callback_inner_parameter_mismatch_exists")
        .expect("missing strict callback inner parameter mismatch helper");
    let end = source[start..]
        .find("fn no_union_member_matches_switch_source_display")
        .expect("missing next render failure helper")
        + start;
    let helper = &source[start..end];

    assert_eq!(
        helper.matches("assign_relation_outcome").count(),
        1,
        "strict callback inner parameter mismatch should route through assign_relation_outcome"
    );
    assert!(
        helper.contains(".related"),
        "strict callback inner parameter mismatch should use the relation outcome decision"
    );
    assert!(
        !helper.contains("diagnostic_relation_boolean_guard"),
        "strict callback inner parameter mismatch should not regress to the raw boolean relation guard"
    );
}
