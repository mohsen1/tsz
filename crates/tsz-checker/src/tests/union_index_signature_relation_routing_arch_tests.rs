use std::fs;

#[test]
fn union_index_signature_value_checks_use_relation_outcome_boundary() {
    let source = fs::read_to_string(
        "src/state/state_checking/property/union_index_signature_diagnostics.rs",
    )
    .expect("failed to read union index signature diagnostics source");

    let function_start = source
        .find("pub(super) fn try_union_index_signature_value_check(")
        .expect("find union index signature diagnostic helper");
    let helper = &source[function_start..];

    assert!(
        helper.matches(".assign_relation_outcome(").count() >= 3
            && helper.matches(".related").count() >= 3,
        "union index-signature value diagnostics should route relation truth through relation outcomes"
    );
    assert!(
        !helper.contains("diagnostic_relation_boolean_guard("),
        "union index-signature value diagnostics should not use raw boolean relation guards"
    );
}
