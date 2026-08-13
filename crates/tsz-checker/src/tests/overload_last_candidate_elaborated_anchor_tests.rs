//! Regression tests for the TS2769 anchor when the last argument-error
//! candidate's failing argument is an array or object literal.
//!
//! Structural rule: when overload resolution fails and the reporter falls back
//! to the last argument-error candidate (`last_overload_failure_anchor`), tsc
//! (pinned 7.0.2) anchors the top-level TS2769 at the *elaborated failing
//! element* of that candidate's argument — the element the candidate's
//! expected parameter type rejects — not at the literal's first leaf.
//! Conformance witnesses: `conformance/es6/for-ofStatements/for-of39.ts`
//! (`new Map([["", true], ["", 0]])` anchors at `true`) and
//! `conformance/es6/destructuring/iterableArrayPattern28.ts`
//! (`new Map([["", 0], ["hello", true]])` anchors at `true` in the *second*
//! entry).
//!
//! The mixed arity + argument-failure overload sets below route the reporter
//! through `DiagnosticAnchorKind::OverloadPrimary` and the last-candidate
//! anchor, which is the path under test.
//!
//! See `crates/tsz-checker/src/error_reporter/call_errors_anchors.rs` —
//! `last_overload_failure_anchor`.

use crate::test_utils::check_source_diagnostics;

fn only_ts2769(source: &str) -> crate::diagnostics::Diagnostic {
    let diags = check_source_diagnostics(source);
    let ts2769: Vec<_> = diags.iter().filter(|d| d.code == 2769).collect();
    assert_eq!(
        ts2769.len(),
        1,
        "expected exactly one TS2769; got {:?}",
        diags
            .iter()
            .map(|d| (d.code, d.start, &d.message_text))
            .collect::<Vec<_>>()
    );
    ts2769[0].clone()
}

/// Failing element in the FIRST entry's second slot (`for-of39.ts` shape):
/// the anchor is the failing `true`, not the first leaf `""` and not the
/// whole array argument.
#[test]
fn anchors_at_failing_element_in_first_entry() {
    let source = r#"
interface PairStoreCtor {
    new (): object;
    new (entries?: readonly (readonly [string, number])[] | null): object;
    new (entries: readonly (readonly [string, number])[], hint?: string): object;
}
declare var PairStore: PairStoreCtor;
new PairStore([["", true], ["", 0]]);
"#;
    let diag = only_ts2769(source);
    let expected = source.find("true").expect("`true` present") as u32;
    assert_eq!(
        (diag.start, diag.length),
        (expected, 4),
        "TS2769 must anchor at the failing element `true`"
    );
}

/// Failing element in the SECOND entry (`iterableArrayPattern28.ts` shape):
/// first-leaf drilling would stop at the first entry's `""`; the elaborated
/// anchor lands on `true` inside the second entry.
#[test]
fn anchors_at_failing_element_in_second_entry() {
    let source = r#"
interface RegistryCtor {
    new (): object;
    new (rows?: readonly (readonly [string, number])[] | null): object;
    new (rows: readonly (readonly [string, number])[], tag?: string): object;
}
declare var Registry: RegistryCtor;
new Registry([["", 0], ["hello", true]]);
"#;
    let diag = only_ts2769(source);
    let expected = source.find("true").expect("`true` present") as u32;
    assert_eq!(
        (diag.start, diag.length),
        (expected, 4),
        "TS2769 must anchor at the failing element in the second entry"
    );
}

/// A failing first leaf still anchors at that leaf: the elaborated descent
/// agrees with the historical first-leaf drilling when the first element is
/// the culprit (the `new WeakMap([[s, false]])` shape).
#[test]
fn failing_first_leaf_keeps_its_anchor() {
    let source = r#"
interface BucketCtor {
    new (): object;
    new (entries?: readonly (readonly [number, boolean])[] | null): object;
    new (entries: readonly (readonly [number, boolean])[], label?: string): object;
}
declare var Bucket: BucketCtor;
declare var key: string;
new Bucket([[key, false]]);
"#;
    let diag = only_ts2769(source);
    let expected = source.rfind("key").expect("`key` present") as u32;
    assert_eq!(
        (diag.start, diag.length),
        (expected, 3),
        "TS2769 must anchor at the failing first element `key`"
    );
}

