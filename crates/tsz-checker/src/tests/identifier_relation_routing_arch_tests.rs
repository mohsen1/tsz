use std::fs;

#[test]
fn identifier_binding_default_preserve_declared_type_uses_relation_outcome() {
    let source = fs::read_to_string("src/types/computation/identifier/core.rs")
        .expect("failed to read identifier/core.rs");
    let start = source
        .find("request.contextual_type.is_some()")
        .expect("missing binding-default identifier guard");
    let end = start
        + source[start..]
            .find("result_type")
            .expect("missing binding-default guard end");
    let guard: String = source[start..end]
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();

    assert!(
        guard.contains("assign_relation_outcome(flow_type,declared_type).related"),
        "binding-default identifier preservation should route through relation outcomes"
    );
    assert!(
        !guard.contains("is_assignable_to(flow_type,declared_type)"),
        "binding-default identifier preservation should not use raw assignability"
    );
}
