//! An optional-chain root's nullish stripping is `strictNullChecks`-only,
//! not unconditional.
//!
//! Structural rule: tsc's `getOptionalExpressionType` calls
//! `getNonNullableType` on the expression at the root of an optional chain
//! (`on` in `on?.foo`, `on` in `on?.()`), and `getNonNullableType` is
//!
//! ```text
//! function getNonNullableType(type) {
//!     return strictNullChecks ? getTypeWithFacts(type, TypeFacts.NEUndefinedOrNull) : type;
//! }
//! ```
//!
//! — the identity function without `strictNullChecks`. So only in strict mode
//! does `on?.foo` narrow its receiver to `never` (TS2339) and `on?.()` narrow
//! its callee to `never` (TS2349, "has no call signatures"). Without it, the
//! receiver's/callee's own (unstripped) type flows into the ordinary
//! non-chain nullish reporter — the same `checkNonNullTypeWithReporter` this
//! narrows on `type.flags` — and reports TS18047/18048/2721/2722 exactly as
//! the non-chain `on.foo` / `on()` would.
//!
//! tsz's two chain-root call sites
//! (`property_access_type::nullish_access::handle_possibly_null_or_undefined_access`
//! and `computation::call::inner`'s optional-call callee resolution) stripped
//! unconditionally, so both reported the strict-mode answer in every mode.
//!
//! Oracle: `tsc` 7.0.2, `--noEmit --strictNullChecks <bool> --pretty false`.
//! Every expectation below is pinned against a real run, in both modes.

use crate::test_utils::{
    check_source_non_strict_codes as non_strict, check_source_strict_codes as strict,
};

const TS18047: u32 = 18047; // '<x>' is possibly 'null'.
const TS18048: u32 = 18048; // '<x>' is possibly 'undefined'.
const TS2339: u32 = 2339; // Property '<x>' does not exist on type '<T>'.
const TS2349: u32 = 2349; // This expression is not callable.
const TS2721: u32 = 2721; // Cannot invoke an object which is possibly 'null'.
const TS2722: u32 = 2722; // Cannot invoke an object which is possibly 'undefined'.

fn count(codes: &[u32], code: u32) -> usize {
    codes.iter().filter(|&&c| c == code).count()
}

// -------------------------------------------------------------------------
// `null`-typed receiver/callee, property/element/call positions, both modes.
// Binders are varied so nothing keys on identifier text.
// -------------------------------------------------------------------------

#[test]
fn null_receiver_optional_property_access_reports_ts18047_without_strict_null_checks() {
    for binder in ["on", "probe", "receiver"] {
        let source = format!("declare const {binder}: null;\n{binder}?.foo;");
        let lax = non_strict(&source);
        assert_eq!(
            count(&lax, TS18047),
            1,
            "expected TS18047 for `{binder}?.foo` without strictNullChecks, got: {lax:?}"
        );
        assert_eq!(
            count(&lax, TS2339),
            0,
            "`{binder}?.foo` must not report TS2339 without strictNullChecks, got: {lax:?}"
        );
    }
}

#[test]
fn null_receiver_optional_string_literal_element_access_reports_ts18047_without_strict_null_checks()
{
    // Uses a string-literal key (`on?.["foo"]`): the chain-root short
    // circuit this test targets lives on the `literal_string` arm of
    // `get_type_of_element_access_with_request`. A *numeric*-literal key
    // (`on?.[0]`) does not reach that arm at all and reports nothing in
    // either mode — a separate, pre-existing false-negative, not part of
    // this gate's scope (see the NOTE this PR leaves on the board).
    let source = "declare const on: null;\non?.[\"foo\"];";
    let lax = non_strict(source);
    assert_eq!(
        count(&lax, TS18047),
        1,
        "expected TS18047 for `on?.[\"foo\"]` without strictNullChecks, got: {lax:?}"
    );
    assert_eq!(
        count(&lax, TS2339),
        0,
        "`on?.[\"foo\"]` must not report TS2339 without strictNullChecks, got: {lax:?}"
    );

    let strict_codes = strict(source);
    assert_eq!(
        count(&strict_codes, TS2339),
        1,
        "strict mode must keep TS2339 for `on?.[\"foo\"]`, got: {strict_codes:?}"
    );
}

