use std::fs;
use std::path::Path;

#[test]
fn jsx_props_resolution_uses_relation_outcome_boundary() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let source_path = Path::new(manifest_dir).join("src/checkers/jsx/props/resolution.rs");
    let source = fs::read_to_string(source_path).expect("read JSX props resolution source");

    assert!(
        !source.contains("jsx_props_relation_outcome("),
        "JSX props resolution relation probes should route through the JSX props boundary helper"
    );
    assert!(
        source.contains("checkers::jsx::props_are_assignable("),
        "JSX props resolution should use query_boundaries::checkers::jsx::props_are_assignable"
    );
    assert!(
        !source.contains("assign_relation_outcome("),
        "JSX props resolution should not regress to generic assignment relation routing"
    );
    assert!(
        !source.contains("diagnostic_relation_boolean_guard("),
        "JSX props resolution should not regress to raw boolean relation guards"
    );
}
