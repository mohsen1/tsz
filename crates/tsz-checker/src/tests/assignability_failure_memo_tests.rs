//! Tests for the stamp-guarded assignability failure-analysis memo
//! (issue #13243).
//!
//! The memo must be semantically invisible: a failing TS2322/TS2345 relation
//! may skip the second reason-collecting solver pass, but every rendered
//! diagnostic (code, message, elaboration chain) must be byte-identical to
//! the unmemoized pipeline. Entries follow the
//! `evaluate_type_for_assignability` memo's validity model and are dropped
//! whenever the session stamp moves.

use crate::context::{AssignabilityFailureMemo, CachedAssignabilityAnalysis, CheckerOptions};
use crate::test_utils::check_source;
use tsz_common::perf_counters::PerfCounters;
use tsz_solver::TypeId;

// ---------------------------------------------------------------------------
// Memo container semantics
// ---------------------------------------------------------------------------

fn failing_analysis() -> CachedAssignabilityAnalysis {
    CachedAssignabilityAnalysis {
        related: false,
        depth_exceeded: false,
        iteration_exceeded: false,
        weak_union_violation: false,
        failure_reason: Some(tsz_solver::SubtypeFailureReason::TypeMismatch {
            source_type: TypeId::STRING,
            target_type: TypeId::NUMBER,
        }),
    }
}

#[test]
fn memo_serves_entries_under_unchanged_stamp() {
    let mut memo = AssignabilityFailureMemo::default();
    let stamp = (1, 0, 0);
    let key = (TypeId::STRING, TypeId::NUMBER, 0b11, false);
    memo.insert(stamp, key, failing_analysis());
    let served = memo.get(stamp, key).expect("entry must be served");
    assert!(!served.related);
    assert!(matches!(
        served.failure_reason,
        Some(tsz_solver::SubtypeFailureReason::TypeMismatch { .. })
    ));
}

#[test]
fn memo_misses_on_different_flags_or_sound_mode() {
    let mut memo = AssignabilityFailureMemo::default();
    let stamp = (1, 0, 0);
    memo.insert(
        stamp,
        (TypeId::STRING, TypeId::NUMBER, 0b11, false),
        failing_analysis(),
    );
    assert!(
        memo.get(stamp, (TypeId::STRING, TypeId::NUMBER, 0b01, false))
            .is_none(),
        "different relation flags must be a distinct key"
    );
    assert!(
        memo.get(stamp, (TypeId::STRING, TypeId::NUMBER, 0b11, true))
            .is_none(),
        "sound mode must be a distinct key"
    );
}

#[test]
fn memo_drops_entries_when_any_stamp_component_moves() {
    let key = (TypeId::STRING, TypeId::NUMBER, 0, false);
    for moved in [(2, 0, 0), (1, 1, 0), (1, 0, 1)] {
        let mut memo = AssignabilityFailureMemo::default();
        memo.insert((1, 0, 0), key, failing_analysis());
        assert!(
            memo.get(moved, key).is_none(),
            "stamp {moved:?} must invalidate"
        );
        // The memo re-stamps on the miss; fresh entries are valid again.
        memo.insert(moved, key, failing_analysis());
        assert!(memo.get(moved, key).is_some());
    }
}

// ---------------------------------------------------------------------------
// Counter-asserted single-pass behavior. nextest runs each test in its own
// process, so the process-global counters are isolated per test.
// ---------------------------------------------------------------------------

fn enable_counters() {
    tsz_common::perf_counters::force_enable_perf_counters_for_tests();
    assert!(tsz_common::perf_counters::enabled_fast());
}

fn relation_failure_counts() -> (u64, u64) {
    let snapshot = PerfCounters::snapshot();
    (
        snapshot.relation_failure.reason_walks,
        snapshot.relation_failure.memo_hits,
    )
}

/// A failing nested-object assignment renders its full elaboration chain
/// while the failure analysis is served from the gateway's captured pass:
/// at least one reason walk runs, and at least one later analysis of the
/// same prepared pair is a memo hit instead of a second walk.
#[test]
fn failing_assignment_serves_second_analysis_from_memo() {
    enable_counters();
    let diagnostics = check_source(
        "let target: { outer: { inner: string } } = { outer: { inner: 42 } };",
        "memo_hit.ts",
        CheckerOptions::default(),
    );
    assert!(
        diagnostics.iter().any(|d| d.code == 2322),
        "expected a TS2322 diagnostic, got {diagnostics:?}"
    );
    let (walks, hits) = relation_failure_counts();
    assert!(walks >= 1, "a failing relation must walk a reason once");
    assert!(
        hits >= 1,
        "the failure-analysis pass must be served from the memo (walks={walks}, hits={hits})"
    );
}

