//! TS1318 for an abstract accessor that carries an implementation body.
//!
//! Structural rule: when an abstract `get`/`set` accessor declares a body,
//! `tsc` reports **TS1318** (`An abstract accessor cannot have an
//! implementation.`), a distinct diagnostic from **TS1245** (`Method '{0}'
//! cannot have an implementation because it is marked abstract.`) used for the
//! sibling abstract *method* case. The two messages differ in shape — TS1318
//! takes no substitution, TS1245 interpolates the member name — so they are
//! not interchangeable renderings of one rule.
//!
//! `check_accessor_declaration_with_request`
//! (`crates/tsz-checker/src/state/state_checking_members/ambient_signature_checks.rs`)
//! already detected the shape and had the right message text, but reported it
//! under the TS1245 diagnostic *code* constant
//! (`METHOD_CANNOT_HAVE_AN_IMPLEMENTATION_BECAUSE_IT_IS_MARKED_ABSTRACT`)
//! instead of TS1318's own
//! (`AN_ABSTRACT_ACCESSOR_CANNOT_HAVE_AN_IMPLEMENTATION`) — so tsz emitted the
//! right text under the wrong code. Every expectation below was taken from the
//! vendored `tsc` 7.0.2 oracle (`--noEmit --strict --pretty false`), not from
//! tsz's own prior output.

use crate::test_utils::check_source_diagnostics;

fn codes(source: &str) -> Vec<u32> {
    check_source_diagnostics(source)
        .iter()
        .map(|d| d.code)
        .collect()
}

// ---------------------------------------------------------------------------
// Positives: an abstract accessor with a body reports TS1318, never TS1245.
// ---------------------------------------------------------------------------

#[test]
fn abstract_get_accessor_with_body_reports_ts1318_not_ts1245() {
    let got = codes(
        r#"
abstract class A {
  abstract get g(): number {
    return 1;
  }
}
"#,
    );
    assert!(got.contains(&1318), "expected TS1318, got {got:?}");
    assert!(!got.contains(&1245), "must not report TS1245, got {got:?}");
}

#[test]
fn abstract_set_accessor_with_body_reports_ts1318_not_ts1245() {
    let got = codes(
        r#"
abstract class A {
  abstract set s(v: number) {
    console.log(v);
  }
}
"#,
    );
    assert!(got.contains(&1318), "expected TS1318, got {got:?}");
    assert!(!got.contains(&1245), "must not report TS1245, got {got:?}");
}

/// Both accessors of an abstract pair carry their own body: each is an
/// independent TS1318, not deduplicated to one diagnostic.
#[test]
fn abstract_get_and_set_both_with_bodies_each_report_ts1318() {
    let got = codes(
        r#"
abstract class A {
  abstract get g(): number {
    return 1;
  }
  abstract set s(v: number) {
    console.log(v);
  }
}
"#,
    );
    assert_eq!(
        got.iter().filter(|&&c| c == 1318).count(),
        2,
        "expected two independent TS1318s, got {got:?}"
    );
}

/// Renamed-binder control: the rule is structural, so renaming every
/// identifier must not change the outcome.
#[test]
fn abstract_accessor_renamed_binders_reports_ts1318() {
    let got = codes(
        r#"
abstract class Shape {
  abstract get area(): number {
    return 1;
  }
}
"#,
    );
    assert!(got.contains(&1318), "expected TS1318, got {got:?}");
    assert!(!got.contains(&1245), "must not report TS1245, got {got:?}");
}

// ---------------------------------------------------------------------------
// Sibling control: an abstract *method* with a body still reports TS1245 —
// this fix must not blur the two codes together.
// ---------------------------------------------------------------------------

#[test]
fn abstract_method_with_body_still_reports_ts1245_not_ts1318() {
    let got = codes(
        r#"
abstract class A {
  abstract m(): number {
    return 1;
  }
}
"#,
    );
    assert!(got.contains(&1245), "expected TS1245, got {got:?}");
    assert!(!got.contains(&1318), "must not report TS1318, got {got:?}");
}

// ---------------------------------------------------------------------------
// Negative: a bodyless abstract accessor pair is clean — no TS1318, no TS1245.
// ---------------------------------------------------------------------------

#[test]
fn abstract_accessor_pair_without_bodies_reports_neither_code() {
    let got = codes(
        r#"
abstract class A {
  abstract get g(): number;
  abstract set s(v: number);
}
"#,
    );
    assert!(
        !got.iter().any(|c| matches!(c, 1318 | 1245)),
        "bodyless abstract accessors must not report either code, got {got:?}"
    );
}
