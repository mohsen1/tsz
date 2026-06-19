use std::fs;
use std::path::Path;

#[test]
fn keyof_assignability_suppression_uses_relation_outcome_boundary() {
    // The `should_suppress_assignability_diagnostic` cluster (including the
    // evaluated-keyof `keyof_diagnostic_suppression_relation_outcome` probe) was
    // extracted from `assignability_checker.rs` into the
    // `query_boundaries/assignability_suppression.rs` module to stay under the
    // 2000-LOC architecture cap; read it so the routing contract holds wherever
    // the cluster physically resides.
    let checker = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/query_boundaries/assignability_suppression.rs"),
    )
    .expect("failed to read query_boundaries/assignability_suppression.rs");
    let helpers = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/assignability/application_keyof_helpers.rs"),
    )
    .expect("failed to read application_keyof_helpers.rs");
    let checker_compact: String = checker.chars().filter(|c| !c.is_whitespace()).collect();
    let helpers_compact: String = helpers.chars().filter(|c| !c.is_whitespace()).collect();

    assert!(
        checker_compact
            .contains("keyof_diagnostic_suppression_relation_outcome(source,resolved_keyof)"),
        "keyof diagnostic suppression should route evaluated-keyof probes through a dedicated RelationOutcome"
    );
    assert!(
        helpers_compact
            .matches("keyof_diagnostic_suppression_relation_outcome(member,target)")
            .count()
            >= 2,
        "keyof interface augmentation coverage should route member probes through a dedicated RelationOutcome"
    );
    assert!(
        helpers_compact
            .contains("keyof_diagnostic_suppression_relation_outcome(source_arg,constraint,)")
            && helpers_compact
                .contains("keyof_diagnostic_suppression_relation_outcome(constraint,source_arg,)"),
        "application/keyof type-argument fallback should route mutual probes through a dedicated RelationOutcome"
    );
    assert!(
        !checker_compact.contains("assign_relation_outcome(source,resolved_keyof)")
            && !helpers_compact.contains("assign_relation_outcome(member,target)")
            && !helpers_compact.contains("assign_relation_outcome(source_arg,constraint)")
            && !helpers_compact.contains("assign_relation_outcome(constraint,source_arg)"),
        "keyof diagnostic suppression should not use generic assignment relation outcomes"
    );
    assert!(
        !checker.contains("ctx.types.is_assignable_to(source, resolved_keyof)")
            && !helpers.contains("ctx.types.is_assignable_to(member, target)")
            && !helpers.contains("is_assignable_to(source_arg, constraint)")
            && !helpers.contains("is_assignable_to(constraint, source_arg)"),
        "keyof diagnostic suppression should not bypass CheckerState relation boundaries"
    );
}
