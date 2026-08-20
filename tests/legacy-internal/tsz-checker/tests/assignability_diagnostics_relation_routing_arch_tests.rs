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

    assert_eq!(
        source.matches("assign_relation_outcome(").count(),
        0,
        "top-level assignability diagnostics should not use generic assign requests"
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
    let checker_only_start = root_source
        .find("pub(crate) fn checker_only_assignability_failure_reason")
        .expect("missing checker-only assignability failure helper");
    let checker_only_end = root_source[checker_only_start..]
        .find("fn iterator_next_type_display_mismatch")
        .map(|offset| checker_only_start + offset)
        .expect("missing next iterator display helper");
    let checker_only_helper = &root_source[checker_only_start..checker_only_end];
    assert!(
        checker_only_helper
            .contains("iterator_result_value_relation_outcome(TypeId::UNDEFINED, value_type)"),
        "checker-only IteratorResult value diagnostics should use the iterator-result-value RelationOutcome"
    );
    assert!(
        !checker_only_helper.contains("assign_relation_outcome(TypeId::UNDEFINED, value_type)"),
        "checker-only IteratorResult value diagnostics should not use the generic assign request"
    );
    let weak_union_skip_start = root_source
        .find("pub(crate) fn should_skip_weak_union_error_with_outcome")
        .expect("missing weak-union skip helper");
    let weak_union_skip_end = root_source[weak_union_skip_start..]
        .find("fn check_excess_properties_for_fresh_source")
        .map(|offset| weak_union_skip_start + offset)
        .expect("missing next assignability diagnostics helper");
    let weak_union_skip_helper = &root_source[weak_union_skip_start..weak_union_skip_end];
    assert!(
        weak_union_skip_helper.contains("assignability_reason_relation_outcome(source, target)"),
        "weak-union/excess-property fallback should build its RelationOutcome through the assignability-reason request"
    );
    assert!(
        !weak_union_skip_helper.contains("assign_relation_outcome(source, target)"),
        "weak-union/excess-property fallback should not use the generic assign request"
    );
    let numeric_enum_start = root_source
        .find("fn numeric_enum_assignment_override_from_source")
        .expect("missing numeric enum assignment override helper");
    let numeric_enum_end = root_source[numeric_enum_start..]
        .find("pub(crate) fn check_assignable_or_report_at_exact_anchor")
        .map(|offset| numeric_enum_start + offset)
        .expect("missing next assignability diagnostics helper");
    let numeric_enum_helper = &root_source[numeric_enum_start..numeric_enum_end];
    let numeric_enum_compact: String = numeric_enum_helper
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect();
    assert!(
        numeric_enum_compact
            .contains("numeric_enum_assignment_relation_outcome(source_literal,structural_target"),
        "numeric enum assignment override should use the numeric-enum assignment RelationOutcome"
    );
    assert!(
        numeric_enum_compact
            .contains("enum_query::numeric_enum_assignment_target(&self.ctx,target"),
        "numeric enum assignment override should ask enum_analysis for target classification"
    );
    assert!(
        numeric_enum_compact.contains("enum_query::numeric_literal_value(self.ctx.types"),
        "numeric enum assignment override should ask enum_analysis for source literal classification"
    );
    assert!(
        !numeric_enum_compact.contains("assign_relation_outcome(source_literal,structural_target"),
        "numeric enum assignment override should not use the generic assign request"
    );
    for forbidden_probe in [
        "query_boundaries::diagnostics::enum_def_id",
        "query_boundaries::diagnostics::enum_member_type",
        "query_boundaries::diagnostics::literal_value",
        "ctx.is_numeric_enum",
        "ctx.is_enum_type",
    ] {
        assert!(
            !numeric_enum_helper.contains(forbidden_probe),
            "numeric enum assignment override should not own enum semantic probe `{forbidden_probe}`"
        );
    }
    let default_reporter_start = root_source
        .find("fn check_assignable_or_report_at_with_options")
        .expect("missing default assignability reporter");
    let default_reporter_end = root_source[default_reporter_start..]
        .find("pub(crate) fn check_assignable_or_report_at_exact_anchor")
        .map(|offset| default_reporter_start + offset)
        .expect("missing exact-anchor assignability reporter");
    let default_reporter = &root_source[default_reporter_start..default_reporter_end];
    assert!(
        default_reporter.contains("assignability_reason_relation_outcome(source, target)"),
        "default TS2322 reporter should use the assignability-reason RelationOutcome"
    );
    assert!(
        !default_reporter.contains("assign_relation_outcome(source, target)"),
        "default TS2322 reporter should not use the generic assign request"
    );
    let exact_anchor_start = root_source
        .find("pub(crate) fn check_assignable_or_report_at_exact_anchor")
        .expect("missing exact-anchor assignability reporter");
    let exact_anchor_end = root_source[exact_anchor_start..]
        .find("pub(crate) fn analyze_assignability_failure")
        .map(|offset| exact_anchor_start + offset)
        .expect("missing next assignability diagnostics helper");
    let exact_anchor_reporters = &root_source[exact_anchor_start..exact_anchor_end];
    assert_eq!(
        exact_anchor_reporters
            .matches("assignability_reason_relation_outcome(source, target)")
            .count(),
        3,
        "exact-anchor TS2322 reporters should use the assignability-reason RelationOutcome"
    );
    assert!(
        !exact_anchor_reporters.contains("assign_relation_outcome(source, target)"),
        "exact-anchor TS2322 reporters should not use the generic assign request"
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
