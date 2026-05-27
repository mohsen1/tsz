use std::{fs, path::PathBuf};

#[test]
fn variable_initializer_diagnostics_use_relation_outcome_boundary() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src/state/variable_checking/initializer_policy.rs");
    let source = fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()));

    assert_eq!(
        source.matches("assign_relation_outcome").count(),
        4,
        "variable initializer diagnostic probes should route through assign_relation_outcome"
    );
    assert!(
        !source.contains("diagnostic_relation_boolean_guard("),
        "variable initializer diagnostics should not regress to raw boolean relation guards"
    );
}
