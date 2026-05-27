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
        source.matches("assign_relation_outcome(").count() >= 5,
        "duplicate declaration relation helpers should route through assign_relation_outcome"
    );
    assert!(
        !source.contains("diagnostic_relation_boolean_guard("),
        "duplicate declaration relation helpers should not use the raw boolean relation guard"
    );
}
