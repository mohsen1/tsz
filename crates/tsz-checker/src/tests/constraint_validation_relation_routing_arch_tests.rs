use std::fs;
use std::path::Path;

#[test]
fn generic_constraint_validation_no_weak_checks_use_relation_outcome_boundary() {
    let source = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/checkers/generic_checker/constraint_validation.rs"),
    )
    .expect("failed to read constraint_validation.rs");

    assert!(
        source.matches("no_weak_relation_outcome(").count() >= 3,
        "generic constraint validation should route no-weak relation probes through RelationOutcome"
    );
    assert!(
        !source.contains("diagnostic_relation_boolean_guard_no_weak_checks("),
        "generic constraint validation should not use the raw no-weak boolean guard"
    );
}

#[test]
fn generic_constraint_validation_regular_checks_use_relation_outcome_boundary() {
    let source = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/checkers/generic_checker/constraint_validation.rs"),
    )
    .expect("failed to read constraint_validation.rs");

    assert!(
        source.matches("assign_relation_outcome(").count() >= 19,
        "generic constraint validation should route regular relation probes through RelationOutcome"
    );
    assert!(
        !source.contains("diagnostic_relation_boolean_guard("),
        "generic constraint validation should not use the raw boolean relation guard"
    );
}
