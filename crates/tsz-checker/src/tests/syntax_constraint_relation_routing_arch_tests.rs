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

#[test]
fn mapped_key_constraint_checks_use_relation_outcome_boundary() {
    let source = fs::read_to_string("src/types/type_checking/core_statement_checks.rs")
        .expect("failed to read core_statement_checks.rs");
    let deferred_start = source
        .find("let is_deferred_index_access")
        .expect("find deferred indexed-access key constraint check");
    let invalid_start = source
        .find("let has_invalid_index_constraint")
        .expect("find pre-evaluation indexed-access key constraint check");
    let next_section = source
        .find("// Check if the constraint contains a self-reference")
        .expect("find next mapped constraint section");
    let deferred_block = &source[deferred_start..invalid_start];
    let invalid_block = &source[invalid_start..next_section];

    assert!(
        deferred_block.contains("assign_relation_outcome(") && deferred_block.contains(".related"),
        "deferred mapped key constraint checks should route through RelationOutcome"
    );
    assert!(
        invalid_block.contains("assign_relation_outcome(") && invalid_block.contains(".related"),
        "pre-evaluation mapped key constraint checks should route through RelationOutcome"
    );
    assert!(
        !deferred_block.contains("diagnostic_relation_boolean_guard"),
        "deferred mapped key constraint checks should not use raw diagnostic boolean relation guards"
    );
    assert!(
        !invalid_block.contains("diagnostic_relation_boolean_guard"),
        "pre-evaluation mapped key constraint checks should not use raw diagnostic boolean relation guards"
    );
}
