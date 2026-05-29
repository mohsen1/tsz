use std::fs;

#[test]
fn explicit_alias_constraint_uses_relation_outcome_boundary() {
    let source =
        fs::read_to_string("src/checkers/generic_checker/explicit_alias_constraint_helpers.rs")
            .expect("failed to read explicit_alias_constraint_helpers.rs");

    assert_eq!(
        source.matches("assign_relation_outcome").count(),
        1,
        "explicit alias constraint compatibility should route through assign_relation_outcome"
    );
    assert!(
        source.contains(".related"),
        "explicit alias constraint compatibility should use the relation outcome decision"
    );
    assert!(
        !source.contains("diagnostic_relation_boolean_guard"),
        "explicit alias constraint compatibility should not regress to the raw boolean relation guard"
    );
}
