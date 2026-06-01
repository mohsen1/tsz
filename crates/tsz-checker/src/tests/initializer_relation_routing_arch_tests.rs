use std::{fs, path::PathBuf};

#[test]
fn variable_initializer_diagnostics_use_relation_outcome_boundary() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src/state/variable_checking/initializer_policy.rs");
    let source = fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()));

    assert_eq!(
        source
            .matches("variable_initializer_relation_outcome")
            .count(),
        4,
        "variable initializer diagnostic probes should route through variable_initializer_relation_outcome"
    );
    assert!(
        !source.contains("assign_relation_outcome("),
        "variable initializer diagnostics should not use the generic assignment request"
    );
    assert!(
        !source.contains("diagnostic_relation_boolean_guard("),
        "variable initializer diagnostics should not regress to raw boolean relation guards"
    );
}

#[test]
fn variable_initializer_relation_outcome_uses_initializer_request() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src/assignability/relation_outcome_helpers.rs");
    let source = fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()));

    assert!(
        source.contains("fn variable_initializer_relation_outcome("),
        "variable initializer diagnostics should have a dedicated outcome helper"
    );
    assert!(
        source.contains("RelationRequest::variable_initializer("),
        "variable initializer relation outcome helper should use the initializer request"
    );
}

#[test]
fn async_jsdoc_return_suppression_uses_relation_outcome() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src/state/variable_checking/core/async_jsdoc_return.rs");
    let source = fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()));
    let compact_source: String = source.chars().filter(|c| !c.is_whitespace()).collect();

    assert!(
        compact_source.contains("return_relation_outcome(unwrapped,decl_ret).related"),
        "async JSDoc return suppression should route unwrapped return compatibility through return relation outcomes"
    );
    assert!(
        !compact_source.contains("assign_relation_outcome(unwrapped,decl_ret).related"),
        "async JSDoc return suppression should not use the generic assignment request"
    );
    assert!(
        !compact_source.contains("diagnostic_relation_boolean_guard(unwrapped,decl_ret)"),
        "async JSDoc return suppression should not use a raw diagnostic boolean relation guard"
    );
}
