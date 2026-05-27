use std::fs;

#[test]
fn explicit_callback_param_conflict_uses_relation_outcome_boundary() {
    let source =
        fs::read_to_string("src/checkers/call_context.rs").expect("failed to read call_context.rs");
    let start = source
        .find("pub(crate) fn callback_has_explicit_param_type_conflict")
        .expect("missing callback_has_explicit_param_type_conflict helper");
    let end = start
        + source[start..]
            .find("pub(crate) fn suppress_generic_return_context_for_arg")
            .expect("missing suppress_generic_return_context_for_arg helper");
    let helper = &source[start..end];

    assert_eq!(
        helper.matches("assign_relation_outcome").count(),
        1,
        "explicit callback parameter conflict should route through assign_relation_outcome"
    );
    assert!(
        helper.contains(".related"),
        "explicit callback parameter conflict should use the relation outcome decision"
    );
    assert!(
        !helper.contains("diagnostic_relation_boolean_guard"),
        "explicit callback parameter conflict should not regress to the raw boolean relation guard"
    );
}
