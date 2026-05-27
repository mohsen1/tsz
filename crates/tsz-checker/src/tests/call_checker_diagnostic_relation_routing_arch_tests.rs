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
        "assign_relation_outcome(actual_yield,expected_yield)",
        "assign_relation_outcome(expected_yield,actual_yield)",
        "assign_relation_outcome(actual_gen_return,expected_gen_return,)",
        "assign_relation_outcome(expected_gen_return,actual_gen_return,)",
        "assign_relation_outcome(expected_next,actual_next)",
        "assign_relation_outcome(actual_return,expected_return)",
    ] {
        assert!(
            compact_recovery_block.contains(relation),
            "generator recovery diagnostics should route {relation} through assign_relation_outcome"
        );
    }
    assert!(
        !recovery_block.contains("diagnostic_relation_boolean_guard("),
        "generator recovery diagnostics should not use raw diagnostic boolean relation probes"
    );
}
