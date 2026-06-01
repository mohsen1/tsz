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
        source.matches("assign_relation_outcome(").count() <= 7,
        "remaining generic assignability diagnostics relation probes should keep shrinking"
    );
    let generic_start = argument_reports
        .find("pub(crate) fn check_assignable_or_report_generic_at")
        .expect("missing generic assignability reporter");
    let generic_end = generic_start
        + argument_reports[generic_start..]
            .find("pub(crate) fn check_argument_assignable_or_report")
            .expect("missing argument assignability reporter");
    let generic_reporter = &argument_reports[generic_start..generic_end];
    assert!(
        generic_reporter.contains("assignability_reason_relation_outcome(source, target)"),
        "generic TS2322-style reporter should use the assignability-reason RelationOutcome"
    );
    assert!(
        !generic_reporter.contains("assign_relation_outcome(source, target)"),
        "generic TS2322-style reporter should not use the generic assign request"
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
    let jsx_callback_start = root_source
        .find("pub(crate) fn check_assignable_or_report_jsx_callback_prop_at")
        .expect("missing JSX callback prop assignability reporter");
    let jsx_callback_end = jsx_callback_start
        + root_source[jsx_callback_start..]
            .find("fn check_assignable_or_report_at_with_options")
            .expect("missing next assignability diagnostics helper");
    let jsx_callback_reporter = &root_source[jsx_callback_start..jsx_callback_end];
    assert!(
        jsx_callback_reporter.contains("jsx_props_relation_outcome(source, target)"),
        "JSX callback prop reporter should use the JSX props RelationOutcome"
    );
    assert!(
        !jsx_callback_reporter.contains("assign_relation_outcome(source, target)"),
        "JSX callback prop reporter should not use the generic assign request"
    );
    assert!(
        !source.contains("assign_relation_outcome(target, source).related"),
        "argument diagnostics should not use the generic assign request for reverse callback suppression probes"
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
