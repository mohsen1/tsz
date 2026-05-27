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
