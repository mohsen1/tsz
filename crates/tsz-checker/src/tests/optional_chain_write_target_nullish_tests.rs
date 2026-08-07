//! A *pure-write* optional-chain target (`q?.w.e = 1`, a `for...of`/`for...in`
//! head) reads nothing, so the read-before-write machinery
//! (`optional_chain_read_before_write_tests`) never fires on it. But `tsc`
//! still reports the possibly-nullish family (`TS18047`/`TS18048`/`TS18049`)
//! on the target's *receiver* when that receiver carries GENUINE optionality,
//! alongside the grammar error (`TS2779` for `=`, `TS2781`/`TS2779` for the
//! `for` heads).
//!
//! The discriminator is *which* `undefined` the receiver carries, not the
//! write form:
//!
//! | target | receiver's `undefined` | tsc |
//! |---|---|---|
//! | `a.b?.c.d = 1` (`c` required) | chain marker only | grammar only |
//! | `q?.w.e = 1` (`w?` optional) | genuine | `TS18048 'q.w'` + grammar |
//! | `z.y?.x.v = 1` (`x?` optional) | marker **and** genuine | `TS18048 'z.y.x'` + grammar |
//!
//! `tsc` strips an optional chain's own short-circuit marker
//! (`removeOptionalTypeMarker`) before checking a continuation, so a receiver
//! that is undefined *only* because the chain may short-circuit reports
//! nothing — but real member optionality survives the strip and reports, in
//! every write form including plain `=`. This is the write-position half of
//! the family whose read-before-write half (`+=`/`++`/`--`) landed in #16671;
//! the two fire on different nodes (receiver vs. whole target) and never
//! stack. Owner: `report_write_target_chain_nullish_receiver` in
//! `types/property_access_type/nullish_access.rs`, wired at the write-target
//! short-circuit in `types/property_access_type/resolve.rs` (property access)
//! and `types/computation/access.rs` (element access). #16683.

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

