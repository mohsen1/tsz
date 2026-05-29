use std::fs;
use std::path::Path;

#[test]
fn await_thenable_this_validation_uses_relation_outcome_boundary() {
    let source = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/checkers/promise_checker.rs"),
    )
    .expect("failed to read promise_checker.rs");

    let helper = source
        .split("fn extract_awaited_type_from_valid_thenable")
        .nth(1)
        .and_then(|rest| rest.split("let awaited_type =").next())
        .expect("failed to isolate thenable await helper");

    assert!(
        helper.contains("assign_relation_outcome(type_id, expected_this).related"),
        "await thenable this-type validation should route through assign_relation_outcome"
    );
    assert!(
        !helper.contains("diagnostic_relation_boolean_guard(type_id, expected_this)"),
        "await thenable this-type validation should not use the raw boolean relation guard"
    );
}
