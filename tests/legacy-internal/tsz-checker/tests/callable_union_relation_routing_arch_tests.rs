use std::fs;

#[test]
fn callable_union_compatibility_uses_relation_outcome_boundary() {
    let source = fs::read_to_string("src/assignability/callable_union_relation.rs")
        .expect("failed to read callable_union_relation.rs");

    assert!(
        source.contains("callable_union_return_relation_outcome("),
        "callable-to-union return compatibility should use the callable-union return RelationOutcome"
    );
    assert!(
        source.contains("callable_union_parameter_relation_outcome("),
        "callable-to-union parameter compatibility should use the callable-union parameter RelationOutcome"
    );
    assert_eq!(
        source.matches("assign_relation_outcome").count(),
        0,
        "callable-to-union parameter and return compatibility should not use generic assign requests"
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
