//! Regression tests for tsc parity in unions of callable types where one
//! member has multiple overloads.
//!
//! tsc's `getUnionSignatures` filters the multi-overload member's signatures
//! to those structurally matching the single-overload member's sig, exposing
//! only the matched signature(s) as the union's callable shape. Args that
//! fail the matched param shape must be rejected — even if the multi-overload
//! member has a *different* overload that would individually accept them.
//!
//! See `crates/tsz-solver/src/operations/core/call_resolution.rs` —
//! `resolve_union_call`'s `has_multi_overload_members == 1` arm.

use crate::test_utils::check_source_diagnostics;

/// `{ (a: number): number; } | { (a: number): string; (a: string): boolean; }`
/// has only `(a: number)` as the unified callable. Calling with `"hello"`
/// must emit TS2345 even though M2 has a `(a: string)` overload that would
/// individually accept the arg. Mirrors
/// `conformance/types/union/unionTypeCallSignatures.ts:27`.
#[test]
fn union_single_plus_multi_overload_rejects_via_unified_sig() {
    let diags = check_source_diagnostics(
        r#"
declare var f: { (a: number): number; } | { (a: number): string; (a: string): boolean; };
f("hello");
"#,
    );

    let ts2345: Vec<_> = diags.iter().filter(|d| d.code == 2345).collect();
    assert_eq!(
        ts2345.len(),
        1,
        "Expected one TS2345 (string not assignable to unified-sig param 'number'). Got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
    let msg = &ts2345[0].message_text;
    assert!(
        msg.contains("'string'") && msg.contains("'number'"),
        "Message should be 'string' not assignable to 'number', got {msg:?}"
    );
}

/// Companion lock: when the arg matches the unified sig, the call succeeds —
/// per-member return types still get unioned.
#[test]
fn union_single_plus_multi_overload_accepts_matching_arg() {
    let diags = check_source_diagnostics(
        r#"
declare var f: { (a: number): number; } | { (a: number): string; (a: string): boolean; };
const r = f(10);
"#,
    );

    // No TS2345 — arg type matches unified sig.
    let ts2345: Vec<_> = diags.iter().filter(|d| d.code == 2345).collect();
    assert!(
        ts2345.is_empty(),
        "TS2345 must not fire for an arg matching the unified sig. Got: {:?}",
        ts2345.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );

    let ts2349: Vec<_> = diags.iter().filter(|d| d.code == 2349).collect();
    assert!(
        ts2349.is_empty(),
        "TS2349 must not fire — the union IS callable via the matched pair. Got: {:?}",
        ts2349.iter().map(|d| &d.message_text).collect::<Vec<_>>()
    );
}

/// When the multi-overload member has NO sig matching the single-overload
/// member (M1=`(a: number)` vs M2=`(a: boolean)/(a: string)`), tsc 7.0.2's
/// `getUnionSignatures` pass 2 combines each of M2's overloads with M1's sig,
/// intersecting the params to `never` — so the union IS callable, both
/// combined candidates fail on the argument, and the failure reports as one
/// TS2769 with the last-overload TS2770 header and the `never`-param TS2345
/// (the literal `10` is preserved against `never`). Differential-verified
/// against the pinned tsc 7.0.2 binary.
#[test]
fn union_single_plus_multi_overload_no_match_reports_ts2769_last_overload() {
    let diags = check_source_diagnostics(
        r#"
declare var f: { (a: number): number; } | { (a: boolean): string; (a: string): boolean; };
f(10);
"#,
    );

    let ts2769: Vec<_> = diags.iter().filter(|d| d.code == 2769).collect();
    assert_eq!(
        ts2769.len(),
        1,
        "Expected one TS2769 for the union combined-signature failure. Got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
    let chain: Vec<(u8, u32, &str)> = ts2769[0]
        .related_information
        .iter()
        .map(|r| (r.depth, r.code, r.message_text.as_str()))
        .collect();
    assert_eq!(
        chain,
        vec![
            (0, 2770, "The last overload gave the following error."),
            (
                1,
                2345,
                "Argument of type '10' is not assignable to parameter of type 'never'."
            ),
        ],
        "expected the last-overload chain with the raw literal preserved against 'never'"
    );
    assert!(
        !diags.iter().any(|d| d.code == 2349),
        "the union IS callable through the combined signatures; TS2349 must not fire. Got: {:?}",
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}
