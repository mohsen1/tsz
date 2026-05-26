use std::fs;

/// Import-attribute shape diagnostics own their checker anchor and TS2322
/// message, but the relation decision should use the shared relation outcome
/// boundary rather than a raw boolean relation guard.
#[test]
fn import_attributes_diagnostics_use_relation_outcome_boundary() {
    let source = fs::read_to_string("src/declarations/import/declaration.rs")
        .expect("failed to read declarations/import/declaration.rs");
    let compact_source: String = source.chars().filter(|ch| !ch.is_whitespace()).collect();

    assert!(
        compact_source
            .contains("assign_relation_outcome(source_type,import_attributes_type).related"),
        "import attribute diagnostics must use relation outcome"
    );
    assert!(
        !compact_source
            .contains("diagnostic_relation_boolean_guard(source_type,import_attributes_type)"),
        "import attribute diagnostics must not use a raw relation guard"
    );
}
