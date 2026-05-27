use std::fs;

#[test]
fn jsdoc_extends_object_constraints_use_relation_outcome_boundary() {
    let source = fs::read_to_string("src/classes/class_implements_checker/jsdoc_heritage.rs")
        .expect("failed to read jsdoc_heritage.rs");
    let start = source
        .find("fn jsdoc_extends_object_violates_constraint")
        .expect("missing jsdoc_extends_object_violates_constraint helper");
    let end = start
        + source[start..]
            .find("fn split_jsdoc_type_arguments_with_offsets")
            .expect("missing split_jsdoc_type_arguments_with_offsets helper");
    let helper = &source[start..end];

    assert_eq!(
        helper.matches("assign_relation_outcome").count(),
        1,
        "JSDoc heritage object constraint compatibility should route through assign_relation_outcome"
    );
    assert!(
        helper.contains(".related"),
        "JSDoc heritage object constraint compatibility should use the relation outcome decision"
    );
    assert!(
        !helper.contains("diagnostic_relation_boolean_guard"),
        "JSDoc heritage object constraint compatibility should not regress to the raw boolean relation guard"
    );
}