fn strict_messages_for(source: &str, code: u32) -> Vec<String> {
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
// Genuine receiver optionality: the receiver is named, exactly once, in every
// write form.
// ---------------------------------------------------------------------------

#[test]
fn plain_assignment_genuine_receiver_reports_receiver() {
    let source = r#"
declare const q: { w?: { e: number } };
q?.w.e = 1;
"#;
    let codes = strict_codes(source);
    assert!(
        codes.contains(&2779),
        "expected TS2779 grammar error, got {codes:?}"
    );
    assert!(
        codes.contains(&18048),
        "expected TS18048 on the receiver, got {codes:?}"
    );
    let messages = strict_messages_for(source, 18048);
    assert_eq!(
        messages,
        vec!["'q.w' is possibly 'undefined'.".to_string()],
        "TS18048 must name the receiver (`q.w`), exactly once"
    );
}

#[test]
fn plain_assignment_both_marker_and_genuine_reports_receiver() {
    // `y?` makes the chain able to short-circuit (marker) AND `x?` is genuine
    // optionality: the marker strip is a no-op once the member carries its own
    // `undefined`, so the receiver still reports.
    let source = r#"
declare const z: { y?: { x?: { v: number } } };
z.y?.x.v = 1;
"#;
    let codes = strict_codes(source);
    assert!(codes.contains(&2779), "expected TS2779, got {codes:?}");
    let messages = strict_messages_for(source, 18048);
    assert_eq!(
        messages,
        vec!["'z.y.x' is possibly 'undefined'.".to_string()],
        "TS18048 must name the receiver (`z.y.x`), exactly once"
    );
}

#[test]
fn element_access_target_genuine_receiver_reports_receiver() {
    // The write target is element access `[0]`, but its receiver `idx.list` is
    // a nameable property access with genuine optionality (`list?`).
    let source = r#"
declare const idx: { list?: number[] };
idx?.list[0] = 1;
"#;
    let codes = strict_codes(source);
    assert!(codes.contains(&2779), "expected TS2779, got {codes:?}");
    let messages = strict_messages_for(source, 18048);
    assert_eq!(
        messages,
        vec!["'idx.list' is possibly 'undefined'.".to_string()],
        "TS18048 must name the element-access receiver (`idx.list`)"
    );
}

#[test]
fn for_of_head_genuine_receiver_reports_receiver() {
    let source = r#"
declare const q: { w?: { e: number } };
declare const items: number[];
for (q?.w.e of items);
"#;
    let codes = strict_codes(source);
    assert!(
        codes.contains(&2781),
        "expected TS2781 for...of grammar error, got {codes:?}"
    );
    let messages = strict_messages_for(source, 18048);
    assert_eq!(
        messages,
        vec!["'q.w' is possibly 'undefined'.".to_string()],
        "TS18048 must name the receiver in a for...of head"
    );
}

#[test]
fn for_in_head_genuine_receiver_reports_receiver() {
    let source = r#"
declare const q: { w?: { e: number } };
declare const obj: { [k: string]: unknown };
for (q?.w.e in obj);
"#;
    let messages = strict_messages_for(source, 18048);
    assert_eq!(
        messages,
        vec!["'q.w' is possibly 'undefined'.".to_string()],
        "TS18048 must name the receiver in a for...in head"
    );
}

// ---------------------------------------------------------------------------
// Marker-only receiver: no possibly-undefined report at all (the strip
// removes the chain-introduced `undefined`).
// ---------------------------------------------------------------------------

#[test]
fn plain_assignment_marker_only_receiver_stays_grammar_only() {
    // `c` is a required member, so `a.b?.c`'s `undefined` is purely the chain
    // short-circuit marker; the strip removes it and nothing is reported.
    let source = r#"
declare const a: { b?: { c: { d: number } } };
a.b?.c.d = 1;
"#;
    let codes = strict_codes(source);
    assert!(codes.contains(&2779), "expected TS2779, got {codes:?}");
    assert!(
        !codes.contains(&18048) && !codes.contains(&18047) && !codes.contains(&18049),
        "marker-only receiver must not report possibly-nullish, got {codes:?}"
    );
}

#[test]
fn element_access_marker_only_receiver_stays_grammar_only() {
    let source = r#"
declare const arr: { b?: { c: number[] } };
arr.b?.c[0] = 1;
"#;
    let codes = strict_codes(source);
    assert!(codes.contains(&2779), "expected TS2779, got {codes:?}");
    assert!(
        !codes.contains(&18048) && !codes.contains(&2532),
        "marker-only element receiver must not report possibly-nullish, got {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// null / null|undefined variants: the reporter names the right cause.
// ---------------------------------------------------------------------------

#[test]
fn plain_assignment_null_or_undefined_member_reports_null_or_undefined() {
    // `w?` contributes `undefined`; the member type also admits `null`, so the
    // receiver is `{ e } | null | undefined` → TS18049.
    let source = r#"
declare const q: { w?: { e: number } | null };
q?.w.e = 1;
"#;
    let messages = strict_messages_for(source, 18049);
    assert_eq!(
        messages,
        vec!["'q.w' is possibly 'null' or 'undefined'.".to_string()],
        "a null|undefined receiver must report the combined TS18049 form"
    );
}

#[test]
fn plain_assignment_pure_null_receiver_reports_null() {
    // `x` is a *required* member (no genuine `undefined`), so the only chain
    // `undefined` is the short-circuit marker from `z.y?`. The marker strip
    // removes it, but the member's declared `| null` survives → pure `null`
    // cause → TS18047. This is the discriminating third cause the family
    // advertises alongside 18048/18049.
    let source = r#"
declare const z: { y?: { x: { v: number } | null } };
z.y?.x.v = 1;
"#;
    let codes = strict_codes(source);
    assert!(codes.contains(&2779), "expected TS2779, got {codes:?}");
    let messages = strict_messages_for(source, 18047);
    assert_eq!(
        messages,
        vec!["'z.y.x' is possibly 'null'.".to_string()],
        "a pure-null receiver (chain marker stripped) must report TS18047"
    );
}

// ---------------------------------------------------------------------------
// Generic receiver constrained by a type parameter: the constraint's
// optionality drives the report (no reliance on a concrete object shape).
// ---------------------------------------------------------------------------

#[test]
fn plain_assignment_generic_constrained_receiver_reports() {
    let source = r#"
function f<T extends { w?: { e: number } }>(q: T) {
    q?.w.e = 1;
}
"#;
    let codes = strict_codes(source);
    assert!(codes.contains(&2779), "expected TS2779, got {codes:?}");
    assert!(
        codes.contains(&18048),
        "a generic constrained receiver with optional member must report TS18048, got {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// Binder-name invariance: the same structural shape under different identifier
// names produces the same diagnostics (anti-hardcoding gate).
// ---------------------------------------------------------------------------

#[test]
fn genuine_receiver_report_is_binder_name_invariant() {
    let source = r#"
declare const outer: { middle?: { inner: number } };
outer?.middle.inner = 1;
"#;
    let messages = strict_messages_for(source, 18048);
    assert_eq!(
        messages,
        vec!["'outer.middle' is possibly 'undefined'.".to_string()],
        "the report follows the structure, not the identifier spelling"
    );
}

// ---------------------------------------------------------------------------
// Negatives: guarded continuation, non-null assertion, non-strict mode.
// ---------------------------------------------------------------------------

#[test]
fn guarded_continuation_link_reports_nothing() {
    // The final link carries its own `?.`, so the continuation is guarded and
    // tsc reports no possibly-undefined on it.
    let source = r#"
declare const q: { w?: { e: number } };
q?.w?.e = 1;
"#;
    let codes = strict_codes(source);
    assert!(
        !codes.contains(&18048),
        "a `?.`-guarded continuation must not report TS18048, got {codes:?}"
    );
}

#[test]
fn non_null_asserted_receiver_reports_nothing() {
    // `!` removes the receiver's `undefined`, so there is nothing to report.
    let source = r#"
declare const q: { w?: { e: number } };
q?.w!.e = 1;
"#;
    let codes = strict_codes(source);
    assert!(
        !codes.contains(&18048),
        "a non-null-asserted receiver must not report TS18048, got {codes:?}"
    );
}

#[test]
fn non_strict_null_checks_reports_grammar_only() {
    // Without strictNullChecks the possibly-nullish family does not fire.
    let codes = non_strict_null_codes(
        r#"
declare const q: { w?: { e: number } };
q?.w.e = 1;
"#,
    );
    assert!(codes.contains(&2779), "expected TS2779, got {codes:?}");
    assert!(
        !codes.contains(&18048),
        "no possibly-undefined diagnostic without strictNullChecks, got {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// Overlap with the read-before-write path: a compound assignment reads before
// writing, so the write-target reporter must NOT stack a second diagnostic.
// The result is exactly one TS18048 naming the receiver.
// ---------------------------------------------------------------------------

#[test]
fn compound_assignment_overlap_reports_receiver_exactly_once() {
    let source = r#"
declare const q: { w?: { e: number } };
q?.w.e += 1;
"#;
    let messages = strict_messages_for(source, 18048);
    assert_eq!(
        messages,
        vec!["'q.w' is possibly 'undefined'.".to_string()],
        "the read-before-write and write-target reporters must not stack on `+=`"
    );
}
