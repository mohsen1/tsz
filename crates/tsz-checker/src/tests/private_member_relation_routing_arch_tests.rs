use std::fs;
use std::path::Path;

#[test]
fn private_member_access_compatibility_uses_relation_outcome_boundary() {
    let source = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/state/type_analysis/computed_helpers_private.rs"),
    )
    .expect("failed to read computed_helpers_private.rs");

    assert!(
        source.matches("assign_relation_outcome(").count() >= 4,
        "private member compatibility should route fallback relation checks through assign_relation_outcome"
    );
    assert!(
        !source.contains("diagnostic_relation_boolean_guard("),
        "private member compatibility should not use the raw boolean relation guard"
    );
}
