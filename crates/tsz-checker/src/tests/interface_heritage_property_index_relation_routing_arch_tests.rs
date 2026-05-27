use std::fs;

#[test]
fn interface_heritage_property_index_checks_use_relation_outcome_boundary() {
    let source_path = format!(
        "{}/src/classes/interface_heritage_index.rs",
        env!("CARGO_MANIFEST_DIR")
    );
    let source =
        fs::read_to_string(source_path).expect("read interface heritage index property helper");

    let start = source
        .find("pub(crate) fn check_type_alias_base_properties_against_derived_string_index")
        .expect("find type-alias base property index helper");
    let helper_source = &source[start..];

    assert!(
        helper_source.contains(".assign_relation_outcome(prop_type, string_index_value)")
            || helper_source.contains("assign_relation_outcome(prop_type, string_index_value)"),
        "interface heritage property/index checks must route through assign_relation_outcome"
    );
    assert!(
        helper_source.contains(".related"),
        "interface heritage property/index checks must consume RelationOutcome.related"
    );
    assert!(
        !helper_source.contains("diagnostic_relation_boolean_guard"),
        "interface heritage property/index checks should not regress to raw boolean relation guards"
    );
}
