use std::fs;
use std::path::Path;

#[test]
fn array_like_constraint_uses_relation_outcome_boundary() {
    let source_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/checkers/generic_checker/array_like_constraint_helpers.rs");
    let source = fs::read_to_string(&source_path).expect("read array-like helper source");

    let function_start = source
        .find("pub(crate) fn satisfies_array_like_constraint")
        .expect("find array-like constraint helper");
    let rest = &source[function_start..];
    let function_end = rest
        .find("\n    fn tuple_constraint_accepts_array_like_source")
        .expect("find next helper");
    let function = &rest[..function_end];

    assert!(
        !function.contains("diagnostic_relation_boolean_guard"),
        "array-like element relation decisions must use the shared relation outcome boundary"
    );
    assert_eq!(
        function.matches("assign_relation_outcome").count(),
        1,
        "the direct source-element to target-element relation should route through RelationOutcome"
    );
}
