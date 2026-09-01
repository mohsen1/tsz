use std::fs;

#[test]
fn flow_usage_invalid_narrowing_guard_uses_no_weak_relation_outcome() {
    let source = fs::read_to_string("src/flow/flow_analysis/usage.rs")
        .expect("failed to read flow usage source");

    assert!(
        source.contains("no_weak_relation_outcome(narrowed_type, declared_type)"),
        "flow invalid-narrowing guard should route no-weak relation truth through RelationOutcome"
    );
    assert!(
        !source.contains("is_assignable_to_no_weak_checks(narrowed_type, declared_type)"),
        "flow invalid-narrowing guard should not call the raw no-weak boolean relation helper"
    );
}
