use std::fs;

#[test]
fn property_index_key_acceptance_uses_relation_outcome_boundary() {
    let source = fs::read_to_string("src/state/state_checking/property_index_key_helpers.rs")
        .expect("failed to read property_index_key_helpers.rs");

    assert!(
        source.contains("assign_relation_outcome(prop_literal, key_type).related"),
        "string index key acceptance should route assignability through RelationOutcome.related"
    );
    assert!(
        !source.contains("diagnostic_relation_boolean_guard"),
        "string index key acceptance should not regress to the raw boolean relation guard"
    );
}
