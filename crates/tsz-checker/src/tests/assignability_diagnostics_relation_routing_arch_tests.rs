use std::fs;
use std::path::Path;

#[test]
fn assignability_diagnostics_routes_top_level_mismatch_probes_through_relation_outcomes() {
    let root_source = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/assignability/assignability_diagnostics.rs"),
    )
    .expect("failed to read assignability_diagnostics.rs");
    let argument_reports = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/assignability/assignability_diagnostics/argument_reports.rs"),
    )
    .expect("failed to read argument_reports.rs");
    let source = format!("{root_source}\n{argument_reports}");

    assert!(
        source.matches("assign_relation_outcome(").count() >= 10,
        "top-level assignability diagnostics should use RelationOutcome for TS2322-family probes"
    );
    assert!(
        source.contains("call_arg_relation_outcome(source, target)"),
        "argument diagnostics should use the TS2345 RelationOutcome path"
    );
    assert!(
        !source
            .contains("assign_relation_outcome(source, target).related && !checker_only_mismatch"),
        "argument diagnostics should not use the generic assign request for the initial TS2345 probe"
    );
    let suggest_call_start = source
        .find("pub(crate) fn should_suggest_calling_for_weak_type")
        .expect("missing weak-type call suggestion helper");
    let suggest_call_end = suggest_call_start
        + source[suggest_call_start..]
            .find("pub(crate) fn checker_only_assignability_failure_reason")
            .expect("missing next assignability diagnostics helper");
    let suggest_call_helper = &source[suggest_call_start..suggest_call_end];
    assert_eq!(
        suggest_call_helper
            .matches("return_relation_outcome(")
            .count(),
        2,
        "weak-type call/construct suggestions should route result probes through the return relation outcome"
    );
    assert!(
        !suggest_call_helper.contains("assign_relation_outcome("),
        "weak-type call/construct suggestions should not use generic assignment relation outcomes for result probes"
    );
    assert!(
        !source.contains("diagnostic_relation_boolean_guard("),
        "top-level assignability diagnostics should not regress to the raw boolean relation guard"
    );
}
