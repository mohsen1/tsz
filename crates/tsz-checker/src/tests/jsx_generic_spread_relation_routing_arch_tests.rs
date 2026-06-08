use std::fs;
use std::path::Path;

#[test]
fn jsx_generic_spread_assignability_uses_relation_outcome_boundary() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let source_path = Path::new(manifest_dir).join("src/checkers/jsx/props/generic_spread.rs");
    let source = fs::read_to_string(source_path).expect("read jsx generic_spread.rs");

    let function_start = source
        .find("fn report_invalid_generic_jsx_spread_assignability")
        .expect("find generic JSX spread assignability reporter");
    let function = &source[function_start..];

    assert!(
        !function.contains("jsx_props_relation_outcome"),
        "generic JSX spread assignability decisions should route through the JSX props boundary helper"
    );
    assert!(
        function.contains("checkers::jsx::props_are_assignable("),
        "generic JSX spread assignability should use query_boundaries::checkers::jsx::props_are_assignable"
    );
    assert!(
        !function.contains("diagnostic_relation_boolean_guard"),
        "generic JSX spread assignability should not regress to raw boolean relation guards"
    );
}

#[test]
fn jsx_generic_spread_validation_uses_relation_outcome_boundary() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let source_path = Path::new(manifest_dir).join("src/checkers/jsx/props/validation.rs");
    let source = fs::read_to_string(source_path).expect("read jsx validation.rs");

    let function_start = source
        .find("pub(in crate::checkers_domain::jsx) fn check_jsx_generic_spread_attrs_assignability")
        .expect("find generic spread attrs validation helper");
    let rest = &source[function_start..];
    let function_end = rest
        .find("pub(in crate::checkers_domain::jsx) fn normalize_jsx_required_props_target")
        .expect("find next JSX validation helper");
    let function = &rest[..function_end];
    assert!(
        !function.contains("jsx_props_relation_outcome("),
        "generic JSX spread attrs validation should route through the JSX props boundary helper"
    );
    assert!(
        function.matches("props_are_assignable(").count() >= 2,
        "generic JSX spread attrs validation should use query_boundaries::checkers::jsx::props_are_assignable"
    );
    assert!(
        !function.contains("diagnostic_relation_boolean_guard"),
        "generic JSX spread attrs validation should not use raw boolean relation guards"
    );
}
