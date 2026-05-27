use std::fs;
use std::path::Path;

#[test]
fn index_signature_property_checks_use_relation_outcome_boundary() {
    let source = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/state/state_checking_members/index_signature_key_helpers.rs"),
    )
    .expect("failed to read index_signature_key_helpers.rs");

    let function_start = source
        .find("fn property_type_assignable_to_index_type")
        .expect("find property/index compatibility helper");
    let function_end = function_start
        + source[function_start..]
            .find("pub(crate) fn format_ts2411_type")
            .expect("find end of property/index compatibility helper");
    let helper = &source[function_start..function_end];
    let compact_helper: String = helper.chars().filter(|ch| !ch.is_whitespace()).collect();

    assert!(
        compact_helper.contains("assign_relation_outcome(member,index_value_type).related"),
        "union property members should route index-signature value checks through relation outcome"
    );
    assert!(
        compact_helper.contains("assign_relation_outcome(prop_type,index_value_type).related"),
        "property/index value checks should route through relation outcome"
    );
    assert!(
        !helper.contains("diagnostic_relation_boolean_guard"),
        "property/index value checks should not use raw boolean relation guards"
    );
}
