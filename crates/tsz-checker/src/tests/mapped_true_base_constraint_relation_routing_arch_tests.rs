use std::fs;
use std::path::Path;

#[test]
fn mapped_true_base_constraint_uses_relation_outcome_boundary() {
    let source_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/checkers/generic_checker/mapped_constraint_helpers.rs");
    let source = fs::read_to_string(&source_path).expect("read mapped constraint helper source");

    let function_start = source
        .find("pub(super) fn conditional_true_type_parameter_base_satisfies_constraint")
        .expect("find conditional true base constraint helper");
    let rest = &source[function_start..];
    let function_end = rest
        .find("\n    pub(super) fn constraint_check_base_type")
        .expect("find next helper");
    let function = &rest[..function_end];

    assert!(
        !function.contains("diagnostic_relation_boolean_guard"),
        "conditional true-base constraint relation decisions must use the shared relation outcome boundary"
    );
    assert_eq!(
        function.matches("assign_relation_outcome").count(),
        2,
        "evaluated and resolved true-base constraint relations should route through RelationOutcome"
    );
}
