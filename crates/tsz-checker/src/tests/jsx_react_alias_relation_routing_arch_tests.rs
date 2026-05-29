use std::fs;
use std::path::Path;

#[test]
fn jsx_react_props_alias_storage_uses_relation_outcome_boundary() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let source_path = Path::new(manifest_dir).join("src/checkers/jsx/extraction_react_alias.rs");
    let source = fs::read_to_string(source_path).expect("read JSX React alias source");

    let function_start = source
        .find("fn store_jsx_props_display_alias_if_matching")
        .expect("find JSX props display alias helper");
    let function_end = function_start
        + source[function_start..]
            .find("fn jsx_type_contains_callable_surface")
            .expect("find next JSX React alias helper");
    let function = &source[function_start..function_end];

    assert_eq!(
        function.matches("assign_relation_outcome").count(),
        2,
        "JSX props display-alias storage should route both compatibility decisions through relation outcomes"
    );
    assert!(
        !function.contains("diagnostic_relation_boolean_guard"),
        "JSX props display-alias storage should not regress to raw boolean relation guards"
    );
}
