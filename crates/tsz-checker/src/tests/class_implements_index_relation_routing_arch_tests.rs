use std::fs;
use std::path::PathBuf;

#[test]
fn class_implements_index_signatures_use_relation_outcome_boundary() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source =
        fs::read_to_string(manifest_dir.join("src/classes/class_implements_checker/core.rs"))
            .expect("read class implements checker source");

    let helper = source
        .split("fn class_index_signatures_satisfy_interface")
        .nth(1)
        .expect("find class implements index-signature helper")
        .split("fn class_member_name_is_computed")
        .next()
        .expect("slice helper body before next helper");

    assert_eq!(
        helper
            .matches("assign_relation_outcome(source_index.value_type, target_index.value_type)")
            .count(),
        2,
        "class implements string and number index signature checks should use relation outcomes"
    );
    assert!(
        helper.contains(".related"),
        "class implements index signature checks should inspect the relation outcome"
    );
    assert!(
        !helper.contains("diagnostic_relation_boolean_guard"),
        "class implements index signature checks should not regress to raw boolean relation guards"
    );
}
