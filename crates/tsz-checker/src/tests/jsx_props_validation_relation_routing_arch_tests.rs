use std::fs;
use std::path::Path;

#[test]
fn jsx_props_validation_uses_relation_outcome_boundary() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let source_path = Path::new(manifest_dir).join("src/checkers/jsx/props/validation.rs");
    let source = fs::read_to_string(source_path).expect("read JSX props validation source");

    assert_eq!(
        source.matches("jsx_props_relation_outcome(").count(),
        8,
        "JSX props validation relation probes should route through JSX props relation outcomes"
    );
    assert!(
        !source.contains("diagnostic_relation_boolean_guard("),
        "JSX props validation should not regress to raw boolean relation guards"
    );
}

#[test]
fn jsx_props_relation_outcome_uses_jsx_props_request() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let source_path = Path::new(manifest_dir).join("src/assignability/relation_outcome_helpers.rs");
    let source = fs::read_to_string(source_path).expect("read relation outcome helpers source");

    assert!(
        source.contains("fn jsx_props_relation_outcome")
            && source.contains("RelationRequest::jsx_props(source, target)"),
        "JSX props diagnostics should have a request-shaped RelationKind::JsxProps helper"
    );
}
