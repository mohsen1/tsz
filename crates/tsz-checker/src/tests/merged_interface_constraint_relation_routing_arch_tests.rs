use std::fs;

#[test]
fn merged_interface_constraints_use_relation_outcome_boundary() {
    let src = fs::read_to_string("src/checkers/generic_checker/merged_interface_constraints.rs")
        .expect("failed to read merged interface constraint helper");

    let assign_outcome_calls = src.matches("assign_relation_outcome(").count();
    assert_eq!(
        assign_outcome_calls, 2,
        "merged interface constraints should route both primary relation checks through the shared outcome boundary"
    );
    assert!(
        src.contains(".assign_relation_outcome(candidate, required)\n                    .related")
            || src.contains("assign_relation_outcome(candidate, required).related"),
        "candidate relation check should consume RelationOutcome.related"
    );
    assert!(
        src.contains(
            ".assign_relation_outcome(candidate_evaluated, required)\n                    .related"
        ) || src.contains("assign_relation_outcome(candidate_evaluated, required).related"),
        "evaluated candidate relation check should consume RelationOutcome.related"
    );
    assert!(
        !src.contains("diagnostic_relation_boolean_guard("),
        "merged interface constraints must not use raw diagnostic relation boolean guards"
    );
}
