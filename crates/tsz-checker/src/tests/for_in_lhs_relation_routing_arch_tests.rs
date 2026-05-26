use std::{fs, path::PathBuf};

#[test]
fn for_in_lhs_diagnostic_uses_relation_outcome_boundary() {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/state/variable_checking/for_loop.rs");
    let source = fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()));
    let branch = source
        .split("// TS2405: For for-in, also check that the LHS type is string or any.")
        .nth(1)
        .expect("missing for-in LHS diagnostic branch")
        .split("// Get the type of the initializer expression")
        .next()
        .expect("missing end of for-in LHS diagnostic branch");

    assert!(
        branch.contains("self.assign_relation_outcome(element_type, var_type).related"),
        "for-in LHS diagnostic should use the shared relation outcome boundary"
    );
    assert!(
        !branch.contains("diagnostic_relation_boolean_guard(element_type, var_type)"),
        "for-in LHS diagnostic should not use the legacy boolean guard"
    );
}
