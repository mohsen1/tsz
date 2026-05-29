use std::fs;
use std::path::Path;

#[test]
fn contextual_new_argument_recovery_uses_relation_outcome_boundary() {
    let source = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/types/computation/complex_contextual_new.rs"),
    )
    .expect("failed to read complex_contextual_new.rs");

    let function_start = source
        .find("fn generic_new_argument_accepts_contextual_parameter")
        .expect("find contextual new argument helper");
    let function_end = function_start
        + source[function_start..]
            .find(
                "pub(crate) fn recover_new_expression_return_type_after_contextual_argument_match",
            )
            .expect("find end of contextual new argument helper");
    let helper = &source[function_start..function_end];
    let compact_helper: String = helper.chars().filter(|ch| !ch.is_whitespace()).collect();

    assert!(
        compact_helper.contains("assign_relation_outcome(contextual_actual,expected).related"),
        "contextual new argument recovery should route compatibility through relation outcome"
    );
    assert!(
        !helper.contains("diagnostic_relation_boolean_guard"),
        "contextual new argument recovery should not use a raw boolean relation guard"
    );
}

#[test]
fn constructor_inference_constraint_checks_use_relation_outcome_boundary() {
    let source = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/types/computation/complex_constructor_inference.rs"),
    )
    .expect("failed to read complex_constructor_inference.rs");

    let applyable_start = source
        .find("pub(super) fn new_type_args_are_applyable")
        .expect("find new type-argument applicability helper");
    let applyable_end = applyable_start
        + source[applyable_start..]
            .find("pub(super) fn default_current_infer_placeholders_to_unknown")
            .expect("find end of new type-argument applicability helper");
    let applyable_helper = &source[applyable_start..applyable_end];
    assert_eq!(
        applyable_helper
            .matches("assign_relation_outcome_with_env(")
            .count(),
        1,
        "constructor type-argument applicability should route env-aware constraint checks through RelationOutcome"
    );
    assert!(
        applyable_helper.contains(".related"),
        "constructor type-argument applicability should use the relation outcome decision"
    );
    assert!(
        !applyable_helper.contains("is_assignable_to_with_env("),
        "constructor type-argument applicability should not regress to raw env boolean assignability"
    );

    let fallback_start = source
        .find("pub(super) fn generic_constructor_nested_constraint_failure_return")
        .expect("find primitive constraint fallback helper");
    let fallback_end = fallback_start
        + source[fallback_start..]
            .find("\n    fn primitive_parts")
            .expect("find end of primitive constraint fallback helper");
    let fallback_helper = &source[fallback_start..fallback_end];
    assert!(
        fallback_helper.contains("assign_relation_outcome(part, constraint).related"),
        "constructor primitive constraint fallback should route primitive-part checks through RelationOutcome"
    );
    assert!(
        !fallback_helper.contains("is_assignable_to(part, constraint)"),
        "constructor primitive constraint fallback should not regress to raw boolean assignability"
    );
}
