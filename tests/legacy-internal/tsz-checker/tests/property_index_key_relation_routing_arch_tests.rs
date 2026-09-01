use std::fs;

#[test]
fn property_index_key_acceptance_uses_relation_outcome_boundary() {
    let source = fs::read_to_string("src/state/state_checking/property_index_key_helpers.rs")
        .expect("failed to read property_index_key_helpers.rs");

    assert!(
        source.contains("property_index_key_relation_outcome(prop_literal, key_type)")
            && source.contains(".related"),
        "string index key acceptance should route through the typed property-index-key outcome boundary"
    );
    assert!(
        !source.contains("assign_relation_outcome(prop_literal, key_type)"),
        "string index key acceptance should not use the generic assignment request"
    );
    assert!(
        !source.contains("diagnostic_relation_boolean_guard"),
        "string index key acceptance should not regress to the raw boolean relation guard"
    );
}

#[test]
fn property_index_key_relation_outcome_uses_property_index_request() {
    let source = fs::read_to_string("src/assignability/relation_outcome_helpers.rs")
        .expect("failed to read relation_outcome_helpers.rs");

    assert!(
        source.contains("fn property_index_key_relation_outcome(")
            && source.contains("RelationRequest::property_index_key("),
        "property-index-key relation helper should build the canonical property-index-key request"
    );
}
