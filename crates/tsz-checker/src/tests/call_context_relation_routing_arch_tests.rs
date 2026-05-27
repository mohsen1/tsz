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

#[test]
fn round2_argument_recheck_uses_env_relation_outcome_boundary() {
    let source = fs::read_to_string("src/types/computation/call_inference/argument_context.rs")
        .expect("failed to read call inference argument context source");
    let start = source
        .find("pub(crate) fn recheck_generic_call_arguments_with_real_types")
        .expect("missing round-2 argument recheck helper");
    let end = source[start..]
        .find("pub(crate) fn compute_round2_contextual_types")
        .map(|offset| start + offset)
        .expect("missing next argument context helper");
    let helper = &source[start..end];

    assert_eq!(
        helper.matches("assign_relation_outcome_with_env(").count(),
        2,
        "round-2 argument recheck should route env-aware relation probes through RelationOutcome"
    );
    assert!(
        helper.matches(".related").count() >= 2,
        "round-2 argument recheck should use relation outcome decisions"
    );
    assert!(
        !helper.contains("is_assignable_to_with_env("),
        "round-2 argument recheck should not regress to raw env boolean assignability"
    );
}

#[test]
fn round2_inference_refinement_uses_env_relation_outcome_boundary() {
    let source = fs::read_to_string("src/types/computation/call_inference.rs")
        .expect("failed to read call inference source");
    let start = source
        .find("pub(crate) fn refine_instantiated_params_with_checker_substitution")
        .expect("missing checker substitution refinement helper");
    let end = source[start..]
        .find("#[cfg(test)]")
        .map(|offset| start + offset)
        .expect("missing call inference test module marker");
    let helper = &source[start..end];

    assert_eq!(
        helper.matches("assign_relation_outcome_with_env(").count(),
        4,
        "round-2 inference refinement should route env-aware relation probes through RelationOutcome"
    );
    assert!(
        helper.matches(".related").count() >= 4,
        "round-2 inference refinement should use relation outcome decisions"
    );
    assert!(
        !helper.contains("is_assignable_to_with_env("),
        "round-2 inference refinement should not regress to raw env boolean assignability"
    );
}
