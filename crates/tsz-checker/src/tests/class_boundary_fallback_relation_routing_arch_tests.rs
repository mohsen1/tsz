use std::fs;
use std::path::Path;

#[test]
fn class_member_fallback_relations_use_relation_outcome_boundary() {
    let source = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/query_boundaries/class.rs"),
    )
    .expect("failed to read class.rs");

    let overload_helper = source
        .split("pub(crate) fn interface_overload_set_assignable")
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
        own_member_helper.contains("class_implements_whole_type_relation_outcome(source, target)")
            && own_member_helper.contains(".related"),
        "own member mismatch fallback should route standard relation truth through the class-implements whole-type RelationRequest"
    );
    assert!(
        !own_member_helper.contains("checker.assign_relation_outcome(source, target).related"),
        "own member mismatch fallback should not use generic assignment request routing"
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
fn interface_heritage_member_fallbacks_use_relation_outcome_boundaries() {
    let source = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/classes/interface_heritage_index_compat.rs"),
    )
    .expect("failed to read interface_heritage_index_compat.rs");

    let nongeneric_override_helper = source
        .split("pub(super) fn nongeneric_input_only_generic_override_is_valid")
        .nth(1)
        .and_then(|tail| tail.split("fn single_call_signature_return_type").next())
        .expect("failed to isolate nongeneric override fallback helper");
    assert!(
        nongeneric_override_helper.contains("bivariant_callbacks_relation_outcome(derived, base)")
            && nongeneric_override_helper.contains(".related"),
        "non-generic override fallback should route the bivariant probe through RelationOutcome"
    );
    assert!(
        nongeneric_override_helper
            .contains("no_erase_generics_relation_outcome(derived_return, base_return)")
            && nongeneric_override_helper.contains(".related"),
        "non-generic override fallback should route the no-erase return probe through RelationOutcome"
    );
    assert!(
        !nongeneric_override_helper.contains("is_assignable_to_bivariant(")
            && !nongeneric_override_helper.contains("is_assignable_to_no_erase_generics(")
            && !nongeneric_override_helper.contains("diagnostic_relation_boolean_guard_bivariant(")
            && !nongeneric_override_helper
                .contains("diagnostic_relation_boolean_guard_no_erase_generics("),
        "non-generic override fallback should not embed raw relation predicates or boolean guards"
    );

    let this_member_helper = source
        .split("pub(super) fn this_member_override_is_polymorphic")
        .nth(1)
        .and_then(|tail| tail.split("pub(super) fn type_base_def_id").next())
        .expect("failed to isolate polymorphic this fallback helper");
    assert!(
        this_member_helper.contains("no_erase_generics_relation_outcome(derived, base_member)")
            && this_member_helper.contains(".related"),
        "polymorphic-this fallback should route the no-erase probe through RelationOutcome"
    );
    assert!(
        !this_member_helper.contains("is_assignable_to_no_erase_generics(")
            && !this_member_helper.contains("diagnostic_relation_boolean_guard_no_erase_generics("),
        "polymorphic-this fallback should not embed a raw no-erase relation predicate or boolean guard"
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
        helper.contains("function_type_compatibility_relation_outcome(tp.type_id, sp.type_id)")
            && helper.contains(".related"),
        "coinductive return-cycle parameter compatibility should route through the function-type compatibility RelationRequest"
    );
    assert!(
        !helper.contains("assign_relation_outcome(tp.type_id, sp.type_id)"),
        "coinductive return-cycle parameter compatibility should not use generic assignment request routing"
    );
    assert!(
        !helper.contains("diagnostic_relation_boolean_guard"),
        "coinductive return-cycle helper should not use raw diagnostic boolean guards"
    );
}
