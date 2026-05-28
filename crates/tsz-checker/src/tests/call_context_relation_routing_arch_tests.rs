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
fn source_constraint_substitution_fallback_uses_env_relation_outcome_boundary() {
    let source = fs::read_to_string("src/types/computation/call_inference.rs")
        .expect("failed to read call inference source");
    let start = source
        .find("fn substitution_with_source_constraint_fallbacks")
        .expect("missing source constraint substitution fallback helper");
    let end = source[start..]
        .find("pub(crate) fn resolve_signature_parameter_type_queries")
        .map(|offset| start + offset)
        .expect("missing source constraint substitution fallback end marker");
    let helper = &source[start..end];

    assert_eq!(
        helper.matches("assign_relation_outcome_with_env(").count(),
        1,
        "source constraint substitution fallback should route env-aware relation probes through RelationOutcome"
    );
    assert!(
        helper.contains(".related"),
        "source constraint substitution fallback should use the relation outcome decision"
    );
    assert!(
        !helper.contains("is_assignable_to_with_env("),
        "source constraint substitution fallback should not regress to raw env boolean assignability"
    );
}

#[test]
fn round2_contextual_substitution_widening_uses_relation_outcome_boundary() {
    let source = fs::read_to_string("src/types/computation/call_inference.rs")
        .expect("failed to read call inference source");
    let start = source
        .find("pub(crate) fn widen_round2_contextual_substitution")
        .expect("missing round-2 contextual substitution widening helper");
    let end = source[start..]
        .find("fn fill_unresolved_contextual_substitution_from_constraints")
        .map(|offset| start + offset)
        .expect("missing round-2 contextual substitution widening end marker");
    let helper = &source[start..end];

    assert_eq!(
        helper.matches("assign_relation_outcome(").count(),
        2,
        "round-2 contextual substitution widening should route relation probes through RelationOutcome"
    );
    assert!(
        helper.matches(".related").count() >= 2,
        "round-2 contextual substitution widening should use relation outcome decisions"
    );
    assert!(
        !helper.contains("is_assignable_to("),
        "round-2 contextual substitution widening should not regress to raw boolean assignability"
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

#[test]
fn contextual_generic_call_retry_uses_env_relation_outcome_boundary() {
    for (path, end_marker) in [
        (
            "src/types/computation/call/mod.rs",
            "if is_generic_call\n            && should_retry_generic_call",
        ),
        (
            "src/types/computation/call/inner.rs",
            "let mut retried_arg_types = None;",
        ),
    ] {
        let source = fs::read_to_string(path).expect("failed to read call computation source");
        let start = source
            .find("let should_retry_generic_call =")
            .unwrap_or_else(|| panic!("missing contextual generic retry block in {path}"));
        let end = source[start..]
            .find(end_marker)
            .map(|offset| start + offset)
            .unwrap_or_else(|| panic!("missing contextual generic retry end marker in {path}"));
        let retry_block = &source[start..end];

        assert_eq!(
            retry_block
                .matches("assign_relation_outcome_with_env(")
                .count(),
            1,
            "contextual generic retry in {path} should route env-aware relation probes through RelationOutcome"
        );
        assert!(
            retry_block.contains(".related"),
            "contextual generic retry in {path} should use the relation outcome decision"
        );
        assert!(
            !retry_block.contains("is_assignable_to_with_env("),
            "contextual generic retry in {path} should not regress to raw env boolean assignability"
        );
    }
}

#[test]
fn contextual_return_substitution_uses_env_relation_outcome_boundary() {
    let source = fs::read_to_string("src/types/computation/call/inner.rs")
        .expect("failed to read call inner source");
    let start = source
        .find("let contextual_params_fit_args =")
        .expect("missing contextual return substitution fit block");
    let end = source[start..]
        .find("drop(generic_inference_arg_types);")
        .map(|offset| start + offset)
        .expect("missing contextual return substitution end marker");
    let helper = &source[start..end];

    assert_eq!(
        helper.matches("assign_relation_outcome_with_env(").count(),
        3,
        "contextual return substitution should route env-aware relation probes through RelationOutcome"
    );
    assert!(
        helper.matches(".related").count() >= 3,
        "contextual return substitution should use relation outcome decisions"
    );
    assert!(
        !helper.contains("is_assignable_to_with_env("),
        "contextual return substitution should not regress to raw env boolean assignability"
    );
}

#[test]
fn contextual_callback_return_retyping_uses_env_relation_outcome_boundary() {
    let source = fs::read_to_string("src/types/computation/call/inner_argument_collection.rs")
        .expect("failed to read call argument collection source");
    let start = source
        .find("let ctx_return = refreshed_contextual_types")
        .expect("missing contextual callback return block");
    let end = source[start..]
        .find("refreshed_args")
        .map(|offset| start + offset)
        .expect("missing contextual callback return block end marker");
    let helper = &source[start..end];

    assert_eq!(
        helper.matches("assign_relation_outcome_with_env(").count(),
        1,
        "contextual callback return retyping should route env-aware relation probes through RelationOutcome"
    );
    assert!(
        helper.contains(".related"),
        "contextual callback return retyping should use the relation outcome decision"
    );
    assert!(
        !helper.contains("is_assignable_to_with_env("),
        "contextual callback return retyping should not regress to raw env boolean assignability"
    );
}
