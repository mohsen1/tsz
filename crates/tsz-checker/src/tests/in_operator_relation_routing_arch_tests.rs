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

fn compact(source: &str) -> String {
    source.split_whitespace().collect::<String>()
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
fn in_operator_rhs_primitive_constraint_uses_relation_outcome_boundary() {
    let source = fs::read_to_string("src/types/computation/binary_support.rs")
        .expect("failed to read binary support source");
    let body = function_body_until(
        &source,
        "fn type_may_represent_primitive(",
        "\n    /// True when `ty` is an `in`-operator RHS shape",
    );

    assert!(
        body.contains("self.assign_relation_outcome(TypeId::STRING, c).related"),
        "`in` operator TS2638 primitive constraint check should route through relation outcome boundary"
    );
    assert!(
        !body.contains("ctx.types.is_assignable_to(TypeId::STRING, c)"),
        "`in` operator TS2638 primitive constraint check should not use a raw solver relation gate"
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
    let compact_body = compact(body);

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
    assert!(
        compact_body.contains("diagnostic_relation_outcome(src,tgt).related"),
        "`instanceof` RHS Function compatibility callback should route through a relation outcome"
    );
    assert!(
        !compact_body.contains("|src,tgt|self.is_assignable_to(src,tgt)"),
        "`instanceof` RHS Function compatibility callback should not call raw boolean assignability"
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

#[test]
fn binary_relational_number_bigint_probes_use_relation_outcome_boundary() {
    let source =
        fs::read_to_string("src/types/computation/binary.rs").expect("failed to read binary.rs");
    let compact_source = compact(&source);

    assert!(
        compact_source.contains("diagnostic_relation_outcome(cmp_left,number_or_bigint).related"),
        "left relational number/bigint probe should route through relation outcome"
    );
    assert!(
        compact_source.contains("diagnostic_relation_outcome(cmp_right,number_or_bigint).related"),
        "right relational number/bigint probe should route through relation outcome"
    );
    assert!(
        !compact_source.contains("is_assignable_to(cmp_left,number_or_bigint)")
            && !compact_source.contains("is_assignable_to(cmp_right,number_or_bigint)"),
        "relational number/bigint probes should not use raw boolean assignability gates"
    );
}

#[test]
fn bigint_exponentiation_target_probes_use_subtype_outcome_boundary() {
    let binary_source =
        fs::read_to_string("src/types/computation/binary.rs").expect("failed to read binary.rs");
    let compound_source = fs::read_to_string("src/assignability/compound_assignment.rs")
        .expect("failed to read compound_assignment.rs");
    let subtype_source = fs::read_to_string("src/assignability/subtype_identity_checker.rs")
        .expect("failed to read subtype_identity_checker.rs");
    let binary_compact = compact(&binary_source);
    let compound_compact = compact(&compound_source);

    assert!(
        subtype_source.contains("fn diagnostic_subtype_outcome("),
        "checker subtype diagnostics should expose an outcome-shaped probe"
    );
    assert!(
        binary_compact.contains("diagnostic_subtype_outcome(left_type,TypeId::BIGINT).related")
            && binary_compact
                .contains("diagnostic_subtype_outcome(right_type,TypeId::BIGINT).related"),
        "binary exponentiation target probes should route through subtype outcomes"
    );
    assert!(
        compound_compact
            .contains("diagnostic_subtype_outcome(left_read_type,TypeId::BIGINT).related")
            && compound_compact
                .contains("diagnostic_subtype_outcome(right_type,TypeId::BIGINT).related"),
        "compound exponentiation target probes should route through subtype outcomes"
    );
    assert!(
        !binary_compact.contains("is_subtype_of(left_type,TypeId::BIGINT)")
            && !binary_compact.contains("is_subtype_of(right_type,TypeId::BIGINT)")
            && !compound_compact.contains("is_subtype_of(left_read_type,TypeId::BIGINT)")
            && !compound_compact.contains("is_subtype_of(right_type,TypeId::BIGINT)"),
        "TS2791 bigint target probes should not consume raw subtype booleans directly"
    );
}
