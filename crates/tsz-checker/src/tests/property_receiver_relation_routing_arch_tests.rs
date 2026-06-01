use std::fs;

#[test]
fn property_receiver_display_uses_relation_outcome_boundary() {
    let source = fs::read_to_string("src/error_reporter/property_receiver_formatting.rs")
        .expect("failed to read property receiver formatting source");
    let start = source
        .find("pub(crate) fn element_access_receiver_declared_element_display")
        .expect("missing declared element display helper");
    let end = source[start..]
        .find("fn element_access_argument_prefers_number_index")
        .expect("missing element access argument helper")
        + start;
    let helpers = &source[start..end];

    let compact_helpers: String = helpers.chars().filter(|ch| !ch.is_whitespace()).collect();

    assert!(
        compact_helpers.contains(
            "property_receiver_element_display_relation_outcome(type_id,declared_element_type).related"
        ) && compact_helpers.contains(
            "property_receiver_element_display_relation_outcome(declared_element_type,type_id).related"
        ),
        "declared element display probes should use dedicated relation outcomes"
    );
    assert!(
        compact_helpers.contains(
            "property_receiver_index_value_display_relation_outcome(actual_type,index_value_type).related"
        ) && compact_helpers.contains(
            "property_receiver_index_value_display_relation_outcome(index_value_type,actual_type,).related"
        ),
        "declared index-value display probes should use dedicated relation outcomes"
    );
    assert!(
        !helpers.contains("assign_relation_outcome"),
        "property receiver display should not use generic assign relation outcomes"
    );
    assert!(
        !helpers.contains("diagnostic_relation_boolean_guard"),
        "property receiver display should not regress to the raw boolean relation guard"
    );
}

#[test]
fn element_access_index_diagnostics_use_relation_outcome_boundary() {
    let source = fs::read_to_string("src/error_reporter/properties/diagnostic_methods_tail.rs")
        .expect("failed to read property diagnostic methods source");
    let start = source
        .find("let is_for_in_index = self.is_for_in_variable_identifier(arg_idx);")
        .expect("missing element access index diagnostic block");
    let end = start
        + source[start..]
            .find("fn is_named_method_suggestion_receiver")
            .expect("missing end of element access index diagnostic block");
    let block = &source[start..end];
    let compact_block: String = block.chars().filter(|ch| !ch.is_whitespace()).collect();

    assert!(
        compact_block.contains(
            "element_access_number_index_relation_outcome(index_type,TypeId::NUMBER).related"
        ),
        "TS7015 numeric-index diagnostics should use a dedicated relation outcome"
    );
    assert!(
        compact_block.contains(
            "element_access_method_suggestion_relation_outcome(index_type,first.type_id).related"
        ),
        "no-index-signature method suggestions should use a dedicated relation outcome"
    );
    assert!(
        !compact_block.contains("assign_relation_outcome(index_type,TypeId::NUMBER)")
            && !compact_block.contains("assign_relation_outcome(index_type,first.type_id)"),
        "element access index diagnostics should not use generic assign relation outcomes"
    );
    assert!(
        !block.contains("diagnostic_relation_boolean_guard"),
        "element access index diagnostics should not use raw diagnostic boolean relation guards"
    );
}

#[test]
fn property_receiver_relation_outcomes_use_dedicated_requests() {
    let source = fs::read_to_string("src/assignability/relation_outcome_helpers.rs")
        .expect("failed to read relation_outcome_helpers.rs");

    for (helper, request) in [
        (
            "fn property_receiver_element_display_relation_outcome(",
            "RelationRequest::property_receiver_element_display(",
        ),
        (
            "fn property_receiver_index_value_display_relation_outcome(",
            "RelationRequest::property_receiver_index_value_display(",
        ),
        (
            "fn element_access_number_index_relation_outcome(",
            "RelationRequest::element_access_number_index(",
        ),
        (
            "fn element_access_method_suggestion_relation_outcome(",
            "RelationRequest::element_access_method_suggestion(",
        ),
    ] {
        assert!(
            source.contains(helper) && source.contains(request),
            "{helper} should build {request}"
        );
    }
}
