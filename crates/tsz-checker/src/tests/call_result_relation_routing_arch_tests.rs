use std::fs;

#[test]
fn call_result_recovery_uses_relation_outcome_boundary() {
    let source = fs::read_to_string("src/types/computation/call_result.rs")
        .expect("failed to read call_result.rs");

    let recovery_start = source
        .find("fn correlated_union_call_recovery_return")
        .expect("missing correlated_union_call_recovery_return helper");
    let recovery_end = recovery_start
        + source[recovery_start..]
            .find("fn finalize_call_return_like_success")
            .expect("missing finalize_call_return_like_success helper");
    let recovery_helper = &source[recovery_start..recovery_end];

    assert!(
        recovery_helper.contains("assign_relation_outcome(actual, param_union).related"),
        "correlated union call recovery should route assignability through RelationOutcome.related"
    );
    assert!(
        !recovery_helper.contains("diagnostic_relation_boolean_guard"),
        "correlated union call recovery should not regress to the raw boolean relation guard"
    );

    let argument_start = source
        .find("fn report_polymorphic_this_indexed_conditional_arg")
        .expect("missing report_polymorphic_this_indexed_conditional_arg helper");
    let argument_end = argument_start
        + source[argument_start..]
            .find("fn error_argument_not_assignable_preserving_param_display")
            .expect("missing error_argument_not_assignable_preserving_param_display helper");
    let argument_helper = &source[argument_start..argument_end];

    assert!(
        argument_helper.contains("assign_relation_outcome(arg_types[2], target).related"),
        "polymorphic-this argument diagnostics should route assignability through RelationOutcome.related"
    );
    assert!(
        !argument_helper.contains("diagnostic_relation_boolean_guard"),
        "polymorphic-this argument diagnostics should not regress to the raw boolean relation guard"
    );
}

#[test]
fn call_tail_polymorphic_this_rest_target_uses_env_relation_outcome_boundary() {
    let source = fs::read_to_string("src/types/computation/call/tail_helpers.rs")
        .expect("failed to read call tail helpers source");
    let helper_start = source
        .find("pub(crate) fn this_argument_satisfies_polymorphic_this_rest_target")
        .expect("missing polymorphic-this rest-target helper");
    let helper_end = helper_start
        + source[helper_start..]
            .find("pub(super) fn report_checked_js_nullable_this_property_method_call")
            .expect("missing next call tail helper");
    let helper = &source[helper_start..helper_end];

    assert_eq!(
        helper.matches("assign_relation_outcome_with_env(").count(),
        2,
        "polymorphic-this rest-target compatibility should route env-aware relation probes through RelationOutcome"
    );
    assert!(
        helper.matches(".related").count() >= 2,
        "polymorphic-this rest-target compatibility should use relation outcome decisions"
    );
    assert!(
        !helper.contains("diagnostic_relation_boolean_guard_with_env("),
        "polymorphic-this rest-target compatibility should not regress to raw env boolean guards"
    );
}

#[test]
fn nominal_lib_object_callback_returns_use_env_relation_outcome_boundary() {
    let source = fs::read_to_string("src/types/computation/call/nominal_lib_object_callbacks.rs")
        .expect("failed to read nominal lib object callback source");
    let helper_start = source
        .find("pub(crate) fn emit_nominal_lib_object_callback_return_errors")
        .expect("missing nominal lib object callback diagnostic helper");
    let helper_end = source[helper_start..]
        .find("pub(crate) fn nominal_lib_object_callback_return_type")
        .map(|offset| helper_start + offset)
        .expect("missing next nominal lib object callback helper");
    let helper = &source[helper_start..helper_end];

    assert_eq!(
        helper.matches("assign_relation_outcome_with_env(").count(),
        1,
        "nominal lib object callback return diagnostics should route env-aware relation probes through RelationOutcome"
    );
    assert!(
        helper.contains(".related"),
        "nominal lib object callback return diagnostics should use relation outcome decisions"
    );
    assert!(
        !helper.contains("diagnostic_relation_boolean_guard_with_env("),
        "nominal lib object callback return diagnostics should not regress to raw env boolean guards"
    );
}

#[test]
fn call_result_spread_rest_recovery_uses_env_relation_outcome_boundary() {
    let source = fs::read_to_string("src/types/computation/call_result.rs")
        .expect("failed to read call_result.rs");
    let helper_start = source
        .find("let mismatch_is_spread_arg =")
        .expect("missing spread mismatch recovery block");
    let helper_end = source[helper_start..]
        .find("let aggregate_literal_actual =")
        .map(|offset| helper_start + offset)
        .expect("missing aggregate literal recovery block");
    let helper = &source[helper_start..helper_end];

    assert_eq!(
        helper.matches("assign_relation_outcome_with_env(").count(),
        1,
        "spread rest recovery should route env-aware relation probes through RelationOutcome"
    );
    assert!(
        helper.contains(".related"),
        "spread rest recovery should use relation outcome decisions"
    );
    assert!(
        !helper.contains("diagnostic_relation_boolean_guard_with_env("),
        "spread rest recovery should not regress to raw env boolean guards"
    );
}

#[test]
fn call_finalize_aggregate_rest_recovery_uses_env_relation_outcome_boundary() {
    let source = fs::read_to_string("src/types/computation/call_finalize.rs")
        .expect("failed to read call_finalize.rs");
    let helper_start = source
        .find("let aggregate_rest_mismatch =")
        .expect("missing aggregate rest mismatch block");
    let helper_end = source[helper_start..]
        .find("if aggregate_assignable")
        .map(|offset| helper_start + offset)
        .expect("missing aggregate assignability branch");
    let helper = &source[helper_start..helper_end];

    assert_eq!(
        helper.matches("assign_relation_outcome_with_env(").count(),
        2,
        "aggregate rest recovery should route both env-aware relation probes through RelationOutcome"
    );
    assert!(
        helper.matches(".related").count() >= 2,
        "aggregate rest recovery should use relation outcome decisions"
    );
    assert!(
        !helper.contains("diagnostic_relation_boolean_guard_with_env("),
        "aggregate rest recovery should not regress to raw env boolean guards"
    );
}
