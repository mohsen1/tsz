use std::{fs, path::PathBuf};

#[test]
fn nullish_target_diagnostic_uses_relation_outcome_boundary() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src/assignability/nullish_error_targets.rs");
    let source = fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()));
    let branch = source
        .split("let (_, nullable_target) = self.split_nullish_type(target);")
        .nth(1)
        .expect("missing nullish target relation branch");

    assert!(
        branch.contains("self.assign_relation_outcome(source, nullable).related"),
        "nullish target diagnostic should use the shared relation outcome boundary"
    );
    assert!(
        !branch.contains("diagnostic_relation_boolean_guard(source, nullable)"),
        "nullish target diagnostic should not use the legacy boolean guard"
    );
}
