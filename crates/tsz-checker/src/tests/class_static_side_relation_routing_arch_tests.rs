use std::fs;
use std::path::Path;

#[test]
fn namespace_merged_static_side_mismatch_uses_relation_outcome_boundary() {
    let source = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/classes/class_static_side_helpers.rs"),
    )
    .expect("failed to read class_static_side_helpers.rs");

    assert!(
        source.contains("assign_relation_outcome(derived_ctor_type, base_ctor_type)"),
        "namespace-merged static-side diagnostics should route the relation through assign_relation_outcome"
    );
    assert!(
        !source.contains("diagnostic_relation_boolean_guard(derived_ctor_type, base_ctor_type)"),
        "namespace-merged static-side diagnostics should not pre-gate with a raw boolean relation"
    );
}
