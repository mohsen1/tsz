use std::fs;

#[test]
fn return_statement_diagnostics_use_return_relation_outcome_boundary() {
    let helper_source = fs::read_to_string("src/assignability/relation_outcome_helpers.rs")
        .expect("failed to read relation_outcome_helpers.rs");
    let return_source = fs::read_to_string("src/types/type_checking/core_statement_checks.rs")
        .expect("failed to read core_statement_checks.rs");
    let function_return_source = fs::read_to_string("src/types/function_type_helpers.rs")
        .expect("failed to read function_type_helpers.rs");
    let compact_return_source: String = return_source
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    let compact_function_return_source: String = function_return_source
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();

    assert!(
        helper_source.contains("fn return_relation_outcome("),
        "return diagnostics should expose a named relation outcome helper"
    );
    assert!(
        helper_source.contains("RelationRequest::return_stmt("),
        "return diagnostics should build a return-shaped RelationRequest"
    );
    assert!(
        compact_return_source
            .contains("return_relation_outcome(return_type,expected_type).related"),
        "return statement compatibility checks should use the return relation outcome"
    );
    assert!(
        compact_return_source
            .contains("return_relation_outcome(contextual_type,expected_type).related"),
        "contextual return recovery should use the return relation outcome"
    );
    assert!(
        compact_return_source.contains("return_relation_outcome(return_type,member)"),
        "contextual callable-union return deferral should use the return relation outcome"
    );
    assert!(
        !compact_return_source
            .contains("diagnostic_relation_boolean_guard(return_type,expected_type)"),
        "return statement diagnostics should not pre-gate with a raw boolean relation"
    );
    assert!(
        !compact_return_source
            .contains("diagnostic_relation_boolean_guard(contextual_type,expected_type)"),
        "contextual return recovery should not pre-gate with a raw boolean relation"
    );
    assert!(
        !compact_return_source.contains("assign_relation_outcome(return_type,member)"),
        "return statement callable-union deferral should not use the generic assignment relation"
    );
    assert!(
        compact_function_return_source
            .contains("return_relation_outcome(when_true,expected_return_type).related"),
        "expression-bodied conditional true returns should use the return relation outcome"
    );
    assert!(
        compact_function_return_source
            .contains("return_relation_outcome(when_false,expected_return_type).related"),
        "expression-bodied conditional false returns should use the return relation outcome"
    );
    assert!(
        !compact_function_return_source
            .contains("diagnostic_relation_boolean_guard(when_true,expected_return_type)"),
        "expression-bodied conditional true returns should not pre-gate with a raw boolean relation"
    );
    assert!(
        !compact_function_return_source
            .contains("diagnostic_relation_boolean_guard(when_false,expected_return_type)"),
        "expression-bodied conditional false returns should not pre-gate with a raw boolean relation"
    );
}
