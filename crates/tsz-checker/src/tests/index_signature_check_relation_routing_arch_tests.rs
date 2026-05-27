use std::fs;
use std::path::PathBuf;

#[test]
fn index_signature_checks_use_relation_outcome_boundary() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source = fs::read_to_string(
        manifest_dir.join("src/state/state_checking_members/index_signature_checks.rs"),
    )
    .expect("read index signature checking source");

    assert!(
        source.contains("assign_relation_outcome(key_type, previous_key_type)"),
        "template pattern index key compatibility should route through RelationOutcome"
    );
    assert!(
        source.contains("assign_relation_outcome(value_type, previous_value_type)"),
        "template pattern index value compatibility should route through RelationOutcome"
    );
    assert!(
        source.contains("assign_relation_outcome(number_idx.value_type, string_idx.value_type)"),
        "instance number-to-string index compatibility should route through RelationOutcome"
    );
    assert!(
        source.contains("assign_relation_outcome(static_num_type, static_str_type)"),
        "static number-to-string index compatibility should route through RelationOutcome"
    );
    assert!(
        !source.contains("diagnostic_relation_boolean_guard"),
        "index signature checking should not regress to raw boolean relation guards"
    );
}
