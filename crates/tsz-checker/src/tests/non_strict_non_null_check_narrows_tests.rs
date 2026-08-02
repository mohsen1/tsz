//! Without `strictNullChecks`, tsc's non-null check is **narrowed, not
//! suppressed**.
//!
//! Structural rule: `checkNonNullTypeWithReporter` computes
//!
//! ```text
//! const kind = (strictNullChecks ? getFalsyFlags(type) : type.flags) & TypeFlags.Nullable;
//! if (kind) { reportError(node, kind); ... }
//! ```
//!
//! so turning `strictNullChecks` off swaps the operand's *falsy facts* for the
//! operand's *own flags*. An operand that IS `null`/`undefined` still reports the
//! whole family — TS18047/TS18048 through
//! `reportObjectPossiblyNullOrUndefinedError`, TS2721/TS2722 through
//! `reportCannotInvokePossiblyNullOrUndefinedError` — while a merely-nullable
//! union stops reporting because a union's own flags are `TypeFlags.Union`.
//! tsz gated the entire mirror on `strictNullChecks`, so every one of these rows
//! reported nothing, and a nullish callee reported the wrong diagnostic (TS2349
//! "not callable") instead of TS2721.
//!
//! Two exclusions are load bearing and both fall out of `type.flags`:
//!
//! - `void` is `TypeFlags.Void`, never `TypeFlags.Nullable`, so a `void` operand
//!   keeps falling through to the position's own structural check in both modes.
//!   `split_nullish_type` normalizes `void` to `undefined` for narrowing, so the
//!   non-null check has to exclude it explicitly.
//! - A union never triggers the non-strict arm, so `T | null` stays clean — which
//!   is what made the suppression look correct: without `strictNullChecks` almost
//!   everything either widens (`let z = null` → `any`) or is a union, and an
//!   explicit `null`/`undefined` annotation is the shape that survives.
//!
//! Oracle: `tsc` 7.0.2, `--noEmit --strict false --target es2015 --pretty false`.
//! Every expectation below is pinned against a real run, in both modes.

use crate::test_utils::{check_source_non_strict_codes as non_strict, check_source_strict_codes};

const TS18047: u32 = 18047; // '<x>' is possibly 'null'.
const TS18048: u32 = 18048; // '<x>' is possibly 'undefined'.
const TS2721: u32 = 2721; // Cannot invoke an object which is possibly 'null'.
const TS2722: u32 = 2722; // Cannot invoke an object which is possibly 'undefined'.
const TS2349: u32 = 2349; // This expression is not callable.

fn count(codes: &[u32], code: u32) -> usize {
    codes.iter().filter(|&&c| c == code).count()
}

/// The nullish diagnostics this check owns, in every position it reaches.
const NULLISH_FAMILY: [u32; 4] = [TS18047, TS18048, TS2721, TS2722];

fn nullish_codes(codes: &[u32]) -> Vec<u32> {
    codes
        .iter()
        .copied()
        .filter(|c| NULLISH_FAMILY.contains(c))
        .collect()
}

// -------------------------------------------------------------------------
// A `null`-typed operand: every position reports without strictNullChecks.
// Binders are varied so nothing keys on identifier text.
// -------------------------------------------------------------------------

#[test]
fn null_typed_property_access_reports_ts18047_without_strict_null_checks() {
    for binder in ["on", "probe", "receiver"] {
        let source = format!("declare const {binder}: null;\n{binder}.foo;");
        let codes = non_strict(&source);
        assert_eq!(
            count(&codes, TS18047),
            1,
            "expected TS18047 for a `null` receiver (binder {binder}), got: {codes:?}"
        );
    }
}

#[test]
fn null_typed_element_access_reports_ts18047_without_strict_null_checks() {
    for binder in ["on", "probe", "receiver"] {
        let source = format!("declare const {binder}: null;\n{binder}[0];");
        let codes = non_strict(&source);
        assert_eq!(
            count(&codes, TS18047),
            1,
            "expected TS18047 for a `null` element-access receiver (binder {binder}), got: {codes:?}"
        );
    }
}

