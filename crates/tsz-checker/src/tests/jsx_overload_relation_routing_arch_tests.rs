use std::fs;

#[test]
fn jsx_overload_matching_uses_relation_outcome_boundary() {
    let source = fs::read_to_string("src/checkers/jsx/overloads.rs")
        .expect("failed to read JSX overload source");
    let start = source
        .find("fn jsx_attrs_match_overload")
        .expect("missing JSX attrs overload matcher");
    let end = source[start..]
        .find("/// Build an object type from collected JSX attribute info.")
        .expect("missing attrs object builder")
        + start;
    let helpers = &source[start..end];

    assert_eq!(
        helpers.matches("assign_relation_outcome").count(),
        5,
        "JSX overload relation checks should route through assign_relation_outcome"
    );
    assert!(
        helpers.contains(".related"),
        "JSX overload relation checks should use the relation outcome decision"
    );
    assert!(
        !helpers.contains("diagnostic_relation_boolean_guard"),
        "JSX overload matching should not regress to the raw boolean relation guard"
    );
}
