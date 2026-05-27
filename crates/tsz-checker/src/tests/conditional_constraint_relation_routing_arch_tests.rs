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
    assert_eq!(
        function.matches("assign_relation_outcome").count(),
        2,
        "the raw and evaluated branch relations should both route through RelationOutcome"
    );
}
