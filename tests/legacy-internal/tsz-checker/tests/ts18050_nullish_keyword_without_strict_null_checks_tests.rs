//! A literal `null`/`undefined` KEYWORD used where a non-nullable value is
//! required is reported as TS18050 "The value '<x>' cannot be used here." under
//! **both** `strictNullChecks` settings.
//!
//! Structural rule: when such an operand's type carries a nullish part, tsc runs
//! `checkNonNullType` regardless of `strictNullChecks`; the strictness policy
//! lives one level down, in `reportObjectPossiblyNullOrUndefinedError`, which
//! reports the syntactic keyword as TS18050 in both modes and the type-driven
//! TS18047/18048/2531 family only under `strictNullChecks`. tsz gated the whole
//! `checkNonNullType` mirror on `strictNullChecks`, so the strict arm was correct
//! and the non-strict arm reported nothing at all.
//!
//! Two sibling call sites had the same over-wide gate:
//! - `types/computation/binary_support.rs` `check_in_operand_non_null` — both
//!   `in` operands.
//! - `error_reporter/operator_errors.rs` `check_nullish_unary_operand` — the
//!   unary `+`/`-`/`~` operand. Unary `+`/`-` masked the gate with their own
//!   keyword pre-check, so only `~` was observably wrong.
//!
//! The gate is the keyword, not a nullable type: a *named* operand of type `null`
//! keeps the type-driven routing (TS18047 under strict, nothing without it), and a
//! parenthesized `(null)` is an unnamed expression, so it reports TS2531 under
//! strict and, like every other type-driven case, nothing without it. Pinned both
//! ways below so a future widening of this path cannot silently turn either into
//! TS18050.

use crate::test_utils::{check_source_non_strict_codes as non_strict, check_source_strict_codes};

const TS18050: u32 = 18050; // The value '<x>' cannot be used here.
const TS18047: u32 = 18047; // '<x>' is possibly 'null'.
const TS2531: u32 = 2531; // Object is possibly 'null'.
const TS2358: u32 = 2358; // The left-hand side of an 'instanceof' expression must be …

fn count(codes: &[u32], code: u32) -> usize {
    codes.iter().filter(|&&c| c == code).count()
}

// -------------------------------------------------------------------------
// The keyword operand: TS18050 in both modes, both positions, both spellings.
// -------------------------------------------------------------------------

#[test]
fn null_keyword_as_in_lhs_is_ts18050_without_strict_null_checks() {
    let codes = non_strict("null in {};");
    assert_eq!(
        count(&codes, TS18050),
        1,
        "expected one TS18050 for a `null` keyword LHS, got: {codes:?}"
    );
}

#[test]
fn null_keyword_as_in_rhs_is_ts18050_without_strict_null_checks() {
    let codes = non_strict("\"\" in null;");
    assert_eq!(
        count(&codes, TS18050),
        1,
        "expected one TS18050 for a `null` keyword RHS, got: {codes:?}"
    );
}

#[test]
fn undefined_keyword_as_in_lhs_is_ts18050_without_strict_null_checks() {
    let codes = non_strict("undefined in {};");
    assert_eq!(
        count(&codes, TS18050),
        1,
        "expected one TS18050 for an `undefined` LHS, got: {codes:?}"
    );
}

#[test]
fn undefined_keyword_as_in_rhs_is_ts18050_without_strict_null_checks() {
    let codes = non_strict("\"\" in undefined;");
    assert_eq!(
        count(&codes, TS18050),
        1,
        "expected one TS18050 for an `undefined` RHS, got: {codes:?}"
    );
}

#[test]
fn keyword_in_operands_stay_ts18050_under_strict_null_checks() {
    for source in [
        "null in {};",
        "\"\" in null;",
        "undefined in {};",
        "\"\" in undefined;",
    ] {
        let codes = check_source_strict_codes(source);
        assert_eq!(
            count(&codes, TS18050),
            1,
            "strict mode regressed for {source:?}, got: {codes:?}"
        );
    }
}

#[test]
fn a_wholly_nullish_keyword_operand_reports_only_ts18050() {
    // tsc's `checkNonNullType` returns the error type for a wholly nullish
    // operand, so the structural key/object check does not also fire. Both
    // positions report exactly one diagnostic.
    for source in ["null in {};", "\"\" in null;"] {
        let codes = non_strict(source);
        assert_eq!(
            codes,
            vec![TS18050],
            "expected TS18050 alone for {source:?}, got: {codes:?}"
        );
    }
}

// -------------------------------------------------------------------------
// Controls: a nullish TYPE is not a nullish keyword.
// -------------------------------------------------------------------------

#[test]
fn named_null_typed_operand_is_never_ts18050() {
    // Renamed binders: the routing is keyed on the node kind, not on any
    // particular identifier text.
    for binder in ["probe", "nullish", "value"] {
        let source = format!("declare const {binder}: null;\n\"\" in {binder};");

        let lax = non_strict(&source);
        assert_eq!(
            count(&lax, TS18050),
            0,
            "a named `null`-typed operand must not become TS18050 (binder {binder}), got: {lax:?}"
        );

        let strict = check_source_strict_codes(&source);
        assert_eq!(
            count(&strict, TS18050),
            0,
            "a named `null`-typed operand must not become TS18050 under strict (binder {binder}), got: {strict:?}"
        );
        assert_eq!(
            count(&strict, TS18047),
            1,
            "a named `null`-typed operand keeps TS18047 under strict (binder {binder}), got: {strict:?}"
        );
    }
}

