use std::fs;

#[test]
fn call_result_recovery_uses_relation_outcome_boundary() {
    let source = fs::read_to_string("src/types/computation/call_result.rs")
        .expect("failed to read call_result.rs");

    let recovery_start = source
        .find("fn correlated_union_call_recovery_return")
        .expect("missing correlated_union_call_recovery_return helper");
    let recovery_end = recovery_start
        + source[recovery_start..]
            .find("fn finalize_call_return_like_success")
            .expect("missing finalize_call_return_like_success helper");
    let recovery_helper = &source[recovery_start..recovery_end];

    assert!(
        recovery_helper.contains("assign_relation_outcome(actual, param_union).related"),
        "correlated union call recovery should route assignability through RelationOutcome.related"
    );
    assert!(
        !recovery_helper.contains("diagnostic_relation_boolean_guard"),
        "correlated union call recovery should not regress to the raw boolean relation guard"
    );

    let argument_start = source
        .find("fn report_polymorphic_this_indexed_conditional_arg")
        .expect("missing report_polymorphic_this_indexed_conditional_arg helper");
    let argument_end = argument_start
        + source[argument_start..]
            .find("fn error_argument_not_assignable_preserving_param_display")
            .expect("missing error_argument_not_assignable_preserving_param_display helper");
    let argument_helper = &source[argument_start..argument_end];

    assert!(
        argument_helper.contains("assign_relation_outcome(arg_types[2], target).related"),
        "polymorphic-this argument diagnostics should route assignability through RelationOutcome.related"
    );
    assert!(
        !argument_helper.contains("diagnostic_relation_boolean_guard"),
        "polymorphic-this argument diagnostics should not regress to the raw boolean relation guard"
    );
}
