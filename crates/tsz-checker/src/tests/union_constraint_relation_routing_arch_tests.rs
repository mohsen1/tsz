use std::fs;

#[test]
fn base_union_constraint_uses_relation_outcome_boundary() {
    let source = fs::read_to_string("src/checkers/generic_checker/union_constraint_helpers.rs")
        .expect("failed to read union_constraint_helpers.rs");

    assert_eq!(
        source.matches("assign_relation_outcome").count(),
        1,
        "base union member constraint checks should route the primary relation through assign_relation_outcome"
    );
    assert!(
        source.contains(".related"),
        "base union member constraint checks should use the relation outcome decision"
    );
    assert!(
        !source.contains("diagnostic_relation_boolean_guard"),
        "base union member constraint checks should not regress to the raw boolean relation guard"
    );
}
