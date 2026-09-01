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
    let compact_helpers: String = helpers.chars().filter(|ch| !ch.is_whitespace()).collect();

    assert_eq!(
        helpers.matches("props_are_assignable").count(),
        6,
        "JSX component prop tag relation checks should route through the props_are_assignable boundary"
    );
    assert!(
        !compact_helpers.contains("assign_relation_outcome(tag_literal,key_type)")
            && !compact_helpers.contains("assign_relation_outcome(candidate_key,best_key)")
            && !compact_helpers.contains("assign_relation_outcome(best_key,candidate_key)")
            && !compact_helpers.contains("assign_relation_outcome(tag_type,evaluated)"),
        "JSX component prop tag relation checks should not use the generic assignment request"
    );
    assert!(
        !helpers.contains("diagnostic_relation_boolean_guard"),
        "JSX component prop tag relation checks should not regress to the raw boolean relation guard"
    );
}
