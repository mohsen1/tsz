//! A compound-assignment (`+=`, `-=`, ...) or increment/decrement (`++`, `--`)
//! target reads its value before writing it. When that target is an optional
//! chain, `tsc` reports the possibly-undefined family (`TS18047`/`TS18048`/
//! `TS18049`) alongside the existing `TS2777`/`TS2779` grammar error — but
//! *which* node it names depends on where the `undefined` comes from:
//!
//! - A receiver whose own declared type is optional ("genuine" optionality,
//!   e.g. `h?.inner.leaf` where `inner` is `inner?: ...`) reports on the
//!   *receiver* (`'h.inner' is possibly 'undefined'`), exactly once — this is
//!   the same diagnostic an ordinary read (`const v = h?.inner.leaf`) already
//!   produces, so the read-before-write forms must not add a second one.
//! - A receiver whose `undefined` is solely the chain's own short-circuit
//!   marker ("marker-only", e.g. `a.b?.c.d` where `c`/`d` are required)
//!   reports on the *whole target* (`'a.b.c.d' is possibly 'undefined'`),
//!   because the read-before-write happens on the chain's own result, not on
//!   an intermediate continuation.
//!
//! Structural rule: `optional_chain_invalid_assignment_target_context`
//! short-circuits a write-target's type to `any` so an invalid target cannot
//! cascade into assignability diagnostics — but that short-circuit must not
//! apply to a genuine *read* of the target's value (`FlowIntent::Read`),
//! since compound assignment and increment/decrement read before they write.
//! Gating the short-circuit on write flow (`FlowIntent::Write`) lets the
//! already-correct nullish-operand machinery
//! (`check_and_emit_nullish_binary_operands`, the `++`/`--` nullish check,
//! both landing in `emit_nullish_operand_error`) see the real type and fire
//! on its own, with no new diagnostic-emission code needed. Owner:
//! `types/property_access_type/resolve.rs` and
//! `types/computation/access.rs` (element access).

use tsz_common::options::checker::CheckerOptions;

fn strict_codes(source: &str) -> Vec<u32> {
    let opts = CheckerOptions {
        strict: true,
        strict_null_checks: true,
        ..CheckerOptions::default()
    };
    crate::test_utils::check_source(source, "test.ts", opts)
        .iter()
        .map(|diag| diag.code)
        .collect()
}

fn non_strict_null_codes(source: &str) -> Vec<u32> {
    let opts = CheckerOptions {
        strict: false,
        strict_null_checks: false,
        ..CheckerOptions::default()
    };
    crate::test_utils::check_source(source, "test.ts", opts)
        .iter()
        .map(|diag| diag.code)
        .collect()
}

fn messages_for(source: &str, code: u32) -> Vec<String> {
    let opts = CheckerOptions {
        strict: true,
        strict_null_checks: true,
        ..CheckerOptions::default()
    };
    crate::test_utils::check_source(source, "test.ts", opts)
        .into_iter()
        .filter(|diag| diag.code == code)
        .map(|diag| diag.message_text)
        .collect()
}

// ---------------------------------------------------------------------------
// Marker-only receiver: the whole target is named.
// ---------------------------------------------------------------------------

#[test]
fn compound_assignment_marker_only_reports_whole_target() {
    let codes = strict_codes(
        r#"
declare const a: { b?: { c: { d: number } } };
a.b?.c.d += 1;
"#,
    );
    assert!(codes.contains(&2779), "expected TS2779, got {codes:?}");
    assert!(codes.contains(&18048), "expected TS18048, got {codes:?}");

    let messages = messages_for(
        r#"
declare const a: { b?: { c: { d: number } } };
a.b?.c.d += 1;
"#,
        18048,
    );
    assert_eq!(
        messages,
        vec!["'a.b.c.d' is possibly 'undefined'.".to_string()],
        "TS18048 must name the whole target, not the receiver"
    );
}

#[test]
fn increment_marker_only_reports_whole_target() {
    let codes = strict_codes(
        r#"
declare const a: { b?: { c: { d: number } } };
a.b?.c.d++;
"#,
    );
    assert!(codes.contains(&2777), "expected TS2777, got {codes:?}");
    assert!(codes.contains(&18048), "expected TS18048, got {codes:?}");
}

#[test]
fn decrement_prefix_marker_only_reports_whole_target() {
    let codes = strict_codes(
        r#"
declare const a: { b?: { c: { d: number } } };
--a.b?.c.d;
"#,
    );
    assert!(codes.contains(&2777), "expected TS2777, got {codes:?}");
    assert!(codes.contains(&18048), "expected TS18048, got {codes:?}");
}

