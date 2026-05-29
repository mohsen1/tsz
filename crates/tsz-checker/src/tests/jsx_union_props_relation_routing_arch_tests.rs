use std::fs;
use std::path::Path;

#[test]
fn jsx_union_props_diagnostics_use_relation_outcome_boundary() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let source_path = Path::new(manifest_dir).join("src/checkers/jsx/props/union_props.rs");
    let source = fs::read_to_string(source_path).expect("read JSX union props source");

    let function_start = source
        .find("fn check_jsx_union_props")
        .expect("find JSX union props helper");
    let function_end = function_start
        + source[function_start..]
            .find("fn jsx_props_type_is_library_managed_attributes_application")
            .expect("find next JSX union props helper");
    let function = &source[function_start..function_end];

    assert_eq!(
        function.matches("assign_relation_outcome").count(),
        2,
        "JSX union props compatibility decisions should route through relation outcomes"
    );
    assert!(
        !function.contains("diagnostic_relation_boolean_guard"),
        "JSX union props diagnostics should not regress to raw boolean relation guards"
    );
}
