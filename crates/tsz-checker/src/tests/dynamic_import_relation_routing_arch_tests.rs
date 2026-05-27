use std::fs;

/// Dynamic-import specifier and options diagnostics own their checker anchors
/// and messages, but the relation decision should use the shared relation
/// outcome boundary rather than a raw boolean relation guard.
#[test]
fn dynamic_import_diagnostics_use_relation_outcome_boundary() {
    let source = fs::read_to_string("src/declarations/dynamic_import_checker.rs")
        .expect("failed to read dynamic_import_checker.rs");
    let compact_source: String = source.chars().filter(|ch| !ch.is_whitespace()).collect();

    assert!(
        compact_source.contains("assign_relation_outcome(arg_type,TypeId::STRING).related"),
        "dynamic import specifier diagnostics must use relation outcome"
    );
    assert!(
        compact_source
            .contains("assign_relation_outcome(options_type,import_call_options_type).related"),
        "dynamic import options diagnostics must use relation outcome"
    );
    assert!(
        !compact_source.contains("diagnostic_relation_boolean_guard(arg_type,TypeId::STRING)"),
        "dynamic import specifier diagnostics must not use a raw relation guard"
    );
    assert!(
        !compact_source
            .contains("diagnostic_relation_boolean_guard(options_type,import_call_options_type)"),
        "dynamic import options diagnostics must not use a raw relation guard"
    );
}
