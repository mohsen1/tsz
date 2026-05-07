//! Chained `in` checks against a bare type parameter must narrow after the
//! first check so the second `in` operand sees a valid `in`-RHS type.
//!
//! Without narrowing, every `"x" in t` (where `t: T`, no constraint) re-emits
//! an invalid-RHS diagnostic because `T` may represent a primitive. tsc only
//! emits that diagnostic once at the *first* `in` operand; the truthy branch
//! narrows `t` to `T & Record<"x", unknown>`, satisfying the `in`-RHS validity
//! check for every chained operand to its right.

use tsz_common::options::checker::CheckerOptions;

fn diags(source: &str) -> Vec<crate::diagnostics::Diagnostic> {
    let opts = CheckerOptions {
        strict: true,
        strict_null_checks: true,
        ..CheckerOptions::default()
    };
    crate::test_utils::check_source(source, "test.ts", opts)
}

fn count_ts2638_invalid_rhs(diags: &[crate::diagnostics::Diagnostic]) -> usize {
    diags
        .iter()
        .filter(|d| d.code == 2638 && d.message_text.contains("right operand"))
        .count()
}

#[test]
fn chained_in_against_bare_type_param_emits_ts2638_once() {
    let d = diags(
        r#"
function test<T>(x: T) {
    return "a" in x && "b" in x;
}
"#,
    );
    assert_eq!(
        count_ts2638_invalid_rhs(&d),
        1,
        "Expected exactly one TS2638 invalid-RHS diagnostic for `\"a\" in x && \"b\" in x`; got: {d:?}"
    );
}

#[test]
fn three_chain_in_against_bare_type_param_emits_ts2638_once() {
    let d = diags(
        r#"
function test<T>(x: T) {
    return "a" in x && "b" in x && "c" in x;
}
"#,
    );
    assert_eq!(
        count_ts2638_invalid_rhs(&d),
        1,
        "Expected exactly one TS2638 invalid-RHS diagnostic for chain of three `in`s; got: {d:?}"
    );
}

#[test]
fn solo_in_against_bare_type_param_still_emits_ts2638() {
    let d = diags(
        r#"
function test<T>(x: T) {
    return "a" in x;
}
"#,
    );
    assert_eq!(
        count_ts2638_invalid_rhs(&d),
        1,
        "Solo `\"a\" in x` should still emit one TS2638 against bare T; got: {d:?}"
    );
}

#[test]
fn constrained_type_param_in_chain_still_emits_no_extra() {
    // T extends object — the first `in` is valid, no invalid-RHS diagnostic anywhere.
    let d = diags(
        r#"
function test<T extends object>(x: T) {
    return "a" in x && "b" in x;
}
"#,
    );
    assert_eq!(
        count_ts2638_invalid_rhs(&d),
        0,
        "T extends object -> no TS2638; got: {d:?}"
    );
}

#[test]
fn negated_in_against_bare_type_param_still_returns_source() {
    // Negative narrowing of a bare type parameter must NOT intersect with
    // Record (the property is being asserted absent), so this should be a
    // no-op narrowing. Verify nothing about the bare-T branch regresses.
    let d = diags(
        r#"
function test<T>(x: T) {
    if (!("a" in x)) {
        return x;
    }
    return null;
}
"#,
    );
    // Single TS2638 from the `in` operator itself; no cascade in the false branch.
    assert_eq!(
        count_ts2638_invalid_rhs(&d),
        1,
        "Expected one TS2638 from the `in` operator only; got: {d:?}"
    );
}
