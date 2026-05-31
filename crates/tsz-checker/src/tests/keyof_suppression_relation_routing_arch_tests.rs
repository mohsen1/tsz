use std::fs;
use std::path::Path;

#[test]
fn keyof_assignability_suppression_uses_relation_outcome_boundary() {
    let checker = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/assignability/assignability_checker.rs"),
    )
    .expect("failed to read assignability_checker.rs");
    let helpers = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/assignability/application_keyof_helpers.rs"),
    )
    .expect("failed to read application_keyof_helpers.rs");

    assert!(
        checker.contains("assign_relation_outcome(source, resolved_keyof)"),
        "keyof diagnostic suppression should route evaluated-keyof probes through RelationOutcome"
    );
    assert!(
        helpers
            .matches("assign_relation_outcome(member, target)")
            .count()
            >= 2,
        "keyof interface augmentation coverage should route member probes through RelationOutcome"
    );
    assert!(
        helpers.contains("assign_relation_outcome(source_arg, constraint)")
            && helpers.contains("assign_relation_outcome(constraint, source_arg)"),
        "application/keyof type-argument fallback should route mutual probes through RelationOutcome"
    );
    assert!(
        !checker.contains("ctx.types.is_assignable_to(source, resolved_keyof)")
            && !helpers.contains("ctx.types.is_assignable_to(member, target)")
            && !helpers.contains("is_assignable_to(source_arg, constraint)")
            && !helpers.contains("is_assignable_to(constraint, source_arg)"),
        "keyof diagnostic suppression should not bypass CheckerState relation boundaries"
    );
}
