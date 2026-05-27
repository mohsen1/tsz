use std::fs;

#[test]
fn jsx_component_props_tag_relations_use_relation_outcome_boundary() {
    let source = fs::read_to_string("src/checkers/jsx/orchestration/component_props.rs")
        .expect("failed to read JSX component props orchestration source");
    let start = source
        .find("pub(in crate::checkers_domain::jsx) fn get_jsx_intrinsic_props_from_template_literal_index_signatures")
        .expect("missing template literal intrinsic props helper");
    let end = source[start..]
        .find("fn jsx_element_type_for_validation")
        .expect("missing JSX element type validation helper")
        + start;
    let helpers = &source[start..end];

    assert_eq!(
        helpers.matches("assign_relation_outcome").count(),
        6,
        "JSX component prop tag relation checks should route through assign_relation_outcome"
    );
    assert!(
        helpers.contains(".related"),
        "JSX component prop tag relation checks should use the relation outcome decision"
    );
    assert!(
        !helpers.contains("diagnostic_relation_boolean_guard"),
        "JSX component prop tag relation checks should not regress to the raw boolean relation guard"
    );
}