/// An object literal nested in the array drills to the offending property
/// name, mirroring tsc's object-literal elaboration.
#[test]
fn drills_into_object_literal_element() {
    let source = r#"
interface OptionsCtor {
    new (): object;
    new (opts?: readonly { retries: number }[] | null): object;
    new (opts: readonly { retries: number }[], tag?: string): object;
}
declare var Options: OptionsCtor;
new Options([{ retries: false }]);
"#;
    let diag = only_ts2769(source);
    let expected = source.rfind("retries").expect("`retries` present") as u32;
    assert_eq!(
        (diag.start, diag.length),
        (expected, 7),
        "TS2769 must anchor at the mismatched property name"
    );
}

/// Fallback lock: a non-literal argument has no elaborated element, so the
/// anchor stays on the argument itself.
#[test]
fn non_literal_argument_keeps_argument_anchor() {
    let source = r#"
interface FeedCtor {
    new (): object;
    new (entries?: readonly (readonly [string, number])[] | null): object;
    new (entries: readonly (readonly [string, number])[], tag?: string): object;
}
declare var Feed: FeedCtor;
declare var rows: boolean;
new Feed(rows);
"#;
    let diag = only_ts2769(source);
    let expected = source.rfind("rows").expect("`rows` argument present") as u32;
    assert_eq!(
        (diag.start, diag.length),
        (expected, 4),
        "TS2769 must anchor at the argument identifier"
    );
}

/// Generic `V` inference (issue #17364, `for-of39.ts`): when a type
/// parameter's candidates come from array-literal tuple elements and are
/// incompatible bare primitives, tsc does not keep the source-order-leftmost
/// candidate — it orders candidates by TS7 `TypeFlags` rank (`number` = 64
/// beats `boolean` = 256) before the `reduceLeft` leftmost-wins fallback
/// runs. `V` infers to `number`, so `true` (not `0`) is the rejected element
/// and the anchor lands there, even though `true` is the first entry.
#[test]
fn generic_v_inference_prefers_lower_ts7_rank_over_source_order() {
    let source = r#"
interface PairStoreCtor {
    new (): object;
    new <V>(entries?: readonly (readonly [string, V])[] | null): object;
    new <V>(entries: readonly (readonly [string, V])[], hint?: string): object;
}
declare var PairStore: PairStoreCtor;
new PairStore([["", true], ["", 0]]);
"#;
    let diag = only_ts2769(source);
    let expected = source.find("true").expect("`true` present") as u32;
    assert_eq!(
        (diag.start, diag.length),
        (expected, 4),
        "V must infer to number (lower TS7 rank); boolean is the rejected element"
    );
}

/// Same family, source order swapped: the winning candidate (`number`) must
/// stay the same regardless of which literal appears first in the array —
/// tsc's TypeFlags-rank ordering is not source-order dependent.
#[test]
fn generic_v_inference_rank_is_source_order_independent() {
    let source = r#"
interface PairStoreCtor {
    new (): object;
    new <V>(entries?: readonly (readonly [string, V])[] | null): object;
    new <V>(entries: readonly (readonly [string, V])[], hint?: string): object;
}
declare var PairStore: PairStoreCtor;
new PairStore([["", 0], ["", true]]);
"#;
    let diag = only_ts2769(source);
    let expected = source.rfind("true").expect("`true` present") as u32;
    assert_eq!(
        (diag.start, diag.length),
        (expected, 4),
        "swapping source order must not change the winning candidate (V = number)"
    );
}
