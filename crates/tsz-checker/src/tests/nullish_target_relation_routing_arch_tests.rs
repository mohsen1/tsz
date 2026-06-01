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
        branch.contains("nullish_error_target_relation_outcome(source, nullable)")
            && branch.contains(".related"),
        "nullish target diagnostic should use the typed nullish-target relation outcome boundary"
    );
    assert!(
        !branch.contains("assign_relation_outcome(source, nullable)"),
        "nullish target diagnostic should not use the generic assignment request"
    );
    assert!(
        !branch.contains("diagnostic_relation_boolean_guard(source, nullable)"),
        "nullish target diagnostic should not use the legacy boolean guard"
    );
}

#[test]
fn nullish_target_relation_outcome_uses_nullish_request() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src/assignability/relation_outcome_helpers.rs");
    let source = fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()));

    assert!(
        source.contains("fn nullish_error_target_relation_outcome(")
            && source.contains("RelationRequest::nullish_error_target("),
        "nullish target relation helper should build the canonical nullish-target request"
    );
}

#[test]
fn nullish_coalescing_result_collapse_uses_subtype_outcome_boundary() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src/types/computation/nullish_coalescing.rs");
    let source = fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()));
    let compact: String = source.chars().filter(|c| !c.is_whitespace()).collect();

    assert!(
        compact.contains("diagnostic_subtype_outcome(right_type,non_nullish).related")
            && compact.contains("diagnostic_subtype_outcome(non_nullish,right_type).related"),
        "nullish-coalescing result collapse should use subtype outcomes"
    );
    assert!(
        !compact.contains("is_subtype_of(right_type,non_nullish)")
            && !compact.contains("is_subtype_of(non_nullish,right_type)"),
        "nullish-coalescing result collapse should not use raw subtype probes"
    );
}
