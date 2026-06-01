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
        overload_helper
            .contains("interface_heritage_generic_method_relation_outcome(source, target)")
            && overload_helper.contains(".related"),
        "interface overload fallback should route standard relation truth through the interface-heritage generic-method RelationRequest"
    );
    assert!(
        overload_helper.contains("no_erase_generics_relation_outcome(source, target)")
            && overload_helper.contains(".related"),
        "interface overload strict generic compatibility should route through an outcome-shaped no-erase boundary"
    );
    assert!(
        !overload_helper.contains("checker.assign_relation_outcome(source, target).related"),
        "interface overload fallback should not use generic assignment request routing"
    );
    assert!(
        !overload_helper.contains("diagnostic_relation_boolean_guard(source, target)"),
        "interface overload fallback should not use the raw diagnostic boolean guard"
    );
    assert!(
        !overload_helper.contains("checker.is_assignable_to_no_erase_generics(source, target)"),
        "interface overload strict generic compatibility should not regress to raw no-erase assignability"
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
        own_member_helper.contains("no_erase_generics_relation_outcome(source, target)")
            && own_member_helper.contains(".related"),
        "own member mismatch strict generic compatibility should route through an outcome-shaped no-erase boundary"
    );
    assert!(
        !own_member_helper.contains("diagnostic_relation_boolean_guard(source, target)"),
        "own member mismatch fallback should not use the raw diagnostic boolean guard"
    );
    assert!(
        !own_member_helper.contains("checker.is_assignable_to_no_erase_generics(source, target)"),
        "own member mismatch strict generic compatibility should not regress to raw no-erase assignability"
    );
}

#[test]
fn class_boundary_no_erase_generic_probes_use_relation_outcome_boundary() {
    let source = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/query_boundaries/class.rs"),
    )
    .expect("failed to read class.rs");

    assert!(
        source.contains("no_erase_generics_relation_outcome(") && source.contains(".related"),
        "class member compatibility no-erase generic probes should route through RelationOutcome"
    );
    assert!(
        !source.contains("checker.is_assignable_to_no_erase_generics("),
        "class boundary should not call raw no-erase generic assignability directly"
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
