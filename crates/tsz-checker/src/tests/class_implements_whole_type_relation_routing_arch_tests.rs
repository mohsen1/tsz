use std::fs;
use std::path::PathBuf;

#[test]
fn class_implements_whole_type_uses_relation_outcome_boundary() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source =
        fs::read_to_string(manifest_dir.join("src/classes/class_implements_checker/core.rs"))
            .expect("read class implements checker source");

    let helper = source
        .split("let check_whole_type =")
        .nth(1)
        .expect("find class implements whole-type relation block")
        .split("let has_already_reported_missing_member")
        .next()
        .expect("slice whole-type relation block");

    assert!(
        helper.contains("assign_relation_outcome(class_instance_type, target_type)"),
        "class implements whole-type compatibility should route through RelationOutcome"
    );
    assert!(
        helper.contains(".related"),
        "class implements whole-type compatibility should inspect RelationOutcome.related"
    );
    assert!(
        !helper.contains("diagnostic_relation_boolean_guard"),
        "class implements whole-type compatibility should not regress to raw boolean relation guards"
    );
}
