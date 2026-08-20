use std::fs;

#[test]
fn base_union_constraint_uses_relation_outcome_boundary() {
    let source = fs::read_to_string("src/checkers/generic_checker/union_constraint_helpers.rs")
        .expect("failed to read union_constraint_helpers.rs");

    assert!(
        !source.contains("assign_relation_outcome"),
        "base union member constraint checks should route relation probes through named RelationRequests"
    );
    assert!(
        source.contains("union_constraint_member_relation_outcome(member, constraint)"),
        "base union member constraint checks should use the union-constraint request helper"
    );
    assert!(
        !source.contains("diagnostic_relation_boolean_guard"),
        "base union member constraint checks should not regress to the raw boolean relation guard"
    );
}
