use std::fs;
use std::path::PathBuf;

#[test]
fn class_extends_index_signatures_use_relation_outcome_boundary() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source = fs::read_to_string(manifest_dir.join("src/classes/class_checker_compat.rs"))
        .expect("read class checker compatibility source");

    let helper = source
        .split("// Check string index signature compatibility")
        .nth(1)
        .expect("find class extends index-signature compatibility block")
        .split("pub(crate) fn check_interface_extension_compatibility")
        .next()
        .expect("slice class extends index-signature compatibility block");

    assert_eq!(
        helper
            .matches("assign_relation_outcome(derived_type, base_type_instantiated)")
            .count(),
        2,
        "class extends string and number index signature checks should use relation outcomes"
    );
    assert!(
        helper.contains(".related"),
        "class extends index signature checks should inspect the relation outcome"
    );
    assert!(
        !helper.contains("diagnostic_relation_boolean_guard"),
        "class extends index signature checks should not regress to raw boolean relation guards"
    );
}