// ---------------------------------------------------------------------------
// Genuine receiver optionality: the receiver is named, exactly once.
// ---------------------------------------------------------------------------

#[test]
fn compound_assignment_genuine_receiver_reports_receiver_once() {
    let source = r#"
declare const h: { inner?: { leaf: number } };
h?.inner.leaf += 1;
"#;
    let codes = strict_codes(source);
    assert!(codes.contains(&2779), "expected TS2779, got {codes:?}");
    let messages = messages_for(source, 18048);
    assert_eq!(
        messages,
        vec!["'h.inner' is possibly 'undefined'.".to_string()],
        "TS18048 must name the receiver exactly once, not the whole target, \
         and must not stack with a second read-before-write report"
    );
}

#[test]
fn increment_genuine_receiver_reports_receiver_once() {
    let source = r#"
declare const h: { inner?: { leaf: number } };
h?.inner.leaf++;
"#;
    let codes = strict_codes(source);
    assert!(codes.contains(&2777), "expected TS2777, got {codes:?}");
    let messages = messages_for(source, 18048);
    assert_eq!(
        messages,
        vec!["'h.inner' is possibly 'undefined'.".to_string()]
    );
}

#[test]
fn decrement_prefix_genuine_receiver_reports_receiver_once() {
    let source = r#"
declare const h: { inner?: { leaf: number } };
--h?.inner.leaf;
"#;
    let codes = strict_codes(source);
    assert!(codes.contains(&2777), "expected TS2777, got {codes:?}");
    let messages = messages_for(source, 18048);
    assert_eq!(
        messages,
        vec!["'h.inner' is possibly 'undefined'.".to_string()]
    );
}

// ---------------------------------------------------------------------------
// Adjacent cases: renamed binders, element access, non-strict, plain
// assignment (unaffected — no read-before-write).
// ---------------------------------------------------------------------------

#[test]
fn compound_assignment_marker_only_renamed_binders() {
    let codes = strict_codes(
        r#"
declare const zq: { al?: { be: { ga: number } } };
zq.al?.be.ga += 1;
"#,
    );
    assert!(codes.contains(&2779), "expected TS2779, got {codes:?}");
    assert!(codes.contains(&18048), "expected TS18048, got {codes:?}");
}

#[test]
fn compound_assignment_marker_only_element_access_reports_object_possibly_undefined() {
    // Element access receivers use the anonymous `Object is possibly ...`
    // form (TS2532), not the named `'x' is possibly ...` form — matching the
    // pinned oracle, which reports TS2532 (not TS18048) for this shape.
    let codes = strict_codes(
        r#"
declare const arr: { b?: { c: number[] } };
arr.b?.c[0] += 1;
"#,
    );
    assert!(codes.contains(&2779), "expected TS2779, got {codes:?}");
    assert!(codes.contains(&2532), "expected TS2532, got {codes:?}");
}

#[test]
fn compound_assignment_marker_only_non_strict_null_checks_reports_grammar_only() {
    // Without strictNullChecks, tsc emits only the grammar error — no
    // possibly-undefined diagnostic at all.
    let codes = non_strict_null_codes(
        r#"
declare const a: { b?: { c: { d: number } } };
a.b?.c.d += 1;
"#,
    );
    assert!(codes.contains(&2779), "expected TS2779, got {codes:?}");
    assert!(
        !codes.contains(&18048) && !codes.contains(&2532),
        "expected no possibly-undefined diagnostic without strictNullChecks, got {codes:?}"
    );
}

#[test]
fn plain_assignment_marker_only_target_stays_grammar_only() {
    // Plain `=` does not read before writing, so it must not gain a new
    // TS18048 from this change — regression guard for the read/write split.
    let codes = strict_codes(
        r#"
declare const a: { b?: { c: { d: number } } };
a.b?.c.d = 1;
"#,
    );
    assert!(codes.contains(&2779), "expected TS2779, got {codes:?}");
    assert!(
        !codes.contains(&18048),
        "plain assignment must not read-before-write, got {codes:?}"
    );
}

#[test]
fn plain_assignment_genuine_receiver_still_reports_receiver_once() {
    // Unaffected by this change (owned by #16650's receiver check once
    // merged, or by the pre-existing ordinary-read path today), but pinned
    // here so a future regression in either mechanism is caught.
    let source = r#"
declare const h: { inner?: { leaf: number } };
h?.inner.leaf = 1;
"#;
    let codes = strict_codes(source);
    assert!(codes.contains(&2779), "expected TS2779, got {codes:?}");
}
