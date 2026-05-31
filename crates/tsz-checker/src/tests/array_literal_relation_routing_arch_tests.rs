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
            .find("self.is_subtype_of(elem_type, context_element_type)")
            .expect("missing structural subtype fallback");
    let branch: String = source[start..end]
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();

    assert!(
        branch.contains("assign_relation_outcome(elem_type,context_element_type).related"),
        "array literal contextual collapse override probes should use relation outcomes"
    );
    assert!(
        !branch.contains("is_assignable_to(elem_type,context_element_type)"),
        "array literal contextual collapse should not use raw assignability for override probes"
    );
}
