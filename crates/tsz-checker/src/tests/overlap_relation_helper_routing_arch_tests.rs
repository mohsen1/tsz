use std::fs;
use std::path::PathBuf;

#[test]
fn overlap_relation_helpers_use_relation_outcome_boundary() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source =
        fs::read_to_string(manifest_dir.join("src/types/utilities/overlap_relation_helpers.rs"))
            .expect("read overlap relation helper source");

    assert!(
        source.contains("assign_relation_outcome(left, right).related"),
        "left-to-right overlap assignability should route through RelationOutcome"
    );
    assert!(
        source.contains("assign_relation_outcome(right, left).related"),
        "right-to-left overlap assignability should route through RelationOutcome"
    );
    assert!(
        source.contains("assign_relation_outcome(lt, rt).related"),
        "signature parameter overlap should route through RelationOutcome"
    );
    assert!(
        source.contains("assign_relation_outcome(lret, rret).related"),
        "signature return overlap should route through RelationOutcome"
    );
    assert!(
        !source.contains("diagnostic_relation_boolean_guard"),
        "diagnostic overlap helpers should not regress to raw boolean relation guards"
    );
}
