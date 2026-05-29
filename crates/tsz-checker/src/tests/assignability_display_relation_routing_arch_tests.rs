use std::fs;

#[test]
fn assignability_display_type_check_uses_relation_outcome_boundary() {
    let source = fs::read_to_string("src/assignability/assignability_diagnostics/display_types.rs")
        .expect("failed to read display types source");

    let function_start = source
        .find("fn check_assignable_or_report_at_with_display_types_and_options(")
        .expect("find display-type assignability helper");
    let helper = &source[function_start..];

    assert!(
        helper.contains("self.assign_relation_outcome(source, target).related"),
        "display-type assignability relation truth should route through relation outcomes"
    );
    assert!(
        !helper.contains("diagnostic_relation_boolean_guard(source, target)"),
        "display-type assignability should not use the raw boolean relation guard"
    );
}
