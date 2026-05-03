//! Chained `in` checks against a bare type parameter must narrow after the
//! first check so the second `in` operand sees a valid `in`-RHS type.
//!
//! Without narrowing, every `"x" in t` (where `t: T`, no constraint) re-emits
//! TS2322 because `T` itself is not assignable to `object`. tsc only emits
//! TS2322 once at the *first* `in` operand; the truthy branch narrows `t` to
//! `T & Record<"x", unknown>`, satisfying the `in`-RHS validity check for
//! every chained operand to its right.

use tsz_common::options::checker::CheckerOptions;

fn diags(source: &str) -> Vec<crate::diagnostics::Diagnostic> {
    let opts = CheckerOptions {
        strict: true,
        strict_null_checks: true,
        ..CheckerOptions::default()
    };
    crate::test_utils::check_source(source, "test.ts", opts)
}

fn count_ts2322_object(diags: &[crate::diagnostics::Diagnostic]) -> usize {
    diags
        .iter()
        .filter(|d| d.code == 2322 && d.message_text.contains("not assignable to type 'object'"))
        .count()
}

#[test]
fn chained_in_against_bare_type_param_emits_ts2322_once() {
    let d = diags(
        r#"
function test<T>(x: T) {
    return "a" in x && "b" in x;
}
"#,
    );
    assert_eq!(
        count_ts2322_object(&d),
        1,
        "Expected exactly one TS2322 'object' for `\"a\" in x && \"b\" in x`; got: {d:?}"
    );
}

#[test]
fn three_chain_in_against_bare_type_param_emits_ts2322_once() {
    let d = diags(
        r#"
function test<T>(x: T) {
    return "a" in x && "b" in x && "c" in x;
}
"#,
    );
    assert_eq!(
        count_ts2322_object(&d),
        1,
        "Expected exactly one TS2322 'object' for chain of three `in`s; got: {d:?}"
    );
}

#[test]
fn solo_in_against_bare_type_param_still_emits_ts2322() {
    let d = diags(
        r#"
function test<T>(x: T) {
    return "a" in x;
}
"#,
    );
    assert_eq!(
        count_ts2322_object(&d),
        1,
        "Solo `\"a\" in x` should still emit one TS2322 against bare T; got: {d:?}"
    );
}

#[test]
fn constrained_type_param_in_chain_still_emits_no_extra() {
    // T extends object — the first `in` is valid, no TS2322 anywhere.
    let d = diags(
        r#"
function test<T extends object>(x: T) {
    return "a" in x && "b" in x;
}
"#,
    );
    assert_eq!(
        count_ts2322_object(&d),
        0,
        "T extends object → no TS2322; got: {d:?}"
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
    // Single TS2322 from the `in` operator itself; no cascade in the false branch.
    assert_eq!(
        count_ts2322_object(&d),
        1,
        "Expected one TS2322 from the `in` operator only; got: {d:?}"
    );
}
