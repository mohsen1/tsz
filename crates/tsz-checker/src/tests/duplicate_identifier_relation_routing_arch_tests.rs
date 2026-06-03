use std::fs;
use std::path::Path;

#[test]
fn duplicate_identifier_helpers_use_relation_outcome_boundary() {
    let source = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/types/type_checking/duplicate_identifier_relation_helpers.rs"),
    )
    .expect("failed to read duplicate_identifier_relation_helpers.rs");

    assert!(
        source
            .matches("duplicate_identifier_relation_outcome(")
            .count()
            >= 5,
        "duplicate declaration relation helpers should route through duplicate_identifier_relation_outcome"
    );
    assert!(
        !source.contains("assign_relation_outcome("),
        "duplicate declaration relation helpers should not use the generic assign relation outcome"
    );
    assert!(
        !source.contains("diagnostic_relation_boolean_guard("),
        "duplicate declaration relation helpers should not use the raw boolean relation guard"
    );
}

#[test]
fn duplicate_identifier_relation_outcome_uses_duplicate_identifier_request() {
    let source = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/assignability/relation_outcome_helpers.rs"),
    )
    .expect("failed to read relation_outcome_helpers.rs");

    assert!(
        source.contains("fn duplicate_identifier_relation_outcome("),
        "duplicate declaration relations should have a dedicated outcome helper"
    );
    assert!(
        source.contains("RelationRequest::duplicate_identifier("),
        "duplicate declaration relation outcome helper should use the duplicate identifier request"
    );
}