#[test]
fn null_typed_callee_reports_ts2721_not_ts2349_without_strict_null_checks() {
    // The whole point of the call arm: tsz reported TS2349 "This expression is
    // not callable" here, which is a *wrong* diagnostic, not a missing one.
    for binder in ["on", "probe", "callee"] {
        let source = format!("declare const {binder}: null;\n{binder}();");
        let codes = non_strict(&source);
        assert_eq!(
            count(&codes, TS2721),
            1,
            "expected TS2721 for a `null` callee (binder {binder}), got: {codes:?}"
        );
        assert_eq!(
            count(&codes, TS2349),
            0,
            "a `null` callee must not report TS2349 (binder {binder}), got: {codes:?}"
        );
    }
}

#[test]
fn null_typed_in_operand_reports_ts18047_without_strict_null_checks() {
    let codes = non_strict("declare const on: null;\n\"\" in on;");
    assert_eq!(
        count(&codes, TS18047),
        1,
        "expected TS18047 for a `null` `in` RHS, got: {codes:?}"
    );
}

#[test]
fn null_typed_unary_operand_reports_ts18047_without_strict_null_checks() {
    let codes = non_strict("declare const on: null;\n~on;");
    assert_eq!(
        count(&codes, TS18047),
        1,
        "expected TS18047 for a `null` unary operand, got: {codes:?}"
    );
}

// -------------------------------------------------------------------------
// The `undefined` sibling: same rule, the other `TypeFlags.Nullable` member.
// -------------------------------------------------------------------------

#[test]
fn undefined_typed_operands_report_ts18048_and_ts2722_without_strict_null_checks() {
    let access = non_strict("declare const ou: undefined;\nou.bar;");
    assert_eq!(
        count(&access, TS18048),
        1,
        "expected TS18048 for an `undefined` receiver, got: {access:?}"
    );

    let element = non_strict("declare const ou: undefined;\nou[1];");
    assert_eq!(
        count(&element, TS18048),
        1,
        "expected TS18048 for an `undefined` element-access receiver, got: {element:?}"
    );

    let call = non_strict("declare const ou: undefined;\nou();");
    assert_eq!(
        count(&call, TS2722),
        1,
        "expected TS2722 for an `undefined` callee, got: {call:?}"
    );
    assert_eq!(
        count(&call, TS2349),
        0,
        "an `undefined` callee must not report TS2349, got: {call:?}"
    );
}

// -------------------------------------------------------------------------
// Alias, wrapper and nesting forms reach the same predicate.
// -------------------------------------------------------------------------

#[test]
fn aliased_null_type_reports_like_the_written_annotation() {
    let source =
        "type NullAlias = null;\ndeclare const viaAlias: NullAlias;\nviaAlias.member;\nviaAlias();";
    let codes = non_strict(source);
    assert_eq!(
        count(&codes, TS18047),
        1,
        "an aliased `null` annotation must report TS18047, got: {codes:?}"
    );
    assert_eq!(
        count(&codes, TS2721),
        1,
        "an aliased `null` callee must report TS2721, got: {codes:?}"
    );
}

#[test]
fn nested_null_property_reports_on_the_property_receiver() {
    let source = "declare const nested: { p: null };\nnested.p.q;\nnested.p();";
    let codes = non_strict(source);
    assert_eq!(
        count(&codes, TS18047),
        1,
        "a nested `null` property receiver must report TS18047, got: {codes:?}"
    );
    assert_eq!(
        count(&codes, TS2721),
        1,
        "a nested `null` property callee must report TS2721, got: {codes:?}"
    );
}

#[test]
fn static_undefined_member_reports_on_the_qualified_receiver() {
    let source = "class Holder { static m: undefined; }\nHolder.m.x;";
    let codes = non_strict(source);
    assert_eq!(
        count(&codes, TS18048),
        1,
        "a static `undefined` member receiver must report TS18048, got: {codes:?}"
    );
}

// -------------------------------------------------------------------------
// Controls. These are what made the blanket suppression look correct, and every
// one of them must stay clean — they are the false-positive surface of the fix.
// -------------------------------------------------------------------------

#[test]
fn widened_null_initializer_stays_clean_without_strict_null_checks() {
    // `let z = null` widens to `any` without strictNullChecks, so nothing here
    // carries a nullish flag at all.
    let codes = non_strict("let z = null;\nz.foo;\nz();\nlet w = undefined;\nw.foo;");
    assert!(
        nullish_codes(&codes).is_empty(),
        "widened null/undefined initializers must stay clean, got: {codes:?}"
    );
}

