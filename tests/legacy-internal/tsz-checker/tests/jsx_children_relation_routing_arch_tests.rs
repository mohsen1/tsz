use std::fs;
use std::path::Path;

fn jsx_children_source() -> String {
    let source_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/checkers/jsx/children.rs");
    fs::read_to_string(source_path).expect("read JSX children source")
}

fn helper_source<'a>(source: &'a str, name: &str, next_name: &str) -> &'a str {
    let start = source
        .find(name)
        .unwrap_or_else(|| panic!("find {name} helper"));
    let rest = &source[start..];
    let end = rest
        .find(next_name)
        .unwrap_or_else(|| panic!("find {next_name} helper"));
    &rest[..end]
}

#[test]
fn jsx_children_assignability_helpers_use_relation_outcome_boundary() {
    let source = jsx_children_source();

    let single_satisfies = helper_source(
        &source,
        "fn single_jsx_child_satisfies_children_type",
        "fn single_jsx_child_is_function_like",
    );
    assert!(
        single_satisfies
            .contains("jsx_children_relation_outcome(actual_child_type, children_type)"),
        "single JSX child compatibility must use the JSX children relation outcome boundary"
    );

    let text_acceptance = helper_source(
        &source,
        "fn children_type_accepts_text",
        "fn check_jsx_multiple_children_assignable",
    );
    assert!(
        text_acceptance.contains("jsx_children_relation_outcome(TypeId::STRING, children_type)"),
        "JSX text-child compatibility must use the JSX children relation outcome boundary"
    );

    let multiple_assignability = helper_source(
        &source,
        "fn check_jsx_multiple_children_assignable",
        "fn check_jsx_single_child_assignable",
    );
    assert!(
        multiple_assignability
            .contains("jsx_children_relation_outcome(actual_children_type, children_type)"),
        "JSX multiple-children aggregate compatibility must use the JSX children relation outcome boundary"
    );

    let single_assignability = helper_source(
        &source,
        "fn check_jsx_single_child_assignable",
        "fn rewrite_recent_jsx_element_source_display",
    );
    assert!(
        single_assignability
            .contains("jsx_children_relation_outcome(actual_child_type, children_type)"),
        "JSX single-child diagnostics must use the JSX children relation outcome boundary"
    );

    let individual_assignability = helper_source(
        &source,
        "fn report_jsx_multiple_children_individual_assignability",
        "fn normalize_recent_jsx_children_union_diagnostics",
    );
    assert!(
        individual_assignability
            .contains("jsx_children_relation_outcome(actual_child_type, expected_child_type)")
            && individual_assignability.contains(
                "jsx_children_relation_outcome(actual_child_type, original_children_type)"
            ),
        "JSX per-child diagnostics must use the JSX children relation outcome boundary"
    );

    let class_compatibility = helper_source(
        &source,
        "fn report_jsx_single_child_constructor_instance_mismatch",
        "fn get_precise_jsx_children_body_type",
    );
    assert!(
        class_compatibility
            .contains("jsx_children_relation_outcome(resolved_instance, resolved_target)")
            && class_compatibility
                .contains("jsx_children_relation_outcome(resolved_target, resolved_instance)"),
        "JSX child class compatibility must use JSX children relation outcome boundary probes"
    );

    let multiple_allowed = helper_source(
        &source,
        "pub(super) fn type_allows_multiple_children",
        "pub(super) fn type_requires_multiple_children",
    );
    assert!(
        multiple_allowed.contains("jsx_children_relation_outcome(array_of_children, type_id)")
            && multiple_allowed.contains(".related"),
        "JSX multiple-children fallback compatibility must use the JSX children relation outcome boundary"
    );

    let relation_helper_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/assignability/relation_outcome_helpers.rs");
    let relation_helper =
        fs::read_to_string(relation_helper_path).expect("read relation outcome helper source");
    assert!(
        relation_helper.contains("fn jsx_children_relation_outcome")
            && relation_helper.contains("RelationRequest::jsx_children(source, target)"),
        "JSX children diagnostics should have a request-shaped RelationKind::JsxChildren helper"
    );

    assert!(
        !source.contains("diagnostic_relation_boolean_guard"),
        "JSX children relation diagnostics should not use raw diagnostic boolean guards"
    );
}
