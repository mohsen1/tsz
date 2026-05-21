use tsz_checker::test_utils::check_source_strict_codes;

fn assert_no_ts2339(source: &str) {
    let codes = check_source_strict_codes(source);
    assert!(
        !codes.contains(&2339),
        "expected discriminant narrowing to avoid TS2339, got {codes:?}"
    );
}

#[test]
fn literal_typed_parameter_rhs_narrows_discriminant_union() {
    assert_no_ts2339(
        r#"
type A = { k: "a"; a: number };
type B = { k: "b"; b: string };
function f(x: A | B, key: "a") {
    if (x.k === key) {
        const value: number = x.a;
    }
}
"#,
    );
}

#[test]
fn annotated_const_rhs_narrows_discriminant_union() {
    assert_no_ts2339(
        r#"
type A = { k: "a"; a: number };
type B = { k: "b"; b: string };
function f(x: A | B) {
    const key: "a" = "a";
    if (x.k === key) {
        const value: number = x.a;
    }
}
"#,
    );
}

#[test]
fn renamed_literal_typed_reference_rhs_narrows_discriminant_union() {
    assert_no_ts2339(
        r#"
type Left = { tag: "left"; payload: number };
type Right = { tag: "right"; payload: string };
function f(item: Left | Right, expectedTag: "left") {
    if (item.tag === expectedTag) {
        const value: number = item.payload;
    }
}
"#,
    );
}

#[test]
fn literal_expression_and_inferred_const_controls_still_narrow() {
    assert_no_ts2339(
        r#"
type A = { kind: "a"; a: number };
type B = { kind: "b"; b: string };
function literalExpr(x: A | B) {
    if (x.kind === "a") {
        const value: number = x.a;
    }
}
function inferredConst(x: A | B) {
    const key = "a";
    if (x.kind === key) {
        const value: number = x.a;
    }
}
"#,
    );
}

#[test]
fn primitive_typed_reference_rhs_does_not_narrow_to_one_member() {
    let codes = check_source_strict_codes(
        r#"
type A = { k: "a"; a: number };
type B = { k: "b"; b: string };
function f(x: A | B, key: string) {
    if (x.k === key) {
        const value: number = x.a;
    }
}
"#,
    );
    assert!(
        codes.contains(&2339),
        "primitive-typed RHS must not act as a unit discriminant, got {codes:?}"
    );
}
