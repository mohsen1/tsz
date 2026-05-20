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

/// Negative lock: when the multi-overload member has NO sig matching the
/// single-overload member (e.g. M1=`(a: number)` vs M2=`(a: boolean)/(a: string)`),
/// the union is not callable at all (TS2349). This was already correct prior
/// to the fix; locked here so the unified-sig path doesn't regress it.
#[test]
fn union_single_plus_multi_overload_no_match_emits_ts2349() {
    let diags = check_source_diagnostics(
        r#"
declare var f: { (a: number): number; } | { (a: boolean): string; (a: string): boolean; };
f(10);
"#,
    );

    let ts2349: Vec<_> = diags.iter().filter(|d| d.code == 2349).collect();
    assert_eq!(
        ts2349.len(),
        1,
        "Expected TS2349 — no compatible sig pair across union members. Got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}