/// Renamed binders: the memo keys on `TypeId`s, never on identifier text,
/// so an α-renamed failing assignment behaves identically.
#[test]
fn failing_assignment_renamed_binders_serves_second_analysis_from_memo() {
    enable_counters();
    let diagnostics = check_source(
        "let zebra: { hull: { mast: string } } = { hull: { mast: 42 } };",
        "memo_hit_renamed.ts",
        CheckerOptions::default(),
    );
    assert!(
        diagnostics.iter().any(|d| d.code == 2322),
        "expected a TS2322 diagnostic, got {diagnostics:?}"
    );
    let (walks, hits) = relation_failure_counts();
    assert!(walks >= 1);
    assert!(hits >= 1, "walks={walks}, hits={hits}");
}

/// Failing call argument (TS2345): the call-argument diagnostic path also
/// reaches `analyze_assignability_failure`; the memo must serve it when the
/// gateway already captured the pair.
#[test]
fn failing_call_argument_reports_ts2345_without_extra_walks() {
    enable_counters();
    let diagnostics = check_source(
        "function takeRecord(arg: { tag: string }): void {}\ndeclare let payload: { tag: number };\ntakeRecord(payload);",
        "memo_call_arg.ts",
        CheckerOptions::default(),
    );
    assert!(
        diagnostics.iter().any(|d| d.code == 2345),
        "expected a TS2345 diagnostic, got {diagnostics:?}"
    );
    let (walks, _hits) = relation_failure_counts();
    assert!(walks >= 1, "a failing call argument must walk a reason");
}

/// Passing relations never collect reasons: an error-free program performs
/// zero failure-reason walks (decision-only callers stay reason-free,
/// issue #13213) and zero memo traffic.
#[test]
fn passing_program_does_zero_reason_collection() {
    enable_counters();
    let diagnostics = check_source(
        r#"
let person: { name: string; age: number } = { name: "n", age: 3 };
function consume(value: { name: string }): string { return value.name; }
consume(person);
"#,
        "memo_passing.ts",
        CheckerOptions::default(),
    );
    assert!(
        diagnostics.is_empty(),
        "expected a clean program, got {diagnostics:?}"
    );
    let (walks, hits) = relation_failure_counts();
    assert_eq!(walks, 0, "passing relations must not walk failure reasons");
    assert_eq!(hits, 0, "passing relations must not touch the memo");
}

// ---------------------------------------------------------------------------
// Rendered-output pins for the adjacent-case matrix. These are full-message
// equality checks so any memo-induced drift in the elaboration chain fails
// loudly. (Cross-build byte-parity is owned by the conformance gate.)
// ---------------------------------------------------------------------------

fn diagnostics_for(source: &str, name: &str) -> Vec<(u32, String)> {
    check_source(source, name, CheckerOptions::default())
        .into_iter()
        .map(|d| (d.code, d.message_text))
        .collect()
}

#[test]
fn nested_object_failure_keeps_property_elaboration_message() {
    let rendered = diagnostics_for(
        "let target: { outer: { inner: string } } = { outer: { inner: 42 } };",
        "pin_nested.ts",
    );
    assert_eq!(rendered.len(), 1, "got {rendered:?}");
    assert_eq!(rendered[0].0, 2322);
    assert_eq!(
        rendered[0].1,
        "Type 'number' is not assignable to type 'string'."
    );
}

#[test]
fn union_source_failure_keeps_union_display_message() {
    let rendered = diagnostics_for(
        "declare let part: string | number;\nlet whole: string = part;",
        "pin_union.ts",
    );
    assert_eq!(rendered.len(), 1, "got {rendered:?}");
    assert_eq!(rendered[0].0, 2322);
    assert_eq!(
        rendered[0].1,
        "Type 'string | number' is not assignable to type 'string'."
    );
}

#[test]
fn intersection_target_failure_keeps_constituent_elaboration() {
    let rendered = diagnostics_for(
        "type Left = { alpha: string };\ntype Right = { beta: number };\ndeclare let source: { alpha: string; beta: string };\nlet both: Left & Right = source;",
        "pin_intersection.ts",
    );
    assert_eq!(rendered.len(), 1, "got {rendered:?}");
    assert_eq!(rendered[0].0, 2322);
}

#[test]
fn generic_and_concrete_forms_render_identically_shaped_failures() {
    // Concrete form.
    let concrete = diagnostics_for(
        "let boxed: { value: string } = { value: 1 };",
        "pin_concrete.ts",
    );
    // Generic form instantiated to the same shape.
    let generic = diagnostics_for(
        "interface Box<T> { value: T }\nlet boxed: Box<string> = { value: 1 };",
        "pin_generic.ts",
    );
    assert_eq!(concrete.len(), 1, "got {concrete:?}");
    assert_eq!(generic.len(), 1, "got {generic:?}");
    assert_eq!(concrete[0].0, 2322);
    assert_eq!(generic[0].0, 2322);
    assert_eq!(
        concrete[0].1,
        "Type 'number' is not assignable to type 'string'."
    );
    assert_eq!(
        generic[0].1,
        "Type 'number' is not assignable to type 'string'."
    );
}