#[test]
fn nullable_union_annotation_stays_clean_without_strict_null_checks() {
    // A union's own flags are `TypeFlags.Union`, never `Nullable`, so the
    // non-strict arm does not trigger — this is the narrowing, and it is the
    // whole reason the fix is not a corpus-wide false-positive risk.
    let codes = non_strict("declare const un: { a: number } | null;\nun.a;\nun;");
    assert!(
        nullish_codes(&codes).is_empty(),
        "a `T | null` annotation must stay clean without strictNullChecks, got: {codes:?}"
    );
}

#[test]
fn void_operands_stay_clean_in_both_modes() {
    // `void` is `TypeFlags.Void`, not `TypeFlags.Nullable`. tsc reports the
    // position's own structural error (TS2339/TS7053/TS2349/TS2322) and never
    // the nullish family, under both settings — so this is the one arm of the
    // change that also corrects strict mode, where tsz reported TS18048 for
    // `v[0]` / `"" in v` and TS2722 for `v()`.
    let source = "declare const v: void;\nv.foo;\nv[0];\nv();\n\"\" in v;";

    let lax = non_strict(source);
    assert!(
        nullish_codes(&lax).is_empty(),
        "`void` operands must not report the nullish family without strictNullChecks, got: {lax:?}"
    );

    let strict = check_source_strict_codes(source);
    assert!(
        nullish_codes(&strict).is_empty(),
        "`void` operands must not report the nullish family under strict either, got: {strict:?}"
    );
}

#[test]
fn uninitialized_and_any_and_plain_operands_stay_clean_without_strict_null_checks() {
    let codes = non_strict(
        "let uninit;\nuninit.foo;\ndeclare const anyv: any;\nanyv.foo;\nanyv();\ndeclare const s: string;\ns.length;",
    );
    assert!(
        nullish_codes(&codes).is_empty(),
        "implicit-any, `any` and non-nullish operands must stay clean, got: {codes:?}"
    );
}

#[test]
fn optional_chain_on_a_null_receiver_is_unchanged_by_this_gate() {
    // Not a tsc-parity pin: `on?.foo` / `on?.()` on a `null` receiver is a
    // separate pre-existing divergence (tsz reports TS2339 on `never`, tsc
    // reports TS18047/TS2721) that this gate does not reach. Pinned as
    // "identical in both modes" so a later widening of the gate cannot quietly
    // start routing optional chains through it.
    let source = "declare const on: null;\non?.foo;\non?.();";
    assert_eq!(
        nullish_codes(&non_strict(source)),
        nullish_codes(&check_source_strict_codes(source)),
        "the optional-chain rows must not become mode-dependent"
    );
    assert!(
        nullish_codes(&non_strict(source)).is_empty(),
        "the optional-chain rows are not routed through this gate today"
    );
}

// -------------------------------------------------------------------------
// The strict arm is unchanged: it keeps using the falsy-facts trigger, so the
// union rows that the non-strict arm skips still report there.
// -------------------------------------------------------------------------

#[test]
fn strict_mode_keeps_reporting_the_union_and_widened_rows() {
    let widened = check_source_strict_codes("let z = null;\nz.foo;\nz();");
    assert_eq!(
        count(&widened, TS18047),
        1,
        "strict mode must keep TS18047 on a `null`-initialized binding, got: {widened:?}"
    );
    assert_eq!(
        count(&widened, TS2721),
        1,
        "strict mode must keep TS2721 on a `null`-initialized callee, got: {widened:?}"
    );

    let union = check_source_strict_codes("declare const un: { a: number } | null;\nun.a;");
    assert_eq!(
        count(&union, TS18047),
        1,
        "strict mode must keep TS18047 on a `T | null` receiver, got: {union:?}"
    );
}

#[test]
fn strict_mode_rows_are_unchanged_for_the_directly_nullish_operands() {
    let source = "declare const on: null;\non.foo;\non();\non[0];\n\"\" in on;\n~on;";
    let strict = check_source_strict_codes(source);
    assert_eq!(
        count(&strict, TS18047),
        4,
        "strict mode must keep four TS18047 rows, got: {strict:?}"
    );
    assert_eq!(
        count(&strict, TS2721),
        1,
        "strict mode must keep TS2721 for the callee, got: {strict:?}"
    );
}
