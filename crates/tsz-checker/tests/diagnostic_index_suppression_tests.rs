//! Tests for the auxiliary diagnostic index structures that make suppression
//! checks O(log n)/O(k) instead of O(n).
//!
//! Structural rule: when a TS2353/TS2561 diagnostic exists at a position inside
//! an outer TS2322 span, the outer TS2322 is suppressed. When a TS2322 with the
//! same message already covers an overlapping span, the new TS2322 is suppressed.
//! These invariants must hold even with many diagnostics and after speculation
//! rollbacks.

use tsz_checker::context::CheckerOptions;
use tsz_checker::diagnostics::Diagnostic;

fn check(source: &str) -> Vec<Diagnostic> {
    tsz_checker::test_utils::check_source(source, "test.ts", CheckerOptions::default())
}

/// tsc suppresses the enclosing TS2322 when a more specific TS2353 (excess
/// property) fires inside the same object literal span.
/// Structural rule: TS2322 at span [s, e) is suppressed when any TS2353/TS2561
/// has its start position in [s, e).
#[test]
fn ts2322_suppressed_by_inner_ts2353() {
    let diags = check(
        r#"
        type T = { a: number };
        const x: T = { a: 1, b: 2 };
        "#,
    );
    let has_ts2353 = diags.iter().any(|d| d.code == 2353 || d.code == 2561);
    let has_ts2322 = diags.iter().any(|d| d.code == 2322);
    assert!(
        has_ts2353,
        "Expected TS2353 for excess property 'b'; got: {diags:?}"
    );
    // When TS2353 fires for a property inside the literal, the enclosing TS2322
    // should be suppressed to avoid double-reporting the same error site.
    assert!(
        !has_ts2322,
        "TS2322 should be suppressed by inner TS2353; got: {diags:?}"
    );
}

/// Many excess-property errors (different properties on the same literal) should
/// not produce a TS2322 on the outer literal either, even when there are multiple
/// TS2353 entries in the index.
#[test]
fn ts2322_suppressed_by_multiple_ts2353() {
    let diags = check(
        r#"
        type T = { a: number };
        const x: T = { a: 1, b: 2, c: 3, d: 4 };
        "#,
    );
    let ts2353_count = diags
        .iter()
        .filter(|d| d.code == 2353 || d.code == 2561)
        .count();
    assert!(
        ts2353_count >= 1,
        "Expected at least one TS2353; got: {diags:?}"
    );
    let has_ts2322 = diags.iter().any(|d| d.code == 2322);
    assert!(
        !has_ts2322,
        "TS2322 should be suppressed by any inner TS2353; got: {diags:?}"
    );
}

/// A TS2322 on a non-overlapping span must still be emitted when a TS2353 exists
/// elsewhere in the file (the index should not suppress unrelated TS2322s).
#[test]
fn ts2322_not_suppressed_by_unrelated_ts2353() {
    let diags = check(
        r#"
        type A = { x: number };
        const _a: A = { x: 1, extra: 2 };  // TS2353 here
        const _b: string = 42;              // TS2322 here (different span)
        "#,
    );
    let has_ts2353 = diags.iter().any(|d| d.code == 2353 || d.code == 2561);
    let has_ts2322 = diags.iter().any(|d| d.code == 2322);
    assert!(
        has_ts2353,
        "Expected TS2353 for excess property; got: {diags:?}"
    );
    assert!(
        has_ts2322,
        "TS2322 for number→string should NOT be suppressed by an unrelated TS2353; got: {diags:?}"
    );
}

/// Correct type assignment must not produce any diagnostic.
#[test]
fn correct_assignment_no_diagnostics() {
    let diags = check(
        r#"
        const x: number = 42;
        const y: string = "hello";
        "#,
    );
    assert!(
        diags.is_empty(),
        "Expected no diagnostics for correct assignments, got: {diags:?}"
    );
}

/// Many TS2322 diagnostics in one file should not cause duplicates.
/// Each unique (span, message) combination must appear at most once.
#[test]
fn many_ts2322_no_duplicates() {
    let diags = check(
        r#"
        const a: string = 1;
        const b: string = 2;
        const c: string = 3;
        const d: string = 4;
        const e: string = 5;
        "#,
    );
    let ts2322s: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    // Expect exactly 5, one per assignment.
    assert_eq!(
        ts2322s.len(),
        5,
        "Expected 5 TS2322 errors (one per assignment), got: {ts2322s:?}"
    );
    // No two should share both start position and message.
    for i in 0..ts2322s.len() {
        for j in (i + 1)..ts2322s.len() {
            assert!(
                ts2322s[i].start != ts2322s[j].start
                    || ts2322s[i].message_text != ts2322s[j].message_text,
                "Duplicate TS2322 at same position+message: {:?} vs {:?}",
                ts2322s[i],
                ts2322s[j]
            );
        }
    }
}

