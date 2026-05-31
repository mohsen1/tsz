use std::fs;

fn function_body_until<'a>(source: &'a str, signature: &str, boundary: &str) -> &'a str {
    let start = source
        .find(signature)
        .expect("expected function signature in source");
    let rest = &source[start..];
    let end = rest
        .find(boundary)
        .expect("expected next function boundary");
    &rest[..end]
}

#[test]
fn satisfies_diagnostic_uses_satisfies_relation_outcome() {
    let diagnostics = fs::read_to_string("src/assignability/assignability_diagnostics.rs")
        .expect("failed to read assignability diagnostics source");
    let helper = fs::read_to_string("src/assignability/relation_outcome_helpers.rs")
        .expect("failed to read relation outcome helper source");
    let body = function_body_until(
        &diagnostics,
        "pub(crate) fn check_satisfies_assignable_or_report(",
        "\n    /// Elaborate a `satisfies` failure",
    );

    assert!(
        helper.contains("fn satisfies_relation_outcome(")
            && helper.contains("RelationRequest::satisfies(source, target)"),
        "`satisfies` diagnostics should have a request-shaped RelationKind::Satisfies helper"
    );
    assert!(
        body.matches("satisfies_relation_outcome(").count() >= 2,
        "`satisfies` diagnostics and readonly fallback should route through the satisfies helper"
    );
    assert!(
        !body.contains("assign_relation_outcome(source, target)")
            && !body.contains("assign_relation_outcome(inner, target)"),
        "`satisfies` diagnostics should not use the generic assign relation outcome"
    );
}
