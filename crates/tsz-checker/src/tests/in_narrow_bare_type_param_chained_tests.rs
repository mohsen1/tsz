//! Chained `in` checks against a bare type parameter must narrow after the
//! first check so the second `in` operand sees a valid `in`-RHS type.
//!
//! Without narrowing, every `"x" in t` (where `t: T`, no constraint) re-emits
//! an invalid-RHS diagnostic because `T` may represent a primitive. tsc only
//! emits that diagnostic once at the *first* `in` operand; the truthy branch
//! narrows `t` to `T & Record<"x", unknown>`, satisfying the `in`-RHS validity
//! check for every chained operand to its right.
//!
//! For type-parameter operands, tsc reports the diagnostic with code TS2322
//! ("Type 'T' is not assignable to type 'object'") rather than TS2638
//! ("may represent a primitive value"). TS2638 is reserved for concrete
//! primitive shapes; generic positions go through the standard
//! assignability gateway. These tests track the no-cascade narrowing
//! invariant by counting the assignability diagnostic instead.

use tsz_common::options::checker::CheckerOptions;

fn diags(source: &str) -> Vec<crate::diagnostics::Diagnostic> {
    let opts = CheckerOptions {
        strict: true,
        strict_null_checks: true,
        ..CheckerOptions::default()
    };
    crate::test_utils::check_source(source, "test.ts", opts)
}

fn count_in_rhs_object_assignability(diags: &[crate::diagnostics::Diagnostic]) -> usize {
    diags
        .iter()
        .filter(|d| d.code == 2322 && d.message_text.contains("assignable to type 'object'"))
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
        count_in_rhs_object_assignability(&d),
        1,
        "Expected exactly one TS2322 invalid-RHS diagnostic for `\"a\" in x && \"b\" in x`; got: {d:?}"
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
        count_in_rhs_object_assignability(&d),
        1,
        "Expected exactly one TS2322 invalid-RHS diagnostic for chain of three `in`s; got: {d:?}"
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
        count_in_rhs_object_assignability(&d),
        1,
        "Solo `\"a\" in x` should still emit one TS2322 against bare T; got: {d:?}"
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
        count_in_rhs_object_assignability(&d),
        0,
        "T extends object -> no TS2322; got: {d:?}"
    );
}

/// Issue #3769: `instanceof Object && "a" in x` on a generic value
/// must narrow strongly enough that the `in`-RHS doesn't trigger
/// TS2638 ("may represent a primitive value"). tsc accepts both shapes
/// in the issue's repro:
///
/// ```ts
/// function f<T extends {}>(x: T)   { return x instanceof Object && "a" in x; }
/// function g<T>(x: T & {})         { return x instanceof Object && "a" in x; }
/// ```
///
/// The fix: when narrowing by `instanceof Object`, if the source is a
/// type parameter (or an intersection containing one), produce
/// `source & TypeId::OBJECT` so the `in`-operator validity check
/// recognises the result as non-primitive.
#[test]
fn instanceof_object_narrowing_does_not_emit_ts2638_on_generic() {
    let libs = crate::test_utils::load_default_lib_files();
    let opts = tsz_common::options::checker::CheckerOptions {
        strict: true,
        strict_null_checks: true,
        ..tsz_common::options::checker::CheckerOptions::default()
    };
    let d = crate::test_utils::check_source_with_libs(
        r#"
function f<T extends {}>(x: T) {
    return x instanceof Object && "a" in x;
}

function g<T>(x: T & {}) {
    return x instanceof Object && "a" in x;
}
"#,
        "test.ts",
        opts,
        &libs,
    );
    let ts2638_count = d.iter().filter(|d| d.code == 2638).count();
    assert_eq!(
        ts2638_count, 0,
        "Expected no TS2638 after `instanceof Object` narrowing on a generic; got: {d:?}"
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
        count_in_rhs_object_assignability(&d),
        1,
        "Expected one TS2322 from the `in` operator only; got: {d:?}"
    );
}
