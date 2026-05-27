use std::fs;
use std::path::Path;

#[test]
fn indexed_access_constraint_uses_relation_outcome_boundary() {
    let source_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/checkers/generic_checker/constraint_indexed_access_helpers.rs");
    let source = fs::read_to_string(&source_path).expect("read indexed-access helper source");

    let function_start = source
        .find("pub(super) fn constraint_check_indexed_access_value_type")
        .expect("find indexed-access constraint helper");
    let rest = &source[function_start..];
    let function_end = rest
        .find("\n    pub(super) fn concrete_indexed_access_property_union")
        .expect("find next helper");
    let function = &rest[..function_end];

    assert!(
        !function.contains("diagnostic_relation_boolean_guard"),
        "indexed-access key-space relation decisions must use the shared relation outcome boundary"
    );
    assert_eq!(
        function.matches("assign_relation_outcome").count(),
        1,
        "the keyed-object to object-keys relation should route through RelationOutcome"
    );
}
