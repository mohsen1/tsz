use std::fs;
use std::path::Path;

#[test]
fn assignability_type_comparability_uses_relation_outcome_boundary() {
    let source = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/assignability/assignability_diagnostics/type_comparability.rs"),
    )
    .expect("failed to read type_comparability.rs");

    assert!(
        source.contains("bivariant_callbacks_relation_outcome(source, target)"),
        "bivariant method-compatibility diagnostics should use the shared bivariant RelationOutcome"
    );
    assert!(
        source.matches("assign_relation_outcome(").count() >= 12,
        "comparability probes should route bidirectional assignability through RelationOutcome"
    );
    assert!(
        !source.contains("diagnostic_relation_boolean_guard("),
        "comparability probes should not use the raw diagnostic boolean guard"
    );
    assert!(
        !source.contains("diagnostic_relation_boolean_guard_bivariant("),
        "bivariant comparability should not use the raw bivariant diagnostic boolean guard"
    );
}
