use std::fs;

#[test]
fn generic_checker_constraint_paths_use_relation_outcome_boundary() {
    let source = fs::read_to_string("src/checkers/generic_checker/mod.rs")
        .expect("failed to read generic_checker/mod.rs");

    let conditional_start = source
        .find("fn type_argument_is_narrowed_by_conditional_true_branch")
        .expect("missing conditional true-branch helper");
    let conditional_end = conditional_start
        + source[conditional_start..]
            .find("fn type_node_contains_reference")
            .expect("missing next generic checker helper");
    let conditional_helper = &source[conditional_start..conditional_end];

    assert!(
        !conditional_helper.contains("diagnostic_relation_boolean_guard("),
        "conditional true-branch constraint checks should not use raw boolean relation guards"
    );
    assert!(
        conditional_helper
            .matches("assign_relation_outcome(")
            .count()
            >= 5,
        "conditional true-branch constraint checks should route relation probes through RelationOutcome"
    );

    let args_start = source
        .find("pub(crate) fn validate_jsdoc_type_reference_type_arguments_against_params")
        .expect("missing JSDoc type argument constraint helper");
    let args_end = args_start
        + source[args_start..]
            .find("/// Validate explicit type arguments against their constraints for new expressions.")
            .expect("missing next generic checker helper");
    let args_helper = &source[args_start..args_end];

    assert!(
        !args_helper.contains("diagnostic_relation_boolean_guard("),
        "type argument constraint checks should not use raw boolean relation guards"
    );
    assert!(
        args_helper.matches("assign_relation_outcome(").count() >= 3,
        "type argument constraint checks should route relation probes through RelationOutcome"
    );
}
