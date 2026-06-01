use std::fs;

#[test]
fn interface_heritage_index_values_use_relation_outcome_boundary() {
    let source_path = format!(
        "{}/src/classes/interface_heritage_index_compat.rs",
        env!("CARGO_MANIFEST_DIR")
    );
    let source = fs::read_to_string(source_path)
        .expect("read interface heritage index compatibility helpers");

    let start = source
        .find("pub(super) fn index_value_assignable_for_interface_extends")
        .expect("find interface heritage index value entrypoint");
    let end = source[start..]
        .find("fn type_heritage_includes_base")
        .map(|offset| start + offset)
        .expect("find end of relation-routing helpers");
    let helper_source = &source[start..end];
    let compact = helper_source.split_whitespace().collect::<String>();

    assert!(
        compact.contains(
            "interface_heritage_index_value_relation_outcome(derived_value,base_value).related"
        ),
        "interface heritage index value checks must route through their dedicated relation outcome"
    );
    assert!(
        helper_source.matches(".related").count() >= 2,
        "both the direct relation check and member relation check must consume RelationOutcome.related"
    );
    assert!(
        !helper_source.contains("assign_relation_outcome(")
            && !helper_source.contains("diagnostic_relation_boolean_guard"),
        "interface heritage index value checks should not regress to generic assign or raw boolean relation guards"
    );
}

#[test]
fn interface_heritage_generic_method_specialization_uses_relation_outcome_boundary() {
    let source_path = format!(
        "{}/src/classes/interface_heritage_index_compat.rs",
        env!("CARGO_MANIFEST_DIR")
    );
    let source = fs::read_to_string(source_path)
        .expect("read interface heritage index compatibility helpers");

    let start = source
        .find("pub(super) fn generic_method_override_is_valid_specialization")
        .expect("find interface heritage generic method specialization helper");
    let end = source[start..]
        .find("pub(super) fn type_base_def_id")
        .map(|offset| start + offset)
        .expect("find end of generic method specialization helper");
    let helper_source = &source[start..end];
    let compact = helper_source.split_whitespace().collect::<String>();

    assert!(
        compact
            .contains("interface_heritage_generic_method_relation_outcome(derived,base).related"),
        "interface heritage generic method specialization should route relation truth through its dedicated RelationOutcome"
    );
    assert!(
        !helper_source.contains("assign_relation_outcome(")
            && !helper_source.contains("diagnostic_relation_boolean_guard"),
        "interface heritage generic method specialization should not regress to generic assign or raw boolean relation guards"
    );
}

#[test]
fn heritage_relation_outcomes_use_dedicated_requests() {
    let source_path = format!(
        "{}/src/assignability/relation_outcome_helpers.rs",
        env!("CARGO_MANIFEST_DIR")
    );
    let source =
        fs::read_to_string(source_path).expect("read relation outcome helper implementations");

    for (helper, request) in [
        (
            "fn class_implements_index_value_relation_outcome(",
            "RelationRequest::class_implements_index_value(",
        ),
        (
            "fn class_implements_whole_type_relation_outcome(",
            "RelationRequest::class_implements_whole_type(",
        ),
        (
            "fn interface_heritage_index_value_relation_outcome(",
            "RelationRequest::interface_heritage_index_value(",
        ),
        (
            "fn interface_heritage_generic_method_relation_outcome(",
            "RelationRequest::interface_heritage_generic_method(",
        ),
        (
            "fn interface_heritage_property_index_relation_outcome(",
            "RelationRequest::interface_heritage_property_index(",
        ),
        (
            "fn jsdoc_heritage_constraint_relation_outcome(",
            "RelationRequest::jsdoc_heritage_constraint(",
        ),
    ] {
        assert!(
            source.contains(helper) && source.contains(request),
            "{helper} must build {request}"
        );
    }
}
