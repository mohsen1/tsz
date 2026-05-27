use std::{fs, path::PathBuf};

#[test]
fn iterable_next_diagnostic_uses_relation_outcome_boundary() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/checkers/iterable_checker.rs");
    let source = fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()));
    let branch = source
        .split("// Check if the sent type is assignable to the iterator's next type.")
        .nth(1)
        .expect("missing iterator next relation diagnostic branch")
        .split("// Not assignable - emit the appropriate diagnostic")
        .next()
        .expect("missing end of iterator next relation diagnostic branch");

    assert!(
        branch.contains("self.assign_relation_outcome(sent_type, next_type).related"),
        "iterator next diagnostic should use the shared relation outcome boundary"
    );
    assert!(
        !branch.contains("diagnostic_relation_boolean_guard(sent_type, next_type)"),
        "iterator next diagnostic should not use the legacy boolean guard"
    );
}
