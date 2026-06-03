use std::fs;

#[test]
fn merged_interface_constraints_use_relation_outcome_boundary() {
    let src = fs::read_to_string("src/checkers/generic_checker/merged_interface_constraints.rs")
        .expect("failed to read merged interface constraint helper");

    assert!(
        !src.contains("assign_relation_outcome("),
        "merged interface constraints should route relation checks through named RelationRequests"
    );
    assert_eq!(
        src.matches("merged_interface_constraint_relation_outcome(")
            .count(),
        2,
        "candidate and evaluated candidate checks should use the merged-interface request helper"
    );
    assert!(
        !src.contains("diagnostic_relation_boolean_guard("),
        "merged interface constraints must not use raw diagnostic relation boolean guards"
    );
}
