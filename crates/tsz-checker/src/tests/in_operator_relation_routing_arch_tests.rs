use std::fs;

fn function_body<'a>(source: &'a str, signature: &str) -> &'a str {
    let start = source
        .find(signature)
        .expect("expected function signature in source");
    let rest = &source[start..];
    let end = rest
        .find("\n    /// Check the `in` operator.")
        .expect("expected next function boundary");
    &rest[..end]
}

#[test]
fn in_operator_lhs_key_diagnostic_uses_relation_outcome_boundary() {
    let source = fs::read_to_string("src/types/computation/binary_operator_diagnostics.rs")
        .expect("failed to read binary operator diagnostics source");
    let body = function_body(&source, "pub(super) fn check_in_operator_lhs_key_type(");

    assert!(
        body.contains("self.assign_relation_outcome(key_type, target).related"),
        "`in` operator TS2322 key check should route through relation outcome boundary"
    );
    assert!(
        !body.contains("self.is_assignable_to(key_type, target)"),
        "`in` operator TS2322 key check should not use a raw boolean assignability gate"
    );
}
