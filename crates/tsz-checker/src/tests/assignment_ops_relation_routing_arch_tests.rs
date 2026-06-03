use std::fs;

#[test]
fn assignment_ops_relation_checks_use_relation_outcome_boundary() {
    let source = fs::read_to_string("src/assignability/assignment_checker/assignment_ops.rs")
        .expect("failed to read assignment_ops.rs");
    let compact_source: String = source.chars().filter(|c| !c.is_whitespace()).collect();

    assert!(
        compact_source
            .contains("compound_assignment_relation_outcome(right_type,widened_left).related")
            && compact_source.contains(
                "generic_element_write_relation_outcome(source_type,generic_target).related"
            ),
        "assignment operation diagnostic relation checks should route through dedicated relation outcomes"
    );
    assert!(
        !compact_source.contains("assign_relation_outcome(right_type,widened_left)")
            && !compact_source.contains("assign_relation_outcome(source_type,generic_target)"),
        "assignment operation diagnostics should not use generic assign relation outcomes"
    );
    assert!(
        !compact_source.contains("diagnostic_relation_boolean_guard(right_type,widened_left)")
            && !compact_source
                .contains("diagnostic_relation_boolean_guard(source_type,generic_target)"),
        "assignment operation diagnostic relation checks should not use raw boolean guards"
    );
}

#[test]
fn assignment_ops_relation_outcomes_use_dedicated_requests() {
    let source = fs::read_to_string("src/assignability/relation_outcome_helpers.rs")
        .expect("failed to read relation_outcome_helpers.rs");

    assert!(
        source.contains("fn compound_assignment_relation_outcome(")
            && source.contains("RelationRequest::compound_assignment("),
        "compound assignment diagnostics should have a dedicated RelationRequest helper"
    );
    assert!(
        source.contains("fn generic_element_write_relation_outcome(")
            && source.contains("RelationRequest::generic_element_write("),
        "generic element-write diagnostics should have a dedicated RelationRequest helper"
    );
}
