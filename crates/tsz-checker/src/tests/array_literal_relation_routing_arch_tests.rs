use std::fs;

#[test]
fn array_literal_contextual_collapse_uses_relation_outcome_boundary() {
    let source = fs::read_to_string("src/types/computation/array_literal.rs")
        .expect("failed to read array_literal.rs");
    let start = source
        .find("context_requires_assignability_overrides")
        .expect("missing contextual collapse override probe");
    let end = start
        + source[start..]
            .find("self.check_object_literal_excess_properties(")
            .expect("missing excess property check after contextual collapse");
    let branch: String = source[start..end]
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();

    assert!(
        branch.contains(
            "array_literal_contextual_collapse_relation_outcome(elem_type,context_element_type,).related"
        ),
        "array literal contextual collapse override probes should use the named contextual-collapse relation outcome"
    );
    assert!(
        branch.contains(
            "array_literal_contextual_collapse_subtype_outcome(elem_type,context_element_type,).related"
        ),
        "array literal contextual collapse structural fallback should use the named subtype outcome"
    );
    assert!(
        !branch.contains("is_assignable_to(elem_type,context_element_type)"),
        "array literal contextual collapse should not use raw assignability for override probes"
    );
    assert!(
        !branch.contains("is_subtype_of(elem_type,context_element_type)"),
        "array literal contextual collapse should not use raw subtype checks in type computation"
    );
    assert!(
        !branch.contains("assign_relation_outcome(elem_type,context_element_type)"),
        "array literal contextual collapse should not use generic assignment request routing"
    );
}

#[test]
fn array_literal_contextual_collapse_relation_outcome_uses_dedicated_request() {
    let source = fs::read_to_string("src/assignability/relation_outcome_helpers.rs")
        .expect("failed to read relation_outcome_helpers.rs");

    assert!(
        source.contains("fn array_literal_contextual_collapse_relation_outcome(")
            && source.contains("RelationRequest::array_literal_contextual_collapse("),
        "array literal contextual collapse should build a dedicated RelationRequest"
    );
    assert!(
        source.contains("fn array_literal_contextual_collapse_subtype_outcome(")
            && source.contains("self.diagnostic_subtype_outcome(source, target)"),
        "array literal contextual collapse structural fallback should use a named subtype outcome"
    );
}
