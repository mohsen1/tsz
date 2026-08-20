use std::fs;
use std::path::Path;

#[test]
fn infer_result_check_constraint_uses_relation_outcome_boundary() {
    let source_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/checkers/generic_checker/infer_conditional_constraints.rs");
    let source =
        fs::read_to_string(&source_path).expect("read infer conditional constraint helper source");

    let function_start = source
        .find("pub(super) fn infer_result_satisfies_via_check_constraint")
        .expect("find infer-result check-constraint helper");
    let rest = &source[function_start..];
    let function_end = rest
        .find("\n    fn infer_result_satisfies_via_mapped_key_subset")
        .expect("find next helper");
    let function = &rest[..function_end];

    assert!(
        !function.contains("diagnostic_relation_boolean_guard"),
        "infer-result check-constraint relation decisions must use the shared relation outcome boundary"
    );
    assert!(
        !function.contains("assign_relation_outcome"),
        "infer-result check-constraint relation decisions should route through named RelationRequests"
    );
    assert_eq!(
        function
            .matches("infer_result_constraint_relation_outcome(")
            .count(),
        2,
        "the evaluated and raw restricted relations should both route through the infer-result constraint request helper"
    );
}

#[test]
fn infer_result_referenced_constraints_use_relation_outcome_boundary() {
    let source_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/checkers/generic_checker/infer_conditional_constraints.rs");
    let source =
        fs::read_to_string(&source_path).expect("read infer conditional constraint helper source");

    let function_start = source
        .find("pub(super) fn infer_result_satisfies_via_referenced_constraints")
        .expect("find referenced-constraint helper");
    let rest = &source[function_start..];
    let function_end = rest
        .find("\n    pub(super) fn type_arg_satisfies_via_hidden_infer_constraints")
        .expect("find hidden-infer helper");
    let function = &rest[..function_end];

    assert!(
        !function.contains("diagnostic_relation_boolean_guard"),
        "infer-result referenced-constraint relation decisions must use the \
         shared relation outcome boundary"
    );
    assert!(
        !function.contains("assign_relation_outcome"),
        "infer-result referenced-constraint relation decisions should route through named RelationRequests"
    );
    assert_eq!(
        function
            .matches("infer_result_constraint_relation_outcome(")
            .count(),
        2,
        "the evaluated and raw restricted relations should both route through the infer-result constraint request helper"
    );
}

#[test]
fn hidden_infer_constraints_use_relation_outcome_boundary() {
    let source_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/checkers/generic_checker/infer_conditional_constraints.rs");
    let source =
        fs::read_to_string(&source_path).expect("read infer conditional constraint helper source");

    let function_start = source
        .find("pub(super) fn type_arg_satisfies_via_hidden_infer_constraints")
        .expect("find hidden-infer helper");
    let rest = &source[function_start..];
    let function_end = rest
        .find("\n    pub(super) fn infer_result_satisfies_array_like_constraint")
        .expect("find array-like helper");
    let function = &rest[..function_end];

    assert!(
        !function.contains("diagnostic_relation_boolean_guard"),
        "hidden-infer constraint relation decisions must use the shared relation \
         outcome boundary"
    );
    assert!(
        !function.contains("assign_relation_outcome"),
        "hidden-infer constraint relation decisions should route through named RelationRequests"
    );
    assert_eq!(
        function
            .matches("infer_result_constraint_relation_outcome(")
            .count(),
        1,
        "the restricted relation should route through the infer-result constraint request helper"
    );
}

#[test]
fn infer_conditional_alias_element_constraints_use_named_no_weak_relation_outcome_boundary() {
    let source_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/checkers/generic_checker/infer_conditional_constraints.rs");
    let source =
        fs::read_to_string(&source_path).expect("read infer conditional constraint helper source");

    let alias_start = source
        .find("pub(crate) fn array_element_infer_alias_satisfies_constraint")
        .expect("find alias element infer helper");
    let alias_rest = &source[alias_start..];
    let alias_end = alias_rest
        .find("\n    fn instantiated_alias_body_candidates")
        .expect("find next alias helper");
    let alias_helper = &alias_rest[..alias_end];
    assert!(
        alias_helper.contains(
            "infer_result_constraint_no_weak_relation_outcome(candidate, inst_constraint)"
        ) && alias_helper.contains(".related"),
        "alias element infer constraints should route no-weak relation truth through the named infer-result constraint fallback"
    );
    assert!(
        !alias_helper.contains("self.no_weak_relation_outcome(")
            && !alias_helper.contains("diagnostic_relation_boolean_guard_no_weak_checks"),
        "alias element infer constraints should not use the raw no-weak relation helpers"
    );

    let array_start = source
        .find("fn conditional_array_element_infer_satisfies_constraint")
        .expect("find conditional array element infer helper");
    let array_rest = &source[array_start..];
    let array_end = array_rest
        .find("\n    fn array_like_pattern_extracts_infer")
        .expect("find next array helper");
    let array_helper = &array_rest[..array_end];
    assert!(
        array_helper
            .contains("infer_result_constraint_no_weak_relation_outcome(element, inst_constraint)")
            && array_helper.contains(".related"),
        "conditional array element infer constraints should route no-weak relation truth through the named infer-result constraint fallback"
    );
    assert!(
        !array_helper.contains("self.no_weak_relation_outcome(")
            && !array_helper.contains("diagnostic_relation_boolean_guard_no_weak_checks"),
        "conditional array element infer constraints should not use the raw no-weak relation helpers"
    );
}
