//! TS2300 "Duplicate identifier" is reported at EVERY conflicting class-member
//! declaration, not only the subsequent ones.
//!
//! Two sites in `class_member_checks.rs` under-reported:
//!
//!   1. same-kind duplicate accessors skipped the first declaration for public
//!      names (`start = if info.is_private { 0 } else { 1 }`) — private names
//!      already reported all of them;
//!   2. a public `get`+`set` pair declared *before* a conflicting
//!      property/method suppressed the accessors, on the theory that the pair
//!      "established" the member. The oracle for `compiler/duplicateClassElements.ts`
//!      disproves that: tsc reports the getter, the setter and the property.
//!
//! The private-name variant of (2) is deliberately untouched — no oracle row
//! contradicts it.

use tsz_checker::test_utils::check_source_diagnostics;

fn ts2300_count(source: &str) -> usize {
    check_source_diagnostics(source)
        .iter()
        .filter(|d| d.code == 2300)
        .count()
}

#[test]
fn duplicate_public_accessors_report_every_declaration() {
    assert_eq!(
        ts2300_count("class C { get x() { return 1; } get x() { return 1; } }"),
        2,
        "both getters must be flagged, not just the second"
    );
    assert_eq!(
        ts2300_count("class C { set x(v: number) {} set x(v: number) {} }"),
        2,
        "both setters must be flagged, not just the second"
    );
}

#[test]
fn accessor_pair_before_conflicting_property_reports_all_three() {
    assert_eq!(
        ts2300_count("class C { get x() { return 1; } set x(v: number) {} x: any; }"),
        3,
        "a get+set pair does not establish the member; all three are duplicates"
    );
}

/// The mirrored order was already correct and must stay so.
#[test]
fn property_before_accessor_pair_still_reports_all_three() {
    assert_eq!(
        ts2300_count("class C { x: any; get x() { return 1; } set x(v: number) {} }"),
        3
    );
}

/// Duplicate accessors *plus* a property: three declarations, three diagnostics.
#[test]
fn duplicate_accessors_with_conflicting_property_report_all_three() {
    assert_eq!(
        ts2300_count("class C { get x() { return 1; } get x() { return 1; } x: any; }"),
        3
    );
}

/// Adjacent cases that were already correct — guards against over-reporting.
#[test]
fn single_accessor_and_plain_duplicates_are_unchanged() {
    assert_eq!(
        ts2300_count("class C { get x() { return 1; } x: any; }"),
        2,
        "one accessor plus one property is two duplicates"
    );
    assert_eq!(ts2300_count("class C { x: any; x: any; }"), 2);
}

/// Object literals route through a different path and must not shift.
#[test]
fn object_literal_duplicate_accessors_unchanged() {
    assert_eq!(
        ts2300_count("var o = { get x() { return 1; }, get x() { return 1; } };"),
        2
    );
}

/// A valid get/set pair with no conflict is not a duplicate at all.
#[test]
fn valid_accessor_pair_reports_nothing() {
    assert_eq!(
        ts2300_count("class C { get x(): number { return 1; } set x(v: number) {} }"),
        0,
        "a matched getter/setter pair is legal"
    );
}

/// Renamed binders and a static group, so the rule is not tied to a name or to
/// instance members.
#[test]
fn rule_holds_for_renamed_binders_and_static_members() {
    assert_eq!(
        ts2300_count(
            "class Holder { static get someValue() { return 1; } static get someValue() { return 1; } }"
        ),
        2
    );
}
