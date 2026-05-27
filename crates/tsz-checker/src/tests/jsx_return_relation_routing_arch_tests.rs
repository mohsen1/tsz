use std::fs;
use std::path::Path;

fn jsx_extraction_source() -> String {
    let source_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/checkers/jsx/extraction.rs");
    fs::read_to_string(source_path).expect("read JSX extraction source")
}

fn function_source<'a>(source: &'a str, name: &str, next_name: &str) -> &'a str {
    let function_start = source
        .find(name)
        .unwrap_or_else(|| panic!("find {name} helper"));
    let rest = &source[function_start..];
    let function_end = rest
        .find(next_name)
        .unwrap_or_else(|| panic!("find {next_name} helper"));
    &rest[..function_end]
}

#[test]
fn jsx_component_return_checks_use_relation_outcome_boundary() {
    let source = jsx_extraction_source();

    let element_type_callable_return = function_source(
        &source,
        "fn jsx_component_satisfies_element_type_callable_return",
        "pub(super) fn jsx_construct_return_satisfies_element_class_render",
    );
    assert!(
        element_type_callable_return
            .contains("assign_relation_outcome(*source_return, *target_return)"),
        "`JSX.ElementType` callable return matching must use the relation outcome boundary"
    );
    assert!(
        !element_type_callable_return.contains("diagnostic_relation_boolean_guard"),
        "`JSX.ElementType` callable return matching must not use raw diagnostic boolean guards"
    );

    let construct_render = function_source(
        &source,
        "pub(super) fn jsx_construct_return_satisfies_element_class_render",
        "pub(super) fn check_jsx_component_return_type",
    );
    assert!(
        construct_render.contains("assign_relation_outcome(source_render, target_render)")
            && construct_render.contains("assign_relation_outcome(source_return, target_return)"),
        "`JSX.ElementClass.render` compatibility must use the relation outcome boundary"
    );
    assert!(
        !construct_render.contains("diagnostic_relation_boolean_guard"),
        "`JSX.ElementClass.render` compatibility must not use raw diagnostic boolean guards"
    );

    let component_return = function_source(
        &source,
        "pub(super) fn check_jsx_component_return_type",
        "pub(super) fn check_jsx_sfc_return_type",
    );
    assert!(
        component_return.contains("assign_relation_outcome(non_null_return, element_type)")
            && component_return.contains("assign_relation_outcome(check_ret, t).related"),
        "JSX component return diagnostics must use the relation outcome boundary"
    );
    assert!(
        !component_return.contains("diagnostic_relation_boolean_guard"),
        "JSX component return diagnostics must not use raw diagnostic boolean guards"
    );

    let sfc_return = function_source(
        &source,
        "pub(super) fn check_jsx_sfc_return_type",
        "fn report_invalid_jsx_component_return_type",
    );
    assert!(
        sfc_return.contains("assign_relation_outcome(non_null_return, jsx_element_type)"),
        "JSX SFC return diagnostics must use the relation outcome boundary"
    );
    assert!(
        !sfc_return.contains("diagnostic_relation_boolean_guard"),
        "JSX SFC return diagnostics must not use raw diagnostic boolean guards"
    );
}
