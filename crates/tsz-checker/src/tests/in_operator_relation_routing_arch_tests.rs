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

fn trailing_function_body<'a>(source: &'a str, signature: &str) -> &'a str {
    let start = source
        .find(signature)
        .expect("expected function signature in source");
    &source[start..]
}

#[test]
fn in_operator_lhs_key_diagnostic_uses_relation_outcome_boundary() {
    let source = fs::read_to_string("src/types/computation/binary_support.rs")
        .expect("failed to read binary support source");
    let body = function_body_until(
        &source,
        "fn check_in_operator_lhs_key_type(",
        "\n    /// Check the `in` operator.",
    );

    assert!(
        body.contains("self.assign_relation_outcome(key_type, target).related"),
        "`in` operator TS2322 key check should route through relation outcome boundary"
    );
    assert!(
        !body.contains("self.is_assignable_to(key_type, target)"),
        "`in` operator TS2322 key check should not use a raw boolean assignability gate"
    );
}

#[test]
fn binary_instanceof_symbol_hasinstance_relations_use_relation_outcomes() {
    let source = fs::read_to_string("src/types/computation/binary_support.rs")
        .expect("failed to read binary support source");
    let body = function_body_until(
        &source,
        "pub(super) fn check_instanceof_operator(",
        "\n    /// Validate that the left operand of `in`",
    );

    assert_eq!(
        body.matches("assign_relation_outcome(").count(),
        2,
        "`instanceof` Symbol.hasInstance return and parameter checks should route through relation outcomes"
    );
    assert!(
        body.contains("assign_relation_outcome(ret, TypeId::BOOLEAN).related"),
        "`instanceof` Symbol.hasInstance return check should use a relation outcome"
    );
    assert!(
        body.contains("assign_relation_outcome(lhs_type, param_type).related"),
        "`instanceof` Symbol.hasInstance parameter check should use a relation outcome"
    );
    assert!(
        !body.contains("is_assignable_to(ret, TypeId::BOOLEAN)")
            && !body.contains("is_assignable_to(lhs_type, param_type)"),
        "`instanceof` Symbol.hasInstance checks should not use raw boolean assignability gates"
    );
}

#[test]
fn indexed_access_binary_arithmetic_uses_relation_outcomes() {
    let source = fs::read_to_string("src/types/computation/binary_support.rs")
        .expect("failed to read binary support source");
    let body = trailing_function_body(&source, "pub(super) fn resolve_indexed_access_binary_op(");

    assert_eq!(
        body.matches("assign_relation_outcome(").count(),
        2,
        "indexed-access arithmetic probes should route through relation outcomes"
    );
    assert!(
        body.contains("assign_relation_outcome(left, TypeId::NUMBER).related")
            && body.contains("assign_relation_outcome(right, TypeId::NUMBER).related"),
        "indexed-access arithmetic probes should use relation outcome decisions"
    );
    assert!(
        !body.contains("is_assignable_to(left, TypeId::NUMBER)")
            && !body.contains("is_assignable_to(right, TypeId::NUMBER)"),
        "indexed-access arithmetic probes should not use raw boolean assignability gates"
    );
}
