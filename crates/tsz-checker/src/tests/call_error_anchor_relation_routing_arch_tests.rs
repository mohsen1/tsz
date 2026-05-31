use std::fs;
use std::path::Path;

#[test]
fn call_error_anchors_use_call_argument_relation_outcome_boundary() {
    let source = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/error_reporter/call_errors_anchors.rs"),
    )
    .expect("failed to read call error anchor source");

    assert!(
        source.matches("call_arg_relation_outcome(").count() >= 4,
        "call diagnostic anchors should route parameter mismatch probes through call_arg_relation_outcome"
    );
    assert!(
        !source.contains("assign_relation_outcome("),
        "call diagnostic anchors should not regress to generic assign relation outcomes"
    );
    assert!(
        !source.contains("diagnostic_relation_boolean_guard("),
        "call diagnostic anchors should not use raw diagnostic boolean relation probes"
    );
}
