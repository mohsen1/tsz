use std::fs;

#[test]
fn callable_union_compatibility_uses_relation_outcome_boundary() {
    let source = fs::read_to_string("src/assignability/callable_union_relation.rs")
        .expect("failed to read callable_union_relation.rs");

    assert_eq!(
        source.matches("assign_relation_outcome").count(),
        2,
        "callable-to-union parameter and return compatibility should route through assign_relation_outcome"
    );
    assert!(
        source.contains(".related"),
        "callable-to-union compatibility should use the relation outcome decision"
    );
    assert!(
        !source.contains("diagnostic_relation_boolean_guard"),
        "callable-to-union compatibility should not regress to the raw boolean relation guard"
    );
}