#[test]
fn parenthesized_null_operand_is_not_ts18050() {
    // `(null)` is a parenthesized expression, not a `NullKeyword` node: tsc
    // reports it through the unnamed-expression arm (TS2531), never TS18050.
    for source in ["(null) in {};", "\"\" in (null);"] {
        let lax = non_strict(source);
        assert_eq!(
            count(&lax, TS18050),
            0,
            "parenthesized null must not become TS18050 for {source:?}, got: {lax:?}"
        );

        let strict = check_source_strict_codes(source);
        assert_eq!(
            count(&strict, TS18050),
            0,
            "parenthesized null must not become TS18050 under strict for {source:?}, got: {strict:?}"
        );
        assert_eq!(
            count(&strict, TS2531),
            1,
            "parenthesized null keeps TS2531 under strict for {source:?}, got: {strict:?}"
        );
    }
}

#[test]
fn instanceof_lhs_keeps_ts2358_and_never_becomes_ts18050() {
    // The `instanceof` LHS goes through its own rule; widening the `in`
    // non-null check must not reach it.
    let source = "null instanceof (() => { });";

    let lax = non_strict(source);
    assert_eq!(
        count(&lax, TS2358),
        1,
        "expected TS2358 for an `instanceof` null LHS, got: {lax:?}"
    );
    assert_eq!(
        count(&lax, TS18050),
        0,
        "an `instanceof` null LHS must not become TS18050, got: {lax:?}"
    );

    let strict = check_source_strict_codes(source);
    assert_eq!(
        count(&strict, TS2358),
        1,
        "expected TS2358 for an `instanceof` null LHS under strict, got: {strict:?}"
    );
    assert_eq!(
        count(&strict, TS18050),
        0,
        "an `instanceof` null LHS must not become TS18050 under strict, got: {strict:?}"
    );
}

#[test]
fn non_nullish_in_operands_stay_clean_without_strict_null_checks() {
    // The fallback direction: widening the non-null check must not start
    // reporting on operands that carry no nullish part at all.
    let codes = non_strict("declare const o: { a: number };\n\"a\" in o;");
    assert!(
        codes.is_empty(),
        "a well-formed `in` must stay clean without strictNullChecks, got: {codes:?}"
    );
}

// -------------------------------------------------------------------------
// The unary `+`/`-`/`~` sibling arm.
// -------------------------------------------------------------------------

#[test]
fn unary_operators_report_ts18050_on_a_keyword_operand_without_strict_null_checks() {
    // `+` and `-` already had their own keyword pre-check, so `~` was the only
    // observably wrong arm; all three are pinned so the three stay in step.
    for source in [
        "~null;",
        "~undefined;",
        "-null;",
        "-undefined;",
        "+null;",
        "+undefined;",
    ] {
        let codes = non_strict(source);
        assert_eq!(
            count(&codes, TS18050),
            1,
            "expected one TS18050 for {source:?} without strictNullChecks, got: {codes:?}"
        );
    }
}

#[test]
fn unary_keyword_operands_stay_ts18050_under_strict_null_checks() {
    for source in [
        "~null;",
        "~undefined;",
        "-null;",
        "-undefined;",
        "+null;",
        "+undefined;",
    ] {
        let codes = check_source_strict_codes(source);
        assert_eq!(
            count(&codes, TS18050),
            1,
            "strict mode regressed for {source:?}, got: {codes:?}"
        );
    }
}

#[test]
fn unary_named_null_typed_operand_is_never_ts18050() {
    for binder in ["probe", "nullish", "value"] {
        let source = format!("declare const {binder}: null;\n~{binder};");

        let lax = non_strict(&source);
        assert_eq!(
            count(&lax, TS18050),
            0,
            "a named `null`-typed unary operand must not become TS18050 (binder {binder}), got: {lax:?}"
        );

        let strict = check_source_strict_codes(&source);
        assert_eq!(
            count(&strict, TS18050),
            0,
            "a named `null`-typed unary operand must not become TS18050 under strict (binder {binder}), got: {strict:?}"
        );
        assert_eq!(
            count(&strict, TS18047),
            1,
            "a named `null`-typed unary operand keeps TS18047 under strict (binder {binder}), got: {strict:?}"
        );
    }
}

#[test]
fn unary_parenthesized_null_operand_is_not_ts18050() {
    let source = "~(null);";

    let lax = non_strict(source);
    assert_eq!(
        count(&lax, TS18050),
        0,
        "parenthesized null must not become TS18050 for a unary operand, got: {lax:?}"
    );

    let strict = check_source_strict_codes(source);
    assert_eq!(
        count(&strict, TS18050),
        0,
        "parenthesized null must not become TS18050 under strict, got: {strict:?}"
    );
    assert_eq!(
        count(&strict, TS2531),
        1,
        "parenthesized null keeps TS2531 under strict, got: {strict:?}"
    );
}

#[test]
fn unary_void_and_non_nullish_operands_stay_clean_without_strict_null_checks() {
    // `void` is not in tsc's `Nullable` flag set, and a non-nullish operand has
    // nothing to report — neither may be pulled in by the widened gate.
    let codes = non_strict("declare const v: void;\ndeclare const n: number;\n~v;\n~n;");
    assert!(
        codes.is_empty(),
        "void/number unary operands must stay clean without strictNullChecks, got: {codes:?}"
    );
}
