use std::fs;

#[test]
fn call_error_anchor_mismatch_probes_use_relation_outcome_boundary() {
    let source = fs::read_to_string("src/error_reporter/call_errors_anchors.rs")
        .expect("failed to read call_errors_anchors.rs");

    assert_eq!(
        source.matches("assign_relation_outcome(").count(),
        4,
        "call error anchoring should route mismatch probes through assign_relation_outcome"
    );
    assert_eq!(
        source.matches("diagnostic_relation_boolean_guard(").count(),
        0,
        "call error anchoring should not regress to the raw boolean relation guard"
    );
    assert!(
        source.contains(".assign_relation_outcome(arg_type, expected_type)")
            || source.contains("assign_relation_outcome(arg_type, expected_type)"),
        "literal argument anchoring should route argument-vs-parameter probes through the boundary"
    );
    assert!(
        source.contains(".assign_relation_outcome(source_prop_type, target_prop_type)")
            || source.contains("assign_relation_outcome(source_prop_type, target_prop_type)"),
        "object literal anchoring should route property mismatch probes through the boundary"
    );
    assert!(
        source.contains(".assign_relation_outcome(elem_type, target_element_type)")
            || source.contains("assign_relation_outcome(elem_type, target_element_type)"),
        "array literal anchoring should route element mismatch probes through the boundary"
    );
}
