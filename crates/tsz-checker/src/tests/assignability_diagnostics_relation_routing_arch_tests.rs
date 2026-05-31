use std::fs;
use std::path::Path;

#[test]
fn assignability_diagnostics_routes_top_level_mismatch_probes_through_relation_outcomes() {
    let source = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/assignability/assignability_diagnostics.rs"),
    )
    .expect("failed to read assignability_diagnostics.rs");

    assert!(
        source.matches("assign_relation_outcome(").count() >= 12,
        "top-level assignability diagnostics should use RelationOutcome for TS2322-family probes"
    );
    assert!(
        source.contains("call_arg_relation_outcome(source, target)"),
        "argument diagnostics should use the TS2345 RelationOutcome path"
    );
    assert!(
        !source.contains("diagnostic_relation_boolean_guard("),
        "top-level assignability diagnostics should not regress to the raw boolean relation guard"
    );
}
