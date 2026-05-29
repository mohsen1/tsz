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
    assert_eq!(
        function.matches("assign_relation_outcome").count(),
        2,
        "the evaluated and raw restricted relations should both route through RelationOutcome"
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
    assert_eq!(
        function.matches("assign_relation_outcome").count(),
        2,
        "the evaluated and raw restricted relations should both route through RelationOutcome"
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
    assert_eq!(
        function.matches("assign_relation_outcome").count(),
        1,
        "the restricted relation should route through RelationOutcome"
    );
}
