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

    assert!(
        helper.contains("no_weak_relation_outcome(ready_type_arg_constraint, ready_constraint)")
            && helper.contains(".related"),
        "generic constraint diagnostic suppression should route no-weak relation truth through RelationOutcome"
    );
    assert!(
        !helper.contains("diagnostic_relation_boolean_guard_no_weak_checks"),
        "generic constraint diagnostic suppression should not use the raw no-weak boolean guard"
    );
}
