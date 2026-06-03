use std::fs;
use std::path::Path;

#[test]
fn call_checker_generator_recovery_uses_relation_outcome_boundary() {
    let source = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/checkers/call_checker/diagnostics.rs"),
    )
    .expect("failed to read call checker diagnostics");

    let recovery_block = source
        .split("let is_generator_callback = func.asterisk_token;")
        .nth(1)
        .and_then(|tail| tail.split("let should_force_argument_mismatch").next())
        .expect("failed to isolate generator recovery diagnostics block");
    let compact_recovery_block: String = recovery_block.split_whitespace().collect();

    for relation in [
        "call_generator_yield_relation_outcome(actual_yield,expected_yield,)",
        "call_generator_yield_relation_outcome(expected_yield,actual_yield,)",
    ] {
        assert!(
            compact_recovery_block.contains(relation),
            "generator recovery diagnostics should route yield component {relation} through call_generator_yield_relation_outcome"
        );
    }
    assert!(
        !compact_recovery_block.contains("assign_relation_outcome(actual_yield,expected_yield)")
            && !compact_recovery_block
                .contains("assign_relation_outcome(expected_yield,actual_yield)"),
        "generator recovery diagnostics should not route yield components through the generic assignment request"
    );
    assert!(
        compact_recovery_block.contains("call_arg_relation_outcome(expected_next,actual_next)"),
        "generator recovery diagnostics should route TNext through call_arg_relation_outcome"
    );
    assert!(
        !compact_recovery_block.contains("assign_relation_outcome(expected_next,actual_next)"),
        "generator recovery diagnostics should not route TNext through the generic assignment request"
    );
    for relation in [
        "return_relation_outcome(actual_gen_return,expected_gen_return,)",
        "return_relation_outcome(expected_gen_return,actual_gen_return,)",
        "return_relation_outcome(actual_return,expected_return)",
    ] {
        assert!(
            compact_recovery_block.contains(relation),
            "generator recovery diagnostics should route callback return component {relation} through return_relation_outcome"
        );
    }
    assert!(
        !recovery_block.contains("diagnostic_relation_boolean_guard("),
        "generator recovery diagnostics should not use raw diagnostic boolean relation probes"
    );
}

#[test]
fn call_generator_yield_relation_outcome_uses_dedicated_request() {
    let source = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/assignability/relation_outcome_helpers.rs"),
    )
    .expect("failed to read relation outcome helpers");

    assert!(
        source.contains("fn call_generator_yield_relation_outcome(")
            && source.contains("RelationRequest::call_generator_yield("),
        "call checker generator yield probes should have a dedicated RelationRequest helper"
    );
}

#[test]
fn call_checker_adapter_uses_relation_outcome_boundary() {
    let source = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/checkers/call_checker/mod.rs"),
    )
    .expect("failed to read call checker adapter");

    let adapter_start = source
        .find("impl AssignabilityChecker for CheckerCallAssignabilityAdapter")
        .expect("missing call checker assignability adapter");
    let adapter_end = source[adapter_start..]
        .find("impl CheckerState")
        .map(|offset| adapter_start + offset)
        .expect("missing post-adapter CheckerState impl");
    let adapter = &source[adapter_start..adapter_end];

    assert!(
        adapter.contains("call_adapter_compatibility_relation_outcome(source, target)")
            && adapter.contains(".related"),
        "call checker adapter should route default compatibility probes through a dedicated RelationOutcome helper"
    );
    assert!(
        adapter.contains("call_adapter_identity_relation_outcome(a_resolved, b_resolved)")
            && adapter.contains("call_adapter_identity_relation_outcome(b_resolved, a_resolved)")
            && adapter.contains(".related"),
        "call checker adapter should route lazy identity fallback probes through a dedicated RelationOutcome helper"
    );
    assert!(
        adapter.matches(".related").count() >= 3,
        "call checker adapter should use relation outcome decisions"
    );
    assert!(
        !adapter.contains("assign_relation_outcome(source, target)")
            && !adapter.contains("assign_relation_outcome(a_resolved, b_resolved)")
            && !adapter.contains("assign_relation_outcome(b_resolved, a_resolved)"),
        "call checker adapter should not use the generic assignment request for call-adapter probes"
    );
    assert!(
        !adapter.contains("state.is_assignable_to(source, target)"),
        "call checker adapter default assignability should not regress to raw checker assignability"
    );
    assert!(
        !adapter.contains("state.is_assignable_to(a_resolved, b_resolved)"),
        "call checker adapter identity comparison should not regress to raw checker assignability"
    );
    assert!(
        adapter.contains("strict_relation_outcome(source, target)") && adapter.contains(".related"),
        "call checker adapter strict probes should route through RelationOutcome"
    );
    assert!(
        !adapter.contains("state.is_assignable_to_strict(source, target)"),
        "call checker adapter strict probes should not regress to raw strict assignability"
    );
    assert!(
        adapter.contains("bivariant_callbacks_relation_outcome(source, target)")
            && adapter.contains(".related"),
        "call checker adapter bivariant callback probes should route through RelationOutcome"
    );
    assert!(
        !adapter.contains("state.is_assignable_to_bivariant(source, target)"),
        "call checker adapter bivariant callback probes should not regress to raw bivariant assignability"
    );
}

#[test]
fn call_checker_adapter_relation_outcomes_use_dedicated_requests() {
    let source = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/assignability/relation_outcome_helpers.rs"),
    )
    .expect("failed to read relation outcome helpers");

    assert!(
        source.contains("fn call_adapter_compatibility_relation_outcome(")
            && source.contains("RelationRequest::call_adapter_compatibility("),
        "call checker adapter default compatibility probes should have a dedicated RelationRequest helper"
    );
    assert!(
        source.contains("fn call_adapter_identity_relation_outcome(")
            && source.contains("RelationRequest::call_adapter_identity("),
        "call checker adapter identity fallback probes should have a dedicated RelationRequest helper"
    );
}
