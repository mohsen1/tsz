#[test]
fn recursive_heritage_conflict_check_does_not_compare_rendered_types() {
    let source = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/checkers/generic_checker/recursive_heritage_constraint.rs"
    ))
    .expect("recursive heritage checker source should be readable");

    let start = source
        .find("pub(super) fn member_has_conflicting_constraint_property")
        .expect("recursive heritage conflict helper should exist");
    let body = &source[start..];
    let end = body
        .find("\n    }\n}")
        .expect("recursive heritage conflict helper should end before impl close");
    let helper_body = &body[..end];

    assert!(
        !helper_body.contains("format_type_diagnostic"),
        "recursive heritage conflict detection must use structural facts, not rendered type strings"
    );
    assert!(
        helper_body.contains("recursive_heritage_property_types_conflict"),
        "recursive heritage conflict detection should route through the assignability boundary"
    );
}

#[test]
fn call_parameter_array_display_normalization_is_not_gated_by_rendered_text() {
    let source = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/types/computation/call_result.rs"
    ))
    .expect("call result source should be readable");

    let start = source
        .find("fn error_argument_not_assignable_preserving_param_display")
        .expect("call argument diagnostic helper should exist");
    let body = &source[start..];
    let end = body
        .find("\n    fn finite_mapped_parameter_display_type")
        .expect("call argument diagnostic helper should end before next helper");
    let helper_body = &body[..end];

    assert!(
        !helper_body.contains("target_display.contains(\"Array<\")"),
        "call argument target display normalization must not branch on rendered Array<T> text"
    );
    assert!(
        helper_body.contains("Self::normalize_array_generic_to_shorthand(&target_display)"),
        "call argument target display should always route through the idempotent display normalizer"
    );
}

#[test]
fn mapped_target_type_parameter_containment_is_structural() {
    let source = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/assignability/assignability_diagnostics/argument_reports.rs"
    ))
    .expect("assignability diagnostics source should be readable");

    let start = source
        .find("pub(crate) fn should_suppress_self_referential_mapped_constraint_arg_mismatch")
        .expect("self-referential mapped constraint helper should exist");
    let body = &source[start..];
    let end = body
        .find("\n    fn self_referential_mapped_intersection_accepts_object_literal")
        .expect("self-referential mapped constraint helper should end before next helper");
    let helper_body = &body[..end];

    assert!(
        !helper_body.contains("format_type_for_assignability_message"),
        "mapped target type-parameter containment must not inspect rendered target text"
    );
    assert!(
        !helper_body.contains(".contains(name.as_ref())"),
        "mapped target type-parameter containment must not string-match user-chosen parameter names"
    );
    assert!(
        helper_body.contains("contains_type_parameter_named("),
        "mapped target type-parameter containment should route through the structural query boundary"
    );
}
