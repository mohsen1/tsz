//! Object-literal present-property mismatch vs missing-property precedence.
//!
//! Structural rule: when a fresh object literal is assigned to an object target
//! and it has *both* a present property whose type is wrong *and* a required
//! property that is absent, `tsc`'s `elaborateObjectLiteral` reports the
//! present-property type mismatch(es) (TS2322, in source order) and the
//! missing-property diagnostic (TS2741/TS2739) is suppressed. The missing
//! property only surfaces when no present property mismatches — i.e. the
//! missing-property report is the *fallback* the elaboration falls through to,
//! not a pre-empting check.
//!
//! tsz previously bailed out of per-property elaboration the moment the source
//! object literal was missing any required property (except for generic mapped
//! receivers), so it reported TS2741 ("Property 'x' is missing") where `tsc`
//! reports the present-property TS2322. This file pins the corrected precedence
//! across the assignment (TS2322), call-argument, and return positions, and
//! keeps the missing-only floor (TS2741) intact.
//!
//! Binder spellings vary across cases so a fix keyed to a particular identifier
//! would not satisfy them; assertions are structural (diagnostic codes and the
//! property named by the "is missing" message), not dependent on type-printer
//! rendering.

use tsz_checker::test_utils::{check_source_diagnostics, diagnostic_count};
use tsz_common::diagnostics::Diagnostic;

fn diagnostics(source: &str) -> Vec<Diagnostic> {
    check_source_diagnostics(source)
}

/// True when any diagnostic — primary or elaboration line — claims `prop` is
/// "missing" (the TS2741/TS2739 family wording).
fn any_missing_message(diags: &[Diagnostic], prop: &str) -> bool {
    let needle = format!("Property '{prop}' is missing");
    diags.iter().any(|d| {
        d.message_text.starts_with(&needle)
            || d.related_information
                .iter()
                .any(|info| info.message_text.starts_with(&needle))
    })
}

#[test]
fn present_mismatch_wins_over_missing_in_assignment() {
    // `a` is present but wrong; `b` is absent. tsc reports the present mismatch
    // (TS2322 on `a`), not "Property 'b' is missing".
    let diags = diagnostics(
        r#"
interface Box { a: string; b: number; }
const x: Box = { a: 1 };
"#,
    );
    assert!(
        diags.iter().any(|d| d.code == 2322),
        "expected a present-property TS2322; got {diags:?}"
    );
    assert!(
        !any_missing_message(&diags, "b"),
        "missing-property report must be suppressed when a present property mismatches; got {diags:?}"
    );
}

#[test]
fn precedence_is_identifier_independent() {
    // Same shape, different binder spellings — proves the rule is structural.
    let diags = diagnostics(
        r#"
interface Settings { label: string; count: number; }
const cfg: Settings = { label: 42 };
"#,
    );
    assert!(
        diags.iter().any(|d| d.code == 2322),
        "expected a present-property TS2322 for renamed binders; got {diags:?}"
    );
    assert!(
        !any_missing_message(&diags, "count"),
        "renamed missing property must also be suppressed; got {diags:?}"
    );
}

#[test]
fn all_present_mismatches_reported_and_missing_suppressed() {
    // Two present-wrong properties plus one absent: tsc reports both present
    // mismatches (TS2322 x2) and omits the missing-property report entirely.
    let diags = diagnostics(
        r#"
interface Rec { a: string; b: number; c: boolean; }
const x: Rec = { a: 1, b: "z" };
"#,
    );
    assert!(
        diagnostic_count(&diags, 2322) >= 2,
        "expected both present-property mismatches; got {diags:?}"
    );
    assert!(
        !any_missing_message(&diags, "c"),
        "missing 'c' must be suppressed while present properties mismatch; got {diags:?}"
    );
}

#[test]
fn present_mismatch_wins_in_call_argument() {
    let diags = diagnostics(
        r#"
interface Param { a: string; b: number; }
declare function need(p: Param): void;
need({ a: 1 });
"#,
    );
    assert!(
        diags.iter().any(|d| d.code == 2322),
        "expected a present-property TS2322 in the call-argument position; got {diags:?}"
    );
    assert!(
        !any_missing_message(&diags, "b"),
        "missing 'b' must be suppressed in the call-argument position; got {diags:?}"
    );
}

#[test]
fn present_mismatch_wins_in_return_position() {
    let diags = diagnostics(
        r#"
interface Out { a: string; b: number; }
function make(): Out { return { a: 1 }; }
"#,
    );
    assert!(
        diags.iter().any(|d| d.code == 2322),
        "expected a present-property TS2322 in the return position; got {diags:?}"
    );
    assert!(
        !any_missing_message(&diags, "b"),
        "missing 'b' must be suppressed in the return position; got {diags:?}"
    );
}

#[test]
fn nested_present_mismatch_wins_over_outer_missing() {
    // The mismatch is one level deep (`a.x`) while `b` is absent at the top.
    let diags = diagnostics(
        r#"
interface Nested { a: { x: string }; b: number; }
const x: Nested = { a: { x: 1 } };
"#,
    );
    assert!(
        diags.iter().any(|d| d.code == 2322),
        "expected a nested present-property TS2322; got {diags:?}"
    );
    assert!(
        !any_missing_message(&diags, "b"),
        "outer missing 'b' must be suppressed when a nested property mismatches; got {diags:?}"
    );
}

#[test]
fn missing_only_still_reports_missing_floor() {
    // Negative/floor case: all present properties are correct, one is absent.
    // The missing-property diagnostic (TS2741) must still surface, and no
    // spurious present-property TS2322 may appear.
    let diags = diagnostics(
        r#"
interface Box { a: string; b: number; }
const x: Box = { a: "ok" };
"#,
    );
    assert!(
        any_missing_message(&diags, "b"),
        "an absent property with all-correct present properties must still report 'missing'; got {diags:?}"
    );
    assert!(
        diagnostic_count(&diags, 2322) == 0,
        "no spurious present-property mismatch may appear in the missing-only case; got {diags:?}"
    );
}

#[test]
fn fully_assignable_literal_reports_nothing() {
    // Floor: a correct object literal produces no diagnostic.
    let diags = diagnostics(
        r#"
interface Box { a: string; b: number; }
const x: Box = { a: "ok", b: 2 };
"#,
    );
    assert!(
        diags
            .iter()
            .all(|d| d.code != 2322 && d.code != 2741 && d.code != 2739),
        "a fully-assignable object literal must not error; got {diags:?}"
    );
}
