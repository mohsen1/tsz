use std::fs;
use std::path::Path;

#[test]
fn jsx_props_resolution_uses_relation_outcome_boundary() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let source_path = Path::new(manifest_dir).join("src/checkers/jsx/props/resolution.rs");
    let source = fs::read_to_string(source_path).expect("read JSX props resolution source");

    assert_eq!(
        source.matches("assign_relation_outcome(").count(),
        8,
        "JSX props resolution relation probes should route through relation outcomes"
    );
    assert!(
        !source.contains("diagnostic_relation_boolean_guard("),
        "JSX props resolution should not regress to raw boolean relation guards"
    );
}
