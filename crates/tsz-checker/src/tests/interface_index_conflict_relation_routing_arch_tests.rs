use std::fs;
use std::path::PathBuf;

#[test]
fn interface_index_conflicts_use_relation_outcome_boundary() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source = fs::read_to_string(manifest_dir.join("src/classes/class_checker_compat.rs"))
        .expect("read class checker compatibility source");

    let helper = source
        .split("Conflicting index signatures from different bases")
        .nth(1)
        .expect("find inherited interface index-signature conflict block")
        .split("incorrectly extends interface")
        .next()
        .expect("slice relation decision block");
    let compact: String = helper.chars().filter(|c| !c.is_whitespace()).collect();

    assert!(
        compact.contains(
            "interface_heritage_index_value_relation_outcome(prev_val,value_type,).related"
        ),
        "inherited interface index conflict checks should route previous-to-current relations through the interface heritage index-value request"
    );
    assert!(
        compact.contains(
            "interface_heritage_index_value_relation_outcome(value_type,prev_val,).related"
        ),
        "inherited interface index conflict checks should route current-to-previous relations through the interface heritage index-value request"
    );
    assert!(
        !compact.contains("assign_relation_outcome(prev_val,value_type)")
            && !compact.contains("assign_relation_outcome(value_type,prev_val)"),
        "inherited interface index conflict checks should not use generic assignment relation outcomes"
    );
    assert!(
        !helper.contains("diagnostic_relation_boolean_guard"),
        "inherited interface index conflict checks should not regress to raw boolean relation guards"
    );
}
