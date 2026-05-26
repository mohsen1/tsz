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

    assert!(
        helper_source.contains(".assign_relation_outcome(derived_value, base_value)")
            || helper_source.contains("assign_relation_outcome(derived_value, base_value)"),
        "interface heritage index value checks must route through assign_relation_outcome"
    );
    assert!(
        helper_source.matches(".related").count() >= 2,
        "both the direct relation check and member relation check must consume RelationOutcome.related"
    );
    assert!(
        !helper_source.contains("diagnostic_relation_boolean_guard"),
        "interface heritage index value checks should not regress to raw boolean relation guards"
    );
}
