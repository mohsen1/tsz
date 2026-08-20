use std::fs;

#[test]
fn identifier_binding_default_preserve_declared_type_uses_relation_outcome() {
    let source = fs::read_to_string("src/types/computation/identifier/resolved.rs")
        .expect("failed to read identifier/resolved.rs");
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
        guard.contains(
            "identifier_binding_default_relation_outcome(flow_type,declared_type).related"
        ),
        "binding-default identifier preservation should route through the identifier binding-default relation request"
    );
    assert!(
        !guard.contains("assign_relation_outcome(flow_type,declared_type)")
            && !guard.contains("is_assignable_to(flow_type,declared_type)"),
        "binding-default identifier preservation should not use generic or raw assignability"
    );
}
