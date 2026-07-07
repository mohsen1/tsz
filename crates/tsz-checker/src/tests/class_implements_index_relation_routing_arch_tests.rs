use std::fs;
use std::path::PathBuf;

#[test]
fn class_implements_index_signatures_use_relation_outcome_boundary() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source =
        fs::read_to_string(manifest_dir.join("src/classes/class_implements_checker/core.rs"))
            .expect("read class implements checker source");

    let helper = source
        .split("fn class_index_signatures_satisfy_interface")
        .nth(1)
        .expect("find class implements index-signature helper")
        .split("fn class_member_name_is_computed")
        .next()
        .expect("slice helper body before next helper");
    let compact = helper.split_whitespace().collect::<String>();

    assert!(
        source.contains("fn class_implements_index_signature_value_satisfies("),
        "class implements index signature checks should use a dedicated value-relation helper"
    );
    assert!(
        compact.contains(
            "class_implements_index_signature_value_satisfies(source_index,target_index)"
        ),
        "class implements index signature branches should use the dedicated value-relation helper"
    );
    assert!(
        source.contains(
            "class_implements_index_value_relation_outcome(\n            source_index.value_type,"
        ),
        "class implements index signature value checks should route through the dedicated relation outcome"
    );
    assert!(
        helper.contains("string_index_signature()")
            && helper.contains("number_index")
            && helper.contains("symbol_index_signature()"),
        "class implements index signature checks should cover string, number, and symbol index requirements"
    );
    assert!(
        source.contains(".related"),
        "class implements index signature checks should inspect the relation outcome"
    );
    assert!(
        !helper.contains("assign_relation_outcome(")
            && !helper.contains("diagnostic_relation_boolean_guard"),
        "class implements index signature checks should not regress to generic assign or raw boolean relation guards"
    );
}
