use std::fs;
use std::path::Path;

#[test]
fn assignability_type_comparability_uses_relation_outcome_boundary() {
    let comparability = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/assignability/assignability_diagnostics/type_comparability.rs"),
    )
    .expect("failed to read type_comparability.rs");
    let type_param_helpers = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/assignability/assignability_type_param_helpers.rs"),
    )
    .expect("failed to read assignability_type_param_helpers.rs");

    assert!(
        comparability.contains("bivariant_callbacks_relation_outcome(source, target)"),
        "bivariant method-compatibility diagnostics should use the shared bivariant RelationOutcome"
    );
    assert!(
        comparability.contains("type_comparability_relation_outcome("),
        "comparability probes should use the type-comparability RelationOutcome boundary"
    );
    assert_eq!(
        comparability.matches("assign_relation_outcome(").count(),
        0,
        "comparability probes should not use generic assign requests"
    );
    assert!(
        type_param_helpers.contains("type_comparability_relation_outcome("),
        "type-parameter comparability constraint probes should use the type-comparability RelationOutcome boundary"
    );
    assert_eq!(
        type_param_helpers
            .matches("assign_relation_outcome(")
            .count(),
        0,
        "type-parameter comparability constraint probes should not use generic assign requests"
    );
    assert!(
        !comparability.contains("diagnostic_relation_boolean_guard("),
        "comparability probes should not use the raw diagnostic boolean guard"
    );
    assert!(
        !comparability.contains("diagnostic_relation_boolean_guard_bivariant("),
        "bivariant comparability should not use the raw bivariant diagnostic boolean guard"
    );
    assert!(
        !type_param_helpers.contains("is_assignable_to("),
        "type-parameter comparability should not bypass CheckerState relation outcomes"
    );
}
