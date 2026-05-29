use std::fs;
use std::path::Path;

#[test]
fn jsx_render_fallback_required_props_use_relation_outcome_boundary() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let source_path =
        Path::new(manifest_dir).join("src/checkers/jsx/extraction_render_fallback.rs");
    let source = fs::read_to_string(source_path).expect("read JSX render fallback source");

    let function_start = source
        .find("fn jsx_construct_return_can_use_render_fallback")
        .expect("find JSX render fallback helper");
    let function = &source[function_start..];

    assert_eq!(
        function.matches("assign_relation_outcome").count(),
        1,
        "JSX render fallback required-prop compatibility should route through relation outcomes"
    );
    assert!(
        !function.contains("diagnostic_relation_boolean_guard"),
        "JSX render fallback required-prop compatibility should not regress to raw boolean relation guards"
    );
}
