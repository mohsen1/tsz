use std::fs;
use std::path::Path;

#[test]
fn generic_constraint_diagnostic_suppression_uses_no_weak_relation_outcome_boundary() {
    let source = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/error_reporter/generics.rs"),
    )
    .expect("failed to read generics.rs");

    let helper_start = source
        .find("pub fn error_type_constraint_not_satisfied")
        .expect("find generic constraint diagnostic helper");
    let helper_end = source[helper_start..]
        .find("\n    /// True when `type_arg`")
        .expect("find next generic diagnostic helper")
        + helper_start;
    let helper = &source[helper_start..helper_end];

    assert_eq!(
        helper
            .matches("type_arg_constraint_no_weak_relation_outcome(")
            .count(),
        1,
        "generic constraint diagnostic suppression should route no-weak relation truth through the named type-argument constraint fallback"
    );
    assert!(helper.contains(".related"));
    assert!(
        !helper.contains("self.no_weak_relation_outcome(")
            && !helper.contains("diagnostic_relation_boolean_guard_no_weak_checks"),
        "generic constraint diagnostic suppression should not use the raw no-weak relation helpers"
    );
}

#[test]
fn generic_constraint_property_carveout_uses_dedicated_relation_request() {
    let source = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/error_reporter/generics.rs"),
    )
    .expect("failed to read generics.rs");
    let relation_helpers = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/assignability/relation_outcome_helpers.rs"),
    )
    .expect("failed to read relation_outcome_helpers.rs");

    let helper_start = source
        .find("pub(crate) fn indexed_access_into_object_uniformly_satisfies_constraint")
        .expect("find indexed-access constraint object-shape helper");
    let helper_end = source[helper_start..]
        .find("\n    /// Report TS2635")
        .expect("find next generic diagnostic helper")
        + helper_start;
    let helper = &source[helper_start..helper_end];

    assert_eq!(
        helper
            .matches("generic_constraint_property_relation_outcome(")
            .count(),
        4,
        "generic constraint property carve-out should route all property probes through its dedicated relation outcome"
    );
    assert!(
        !helper.contains("assign_relation_outcome("),
        "generic constraint property carve-out should not use the generic assign request"
    );
    assert!(
        relation_helpers.contains("fn generic_constraint_property_relation_outcome(")
            && relation_helpers.contains("RelationRequest::generic_constraint_property("),
        "generic constraint property relation helper should construct its dedicated request"
    );
}
