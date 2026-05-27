use std::fs;

#[test]
fn syntax_instantiated_constraint_uses_relation_outcome_boundary() {
    let source =
        fs::read_to_string("src/checkers/generic_checker/constraint_syntax_instantiation.rs")
            .expect("failed to read constraint_syntax_instantiation.rs");

    assert_eq!(
        source.matches("assign_relation_outcome").count(),
        2,
        "syntax-instantiated constraint checks should route primary relations through assign_relation_outcome"
    );
    assert!(
        source.contains(".related"),
        "syntax-instantiated constraint checks should use the relation outcome decision"
    );
    assert!(
        !source.contains("diagnostic_relation_boolean_guard"),
        "syntax-instantiated constraint checks should not regress to the raw boolean relation guard"
    );
}
