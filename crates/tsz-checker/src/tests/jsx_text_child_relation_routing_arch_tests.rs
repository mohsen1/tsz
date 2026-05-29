use std::{fs, path::PathBuf};

#[test]
fn jsx_text_child_diagnostic_uses_relation_outcome_boundary() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/checkers/jsx/diagnostics.rs");
    let source = fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()));
    let branch = source
        .split("pub(crate) fn check_jsx_text_children_accepted")
        .nth(1)
        .expect("missing JSX text child diagnostic helper")
        .split("// Get component name for the diagnostic message.")
        .next()
        .expect("missing JSX text child diagnostic relation branch");

    assert!(
        branch.contains("assign_relation_outcome(TypeId::STRING, children_type)"),
        "JSX text-child diagnostics should use the shared relation outcome boundary"
    );
    assert!(
        !branch.contains("diagnostic_relation_boolean_guard(TypeId::STRING, children_type)"),
        "JSX text-child diagnostics should not use the legacy boolean guard"
    );
}
