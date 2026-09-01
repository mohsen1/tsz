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

    assert!(
        helper.contains("call_elaboration_mutual_relation_outcome(left, right)")
            && helper.contains("call_elaboration_mutual_relation_outcome(right, left)"),
        "mutual assignability display helper should route both directions through dedicated relation outcomes"
    );
    assert!(
        helper.matches(".related").count() >= 2,
        "mutual assignability display helper should use relation outcome decisions"
    );
    assert!(
        !helper.contains("assign_relation_outcome("),
        "mutual assignability display helper should not use generic assign relation outcomes"
    );
    assert!(
        !helper.contains("diagnostic_relation_boolean_guard("),
        "mutual assignability display helper should not regress to raw boolean relation guards"
    );
}

#[test]
fn call_elaboration_mutual_relation_outcome_uses_dedicated_request() {
    let source = fs::read_to_string("src/assignability/relation_outcome_helpers.rs")
        .expect("failed to read relation_outcome_helpers.rs");

    assert!(
        source.contains("fn call_elaboration_mutual_relation_outcome(")
            && source.contains("RelationRequest::call_elaboration_mutual("),
        "call elaboration mutual probes should have a dedicated RelationRequest helper"
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
        assignment_helper.contains("return_relation_outcome(branch_type, target_type)")
            && assignment_helper.contains(".related"),
        "conditional return-source branch elaboration should route through the return RelationOutcome"
    );
    assert!(
        !assignment_helper.contains("assign_relation_outcome(branch_type, target_type)"),
        "conditional return-source branch elaboration should not use generic assign relation outcomes"
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
            .matches("return_relation_outcome(body_type, expected_return_type)")
            .count(),
        2,
        "callback return elaboration should route both body relation probes through the return RelationOutcome"
    );
    assert!(
        !callback_helper.contains("assign_relation_outcome(body_type, expected_return_type)"),
        "callback return elaboration should not regress to generic assign relation outcomes"
    );
    assert!(
        !callback_helper
            .contains("diagnostic_relation_boolean_guard(body_type, expected_return_type)"),
        "callback return elaboration should not use raw relation guards"
    );
}

#[test]
fn call_elaboration_polymorphic_this_properties_use_relation_outcome_boundary() {
    let source = fs::read_to_string("src/error_reporter/call_errors/elaboration.rs")
        .expect("failed to read elaboration.rs");

    let helper_start = source
        .find("pub(crate) fn try_emit_polymorphic_this_object_literal_arg_errors")
        .expect("missing polymorphic this object literal helper");
    let helper_end = helper_start
        + source[helper_start..]
            .find("pub fn try_elaborate_object_literal_arg_error_with_source")
            .expect("missing next object literal elaboration helper");
    let helper = &source[helper_start..helper_end];

    assert!(
        helper.contains("call_arg_relation_outcome(source_prop_type, target_prop_type)")
            && helper.contains(".related"),
        "polymorphic this object literal property probes should route through the call-argument RelationOutcome"
    );
    assert!(
        !helper.contains("assign_relation_outcome(source_prop_type, target_prop_type)"),
        "polymorphic this object literal property probes should not use generic assign relation outcomes"
    );
    assert!(
        !helper.contains("diagnostic_relation_boolean_guard(source_prop_type, target_prop_type)"),
        "polymorphic this object literal property probes should not use the raw relation guard"
    );
}

#[test]
fn call_elaboration_object_literal_properties_use_relation_outcome_boundary() {
    let source =
        fs::read_to_string("src/error_reporter/call_errors/elaboration_object_properties.rs")
            .expect("failed to read elaboration_object_properties.rs");

    let helper_start = source
        .find("pub(super) fn try_elaborate_object_literal_properties_with_source")
        .expect("missing object literal properties helper");
    let helper_end = helper_start
        + source[helper_start..]
            .find("fn object_literal_numeric_members_assign_to_mapped_target")
            .expect("missing next object literal helper");
    let helper = &source[helper_start..helper_end];

    assert_eq!(
        helper.matches("call_arg_relation_outcome(").count(),
        // 9th probe: the `get` accessor branch checks the accessor's own
        // property type directly against the target property type, so it
        // asks the same boundary this contract exists to enforce (8th probe
        // was added by #16443 item 2: the computed-key branch decides from
        // the relation's `RelationFailure` rather than the kind of the key).
        9,
        "object literal property elaboration should route parameter-derived probes through call_arg_relation_outcome"
    );
    assert_eq!(
        helper.matches("return_relation_outcome(").count(),
        1,
        "object literal property callback return elaboration should route through return_relation_outcome"
    );
    assert!(
        !helper.contains("assign_relation_outcome("),
        "object literal property elaboration should not regress to generic assign relation outcomes"
    );
    assert!(
        !helper.contains("diagnostic_relation_boolean_guard("),
        "object literal property elaboration should not use raw relation guards"
    );
}

#[test]
fn call_elaboration_object_array_helpers_use_relation_outcome_boundary() {
    let source =
        fs::read_to_string("src/error_reporter/call_errors/elaboration_object_properties.rs")
            .expect("failed to read elaboration_object_properties.rs");

    let helper_start = source
        .find("fn object_literal_numeric_members_assign_to_mapped_target")
        .expect("missing numeric mapped object helper");
    let helpers = &source[helper_start..];

    assert_eq!(
        helpers.matches("call_arg_relation_outcome(").count(),
        // One probe moved out with `all_object_literal_properties_assignable_with_literals`
        // when it was split into `elaboration_object_literal_completeness.rs` to
        // hold the file under the 2000-line cap; that helper's boundary usage is
        // asserted against its own file below.
        5,
        "object/array call elaboration helper probes should route through call_arg_relation_outcome"
    );
    assert_eq!(
        helpers
            .matches("variable_initializer_relation_outcome(")
            .count(),
        1,
        "tail helper region should route the variable-initializer assignability probe through the named variable-initializer relation outcome"
    );
    assert!(
        !helpers.contains("assign_relation_outcome("),
        "object/array call elaboration helpers should not regress to generic assign relation outcomes"
    );
    assert!(
        !helpers.contains("diagnostic_relation_boolean_guard("),
        "object/array elaboration helpers should not use raw relation guards"
    );

    // The split-out object-literal-completeness helper carries the moved probe,
    // and must keep routing it through the same boundary rather than a raw guard.
    let completeness = fs::read_to_string(
        "src/error_reporter/call_errors/elaboration_object_literal_completeness.rs",
    )
    .expect("failed to read elaboration_object_literal_completeness.rs");
    assert_eq!(
        completeness.matches("call_arg_relation_outcome(").count(),
        1,
        "the split-out completeness helper should route its per-property probe through call_arg_relation_outcome"
    );
    assert!(
        !completeness.contains("assign_relation_outcome(")
            && !completeness.contains("diagnostic_relation_boolean_guard("),
        "the split-out completeness helper should not use generic or raw relation guards"
    );
}
