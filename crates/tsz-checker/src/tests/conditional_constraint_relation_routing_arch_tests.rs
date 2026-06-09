use std::fs;
use std::path::Path;

#[test]
fn conditional_result_branches_use_relation_outcome_boundary() {
    let source_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/checkers/generic_checker/conditional_constraint_helpers.rs");
    let source =
        fs::read_to_string(&source_path).expect("read conditional constraint helper source");

    let function_start = source
        .find("pub(crate) fn conditional_result_branches_satisfy_constraint")
        .expect("find conditional branch constraint helper");
    let rest = &source[function_start..];
    let function_end = rest
        .find("\n    fn type_alias_application_conditional_components")
        .expect("find next helper");
    let function = &rest[..function_end];

    assert!(
        !function.contains("diagnostic_relation_boolean_guard"),
        "conditional branch relation decisions must use the shared relation outcome boundary"
    );
    assert!(
        !function.contains("assign_relation_outcome"),
        "conditional branch relation decisions should route through named RelationRequests"
    );
    assert_eq!(
        function
            .matches("conditional_constraint_component_relation_outcome(")
            .count(),
        7,
        "conditional branch, extends fallback, indexed-object-map branch, and tuple-element relations should route through the conditional constraint component request helper"
    );
}

#[test]
fn conditional_filter_helper_uses_relation_outcome_boundary() {
    let source_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/checkers/generic_checker/conditional_constraint_helpers.rs");
    let source =
        fs::read_to_string(&source_path).expect("read conditional constraint helper source");

    let function_start = source
        .find("pub(crate) fn type_alias_application_filters_to_constraint")
        .expect("find conditional filter helper");
    let function = &source[function_start..];

    assert!(
        !function.contains("diagnostic_relation_boolean_guard"),
        "conditional filter relation decisions must use the shared relation outcome boundary"
    );
    assert!(
        !function.contains("assign_relation_outcome"),
        "conditional filter relation decisions should route through named RelationRequests"
    );
    assert_eq!(
        function
            .matches("conditional_constraint_component_relation_outcome(")
            .count(),
        4,
        "the true and extends relation probes should route through the conditional constraint component request helper"
    );
}

#[test]
fn indexed_object_map_branch_uses_structural_value_proof_before_evaluation() {
    let source_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/checkers/generic_checker/conditional_constraint_helpers.rs");
    let source =
        fs::read_to_string(&source_path).expect("read conditional constraint helper source");

    let function_start = source
        .find("fn indexed_object_map_branch_satisfies_constraint_uncached")
        .expect("find indexed object-map branch helper");
    let rest = &source[function_start..];
    let function_end = rest
        .find("\n    fn tuple_value_satisfies_tuple_constraint")
        .expect("find next helper");
    let function = &rest[..function_end];

    let structural_probe = function
        .find("indexed_object_map_value_structurally_satisfies_constraint")
        .expect("indexed object-map values should have a structural proof");
    let eager_evaluation = function
        .find("let value_evaluated = self.evaluate_type_for_assignability(value)")
        .expect("find eager value evaluation");

    assert!(
        structural_probe < eager_evaluation,
        "indexed object-map branches should prove values of the form never or X & C \
         structurally before evaluating heavy branch values"
    );
}
