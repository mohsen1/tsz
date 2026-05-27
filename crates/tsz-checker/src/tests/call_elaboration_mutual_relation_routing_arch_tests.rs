use std::fs;

#[test]
fn call_elaboration_mutual_assignability_uses_relation_outcome_boundary() {
    let source = fs::read_to_string("src/error_reporter/call_errors/elaboration.rs")
        .expect("failed to read elaboration.rs");
    let start = source
        .find("fn types_are_mutually_assignable")
        .expect("missing mutual assignability helper");
    let end = start
        + source[start..]
            .find("pub(in crate::error_reporter::call_errors) fn contextual_constraint_parameter_display")
            .expect("missing next call elaboration helper");
    let helper = &source[start..end];

    assert_eq!(
        helper.matches("assign_relation_outcome(").count(),
        2,
        "mutual assignability display helper should route both relation directions through assign_relation_outcome"
    );
    assert!(
        helper.matches(".related").count() >= 2,
        "mutual assignability display helper should use relation outcome decisions"
    );
    assert!(
        !helper.contains("diagnostic_relation_boolean_guard("),
        "mutual assignability display helper should not regress to raw boolean relation guards"
    );
}

#[test]
fn call_elaboration_return_probes_use_relation_outcome_boundary() {
    let source = fs::read_to_string("src/error_reporter/call_errors/elaboration.rs")
        .expect("failed to read elaboration.rs");

    let assignment_start = source
        .find("fn try_elaborate_assignment_source_error_with_options")
        .expect("missing assignment source elaboration helper");
    let assignment_end = assignment_start
        + source[assignment_start..]
            .find("pub(crate) fn try_emit_polymorphic_this_object_literal_arg_errors")
            .expect("missing next object-literal argument helper");
    let assignment_helper = &source[assignment_start..assignment_end];
    assert!(
        assignment_helper.contains("assign_relation_outcome(branch_type, target_type)")
            && assignment_helper.contains(".related"),
        "conditional branch elaboration should route relation truth through RelationOutcome"
    );
    assert!(
        !assignment_helper.contains("diagnostic_relation_boolean_guard(branch_type, target_type)"),
        "conditional branch elaboration should not use the raw relation guard"
    );

    let callback_start = source
        .find("fn try_elaborate_function_arg_return_error_with_options")
        .expect("missing function argument return elaboration helper");
    let callback_end = callback_start
        + source[callback_start..]
            .find("fn try_elaborate_function_block_returns_with_param_type")
            .expect("missing block return elaboration helper");
    let callback_helper = &source[callback_start..callback_end];
    assert_eq!(
        callback_helper
            .matches("assign_relation_outcome(body_type, expected_return_type)")
            .count(),
        2,
        "callback return elaboration should route both body relation probes through RelationOutcome"
    );
    assert!(
        !callback_helper
            .contains("diagnostic_relation_boolean_guard(body_type, expected_return_type)"),
        "callback return elaboration should not use raw relation guards"
    );
}
