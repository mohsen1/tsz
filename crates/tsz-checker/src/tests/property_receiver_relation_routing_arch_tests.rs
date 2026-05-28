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

    assert_eq!(
        helpers.matches("assign_relation_outcome").count(),
        4,
        "property receiver display relation checks should route through assign_relation_outcome"
    );
    assert!(
        helpers.contains(".related"),
        "property receiver display relation checks should use the relation outcome decision"
    );
    assert!(
        !helpers.contains("diagnostic_relation_boolean_guard"),
        "property receiver display should not regress to the raw boolean relation guard"
    );
}

#[test]
fn element_access_index_diagnostics_use_relation_outcome_boundary() {
    let source = fs::read_to_string("src/error_reporter/properties.rs")
        .expect("failed to read properties error reporter source");
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
        compact_block.contains("assign_relation_outcome(index_type,TypeId::NUMBER).related"),
        "TS7015 numeric-index diagnostics should route index/number compatibility through relation outcomes"
    );
    assert!(
        compact_block.contains("assign_relation_outcome(index_type,first.type_id).related"),
        "no-index-signature method suggestions should route index/parameter compatibility through relation outcomes"
    );
    assert!(
        !block.contains("diagnostic_relation_boolean_guard"),
        "element access index diagnostics should not use raw diagnostic boolean relation guards"
    );
}
