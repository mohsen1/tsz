use std::fs;
use std::path::Path;

#[test]
fn generic_argument_suppression_uses_env_relation_outcome_boundary() {
    let source = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/assignability/assignability_diagnostics/generic_argument_suppression.rs"),
    )
    .expect("failed to read generic_argument_suppression.rs");

    assert!(
        source.matches("assign_relation_outcome_with_env(").count() >= 5,
        "generic argument suppression should route env-aware probes through RelationOutcome"
    );
    assert!(
        !source.contains("diagnostic_relation_boolean_guard_with_env("),
        "generic argument suppression should not use the raw env-aware boolean relation guard"
    );
}
