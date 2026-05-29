use std::fs;
use std::path::Path;

#[test]
fn class_member_fallback_relations_use_relation_outcome_boundary() {
    let source = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/query_boundaries/class.rs"),
    )
    .expect("failed to read class.rs");

    let overload_helper = source
        .split("pub(crate) fn interface_overload_trailing_signature_assignable")
        .nth(1)
        .and_then(|tail| {
            tail.split("pub(crate) fn should_report_own_member_type_mismatch")
                .next()
        })
        .expect("failed to isolate interface overload fallback helper");
    assert!(
        overload_helper.contains("checker.assign_relation_outcome(source, target).related"),
        "interface overload fallback should route standard relation truth through assign_relation_outcome"
    );
    assert!(
        !overload_helper.contains("diagnostic_relation_boolean_guard(source, target)"),
        "interface overload fallback should not use the raw diagnostic boolean guard"
    );

    let own_member_helper = source
        .split("pub(crate) fn should_report_own_member_type_mismatch")
        .nth(1)
        .and_then(|tail| tail.split("fn is_coinductive_return_type_cycle").next())
        .expect("failed to isolate own member mismatch helper");
    assert!(
        own_member_helper.contains("checker.assign_relation_outcome(source, target).related"),
        "own member mismatch fallback should route standard relation truth through assign_relation_outcome"
    );
    assert!(
        !own_member_helper.contains("diagnostic_relation_boolean_guard(source, target)"),
        "own member mismatch fallback should not use the raw diagnostic boolean guard"
    );
}

#[test]
fn class_coinductive_return_cycle_param_check_uses_relation_outcome_boundary() {
    let source = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/query_boundaries/class.rs"),
    )
    .expect("failed to read class.rs");

    let helper = source
        .split("fn is_coinductive_return_type_cycle")
        .nth(1)
        .and_then(|tail| {
            tail.split("pub(crate) fn should_report_property_type_mismatch")
                .next()
        })
        .expect("failed to isolate coinductive return-cycle helper");

    assert!(
        helper.contains("assign_relation_outcome(tp.type_id, sp.type_id)")
            && helper.contains(".related"),
        "coinductive return-cycle parameter compatibility should route through RelationOutcome"
    );
    assert!(
        !helper.contains("diagnostic_relation_boolean_guard"),
        "coinductive return-cycle helper should not use raw diagnostic boolean guards"
    );
}