#[test]
fn null_callee_optional_call_reports_ts2721_not_ts2349_without_strict_null_checks() {
    for binder in ["on", "probe", "callee"] {
        let source = format!("declare const {binder}: null;\n{binder}?.();");
        let lax = non_strict(&source);
        assert_eq!(
            count(&lax, TS2721),
            1,
            "expected TS2721 for `{binder}?.()` without strictNullChecks, got: {lax:?}"
        );
        assert_eq!(
            count(&lax, TS2349),
            0,
            "`{binder}?.()` must not report TS2349 without strictNullChecks, got: {lax:?}"
        );
    }
}

#[test]
fn undefined_receiver_and_callee_report_the_undefined_family_without_strict_null_checks() {
    let access = non_strict("declare const ou: undefined;\nou?.bar;");
    assert_eq!(
        count(&access, TS18048),
        1,
        "expected TS18048 for `ou?.bar` without strictNullChecks, got: {access:?}"
    );

    let call = non_strict("declare const ou: undefined;\nou?.();");
    assert_eq!(
        count(&call, TS2722),
        1,
        "expected TS2722 for `ou?.()` without strictNullChecks, got: {call:?}"
    );
    assert_eq!(
        count(&call, TS2349),
        0,
        "`ou?.()` must not report TS2349 without strictNullChecks, got: {call:?}"
    );
}

// -------------------------------------------------------------------------
// Strict mode is unchanged: `never`-on-chain-root stays exactly as before.
// -------------------------------------------------------------------------

#[test]
fn strict_mode_keeps_never_on_optional_chain_root() {
    let source = "declare const on: null;\non?.foo;\non?.();";
    let codes = strict(source);
    assert_eq!(
        count(&codes, TS2339),
        1,
        "strict mode must keep TS2339 on the `never`-narrowed receiver, got: {codes:?}"
    );
    assert_eq!(
        count(&codes, TS2349),
        1,
        "strict mode must keep TS2349 on the `never`-narrowed callee, got: {codes:?}"
    );
    assert_eq!(
        count(&codes, TS18047) + count(&codes, TS2721),
        0,
        "strict mode must not additionally report the nullish family here, got: {codes:?}"
    );
}

// -------------------------------------------------------------------------
// Nested chains: one report at the root, no cascading diagnostic on the
// continuation once the receiver/callee resolves to the ERROR sink.
// -------------------------------------------------------------------------

#[test]
fn nested_optional_chain_reports_once_at_the_root_without_strict_null_checks() {
    let property = non_strict("declare const nested: null;\nnested?.foo?.bar;");
    assert_eq!(
        count(&property, TS18047),
        1,
        "expected exactly one TS18047 for `nested?.foo?.bar`, got: {property:?}"
    );

    let call = non_strict("declare const callroot: undefined;\ncallroot?.()?.x;");
    assert_eq!(
        count(&call, TS2722),
        1,
        "expected exactly one TS2722 for `callroot?.()?.x`, got: {call:?}"
    );
}

// -------------------------------------------------------------------------
// Controls: a genuinely partial union (`T | null`) is unaffected by this
// gate in either mode — it never had a wholly-nullish root to begin with.
// -------------------------------------------------------------------------

#[test]
fn partial_union_optional_chain_stays_clean_in_both_modes() {
    let property_source = "declare const h: { a: number } | null;\nh?.a;";
    let lax = non_strict(property_source);
    assert!(
        lax.iter()
            .all(|c| ![TS18047, TS18048, TS2339, TS2721, TS2722, TS2349].contains(c)),
        "`(T | null)?.a` must stay clean without strictNullChecks, got: {lax:?}"
    );
    let strict_codes = strict(property_source);
    assert!(
        strict_codes
            .iter()
            .all(|c| ![TS18047, TS18048, TS2339, TS2721, TS2722, TS2349].contains(c)),
        "`(T | null)?.a` must stay clean under strictNullChecks, got: {strict_codes:?}"
    );

    let call_source = "declare const f: (() => void) | null;\nf?.();";
    let lax_call = non_strict(call_source);
    assert!(
        lax_call
            .iter()
            .all(|c| ![TS18047, TS2339, TS2721, TS2349].contains(c)),
        "`(() => void | null)?.()` must stay clean without strictNullChecks, got: {lax_call:?}"
    );
}
