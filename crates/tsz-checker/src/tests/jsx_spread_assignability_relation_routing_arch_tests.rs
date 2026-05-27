use std::fs;
use std::path::Path;

#[test]
fn jsx_spread_whole_type_assignability_uses_relation_outcome_boundary() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let source_path = Path::new(manifest_dir).join("src/checkers/jsx/spread.rs");
    let source = fs::read_to_string(source_path).expect("read JSX spread source");

    let function_start = source
        .find("fn check_spread_property_types")
        .expect("find JSX spread property checker");
    let shape_branch_end = function_start
        + source[function_start..]
            .find("// When there are multiple spreads")
            .expect("find JSX spread shape branch end");
    let prefix = &source[function_start..shape_branch_end];

    assert_eq!(
        prefix.matches("assign_relation_outcome").count(),
        2,
        "early JSX spread whole-type compatibility decisions should route through relation outcomes"
    );
    assert!(
        !prefix.contains("diagnostic_relation_boolean_guard"),
        "early JSX spread whole-type compatibility should not regress to raw boolean relation guards"
    );
}
