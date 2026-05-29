use std::fs;

#[test]
fn array_elaboration_element_probe_uses_relation_outcome_boundary() {
    let source = fs::read_to_string("src/error_reporter/call_errors/elaboration_array_mismatch.rs")
        .expect("failed to read elaboration_array_mismatch.rs");
    let start = source
        .find("SubtypeFailureReason::ArrayElementMismatch")
        .expect("missing array element mismatch branch");
    let end = start
        + source[start..]
            .find("            _ => false,")
            .expect("missing end of array element mismatch branch");
    let branch = &source[start..end];

    assert_eq!(
        branch.matches("assign_relation_outcome(").count(),
        1,
        "array element elaboration should route the element relation through assign_relation_outcome"
    );
    assert!(
        branch.contains(".related"),
        "array element elaboration should use the shared relation outcome decision"
    );
    assert!(
        !branch.contains("diagnostic_relation_boolean_guard("),
        "array element elaboration should not regress to the raw boolean relation guard"
    );
}