/// Speculation rollback must not leave stale entries in the auxiliary indices.
/// After a failed speculative path, the TS2322 suppression index must reflect
/// only the committed diagnostics.
#[test]
fn speculation_rollback_clears_ts2322_index() {
    // Two separate assignments with the same type mismatch — each should
    // produce exactly one TS2322. If the index is stale after rollback, the
    // second might be wrongly suppressed.
    let diags = check(
        r#"
        declare function f(x: string): void;
        declare function f(x: number): void;
        const a: string = 1;
        const b: string = 2;
        f(a);
        f(b);
        "#,
    );
    let ts2322s: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        ts2322s.len(),
        2,
        "Expected 2 TS2322 errors after speculation rollback, got: {ts2322s:?}"
    );
}

/// TS2301 at a position should suppress TS2304 at the same position.
/// The fix uses an O(1) `HashSet` lookup instead of a linear scan.
#[test]
fn ts2301_suppresses_ts2304_at_same_position() {
    // TS2301: initializer of instance member cannot reference identifier
    // declared in the constructor parameter. This should suppress TS2304.
    // Use a class where the constructor parameter name shadows the class member.
    let diags = check(
        r#"
        class C {
            x = this.#priv;
            #priv: number;
            constructor(private priv: number) {
                this.x = priv;
            }
        }
        "#,
    );
    // There should be no duplicate TS2304 on top of a TS2301 at the same position.
    let pos_with_2301: std::collections::HashSet<u32> = diags
        .iter()
        .filter(|d| d.code == 2301)
        .map(|d| d.start)
        .collect();
    for diag in &diags {
        if diag.code == 2304 {
            assert!(
                !pos_with_2301.contains(&diag.start),
                "TS2304 should be suppressed by TS2301 at the same position; got: {diag:?}"
            );
        }
    }
}

/// Verifies that the TS2353/2561 `BTreeSet` index is rebuilt correctly after a
/// speculative rollback. After rollback, the index should only contain positions
/// from committed diagnostics.
#[test]
fn excess_property_check_after_overload_speculation() {
    // The overload resolution for `g(...)` triggers speculation. After it, the
    // excess-property TS2353 from the object literal in `c` must still fire.
    let diags = check(
        r#"
        declare function g(x: number): void;
        declare function g(x: string): void;
        type T = { a: number };
        const c: T = { a: 1, extra: true };
        g(1);
        "#,
    );
    let has_ts2353 = diags.iter().any(|d| d.code == 2353 || d.code == 2561);
    assert!(
        has_ts2353,
        "Expected TS2353 for excess property after overload speculation; got: {diags:?}"
    );
    // No spurious TS2322 wrapping the excess-property error.
    let has_ts2322 = diags.iter().any(|d| d.code == 2322);
    assert!(
        !has_ts2322,
        "TS2322 should be suppressed by inner TS2353 even after overload speculation; got: {diags:?}"
    );
}

/// Variant: renamed type parameter must not change the suppression behavior.
/// (Guards against a hardcoded-name anti-pattern in the implementation.)
#[test]
fn ts2322_suppression_independent_of_type_parameter_names() {
    // Same logical test as ts2322_suppressed_by_inner_ts2353 but with
    // a different type alias name — verifies the fix is structural, not name-based.
    let diags_original = check(
        r#"
        type Target = { val: number };
        const _x: Target = { val: 1, extra: "surplus" };
        "#,
    );
    let diags_renamed = check(
        r#"
        type Renamed = { val: number };
        const _y: Renamed = { val: 1, extra: "surplus" };
        "#,
    );
    // Both must show TS2353 and no TS2322.
    for (label, diags) in [("original", &diags_original), ("renamed", &diags_renamed)] {
        let has_ts2353 = diags.iter().any(|d| d.code == 2353 || d.code == 2561);
        let has_ts2322 = diags.iter().any(|d| d.code == 2322);
        assert!(has_ts2353, "{label}: expected TS2353; got: {diags:?}");
        assert!(
            !has_ts2322,
            "{label}: TS2322 should be suppressed by inner TS2353; got: {diags:?}"
        );
    }
}
