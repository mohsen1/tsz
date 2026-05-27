use std::fs;
use std::path::Path;

#[test]
fn index_signature_value_checks_use_relation_outcome_boundary() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let source_path = Path::new(manifest_dir)
        .join("src/state/state_checking_members/index_signature_key_helpers.rs");
    let source = fs::read_to_string(source_path).expect("read index_signature_key_helpers.rs");

    let function_start = source
        .find("fn property_type_assignable_to_index_type")
        .expect("find property/index assignability helper");
    let function_end = source[function_start..]
        .find("pub(crate) fn format_ts2411_type")
        .expect("find next helper");
    let helper = &source[function_start..function_start + function_end];

    assert!(
        helper.contains(".assign_relation_outcome(member, index_value_type)")
            && helper.contains(".assign_relation_outcome(prop_type, index_value_type)")
            && helper.matches(".related").count() >= 2,
        "index-signature value relation decisions should use relation outcomes"
    );
    assert!(
        !helper.contains("diagnostic_relation_boolean_guard"),
        "index-signature value helper should not fall back to raw boolean relation guards"
    );
}
