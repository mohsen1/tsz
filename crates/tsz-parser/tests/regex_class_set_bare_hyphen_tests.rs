//! A bare, unescaped `-` in a `v`-mode character class (TS1508).
//!
//! A `-` is only a legal `ClassSetCharacter` when it is consumed as a range
//! separator immediately after the atom it follows, in the same scan step.
//! Any `-` that instead reaches the scan as a *fresh* atom — because it opens
//! the class, stands alone, or follows a range that already completed —
//! is not legal in `v` mode and reports TS1508 ("Unexpected '-'. Did you mean
//! to escape it with backslash?"), once per occurrence rather than once per
//! class.
//!
//! Every row is pinned against `typescript@7.0.2`
//! (`--noEmit --strict --pretty false --target esnext`), on code *and*
//! column. `tsc` renders 1-based columns; this harness reports zero-based
//! offsets, so each expectation below is the oracle's column minus one.
use crate::parser::test_fixture::parse_source;

fn regex_codes(source: &str) -> Vec<u32> {
    let (parser, _root) = parse_source(source);
    parser.get_diagnostics().iter().map(|d| d.code).collect()
}

fn regex_codes_at(source: &str) -> Vec<(u32, u32)> {
    let (parser, _root) = parse_source(source);
    parser
        .get_diagnostics()
        .iter()
        .map(|d| (d.code, d.start))
        .collect()
}

// ---------------------------------------------------------------------------
// A bare `-` that is never a range separator
// ---------------------------------------------------------------------------

#[test]
fn a_lone_hyphen_reports() {
    // `/[-]/v` — offset 12 is the `-`, the class's only content.
    assert_eq!(regex_codes_at("const a = /[-]/v;"), vec![(1508, 12)]);
}

#[test]
fn a_leading_hyphen_before_an_atom_reports() {
    // `/[-a]/v` — the leading `-` cannot pair with anything to its left, so
    // it is not a range separator despite `a` following it.
    assert_eq!(regex_codes_at("const a = /[-a]/v;"), vec![(1508, 12)]);
}

#[test]
fn a_hyphen_after_a_completed_range_reports() {
    // `/[a-b-c]/v` — offset 15 is the second `-`. `a-b` is a legal range;
    // the `-` right after it is a fresh atom scan, not a second range.
    assert_eq!(regex_codes_at("const a = /[a-b-c]/v;"), vec![(1508, 15)]);
}

/// Reported once per offending `-`, not once per class — a fix that gates on
/// "already reported for this class" underreports this row.
#[test]
fn a_hyphen_after_a_completed_range_reports_once_even_with_a_further_range() {
    // `/[a-b-c-d]/v` — the second `-` (offset 15) is the only bad one: the
    // third `-` legally separates the `c-d` range that follows it.
    assert_eq!(regex_codes_at("const a = /[a-b-c-d]/v;"), vec![(1508, 15)]);
}

#[test]
fn a_trailing_hyphen_after_a_completed_range_reports() {
    // `/[a-b-]/v` — offset 15 is the trailing `-`. Unlike `/[a-]/v` (below),
    // a range already completed before this `-` is reached, so it is not the
    // one-off literal-trailing-hyphen shape.
    assert_eq!(regex_codes_at("const a = /[a-b-]/v;"), vec![(1508, 15)]);
}

#[test]
fn two_bad_hyphens_in_one_class_both_report() {
    // `/[-a-b-c]/v` — the leading `-` (offset 12) and the `-` after the
    // completed `a-b` range (offset 16) are two independent occurrences.
    assert_eq!(
        regex_codes_at("const a = /[-a-b-c]/v;"),
        vec![(1508, 12), (1508, 16)]
    );
}

/// The leading `^` shifts every offset by one; an off-by-one in the report
/// position is otherwise invisible.
#[test]
fn negated_class_reports_at_the_shifted_offset() {
    // `/[^-a]/v` and `/[^a-b-]/v`.
    assert_eq!(regex_codes_at("const a = /[^-a]/v;"), vec![(1508, 13)]);
    assert_eq!(regex_codes_at("const a = /[^a-b-]/v;"), vec![(1508, 16)]);
}

/// A nested class is its own scope, same as for TS1519.
#[test]
fn a_nested_class_reports_at_its_own_offset() {
    // `/[[a-b-c]]/v` — offset 16 is the `-` inside the nested class.
    assert_eq!(regex_codes_at("const a = /[[a-b-c]]/v;"), vec![(1508, 16)]);
}

// ---------------------------------------------------------------------------
// Negative controls: a `-` consumed as a range separator is legal, however
// many atoms precede it or however many ranges the class already has
// ---------------------------------------------------------------------------

/// A trailing `-` immediately before `]` is a literal, not a range attempt —
/// as long as no range has completed yet in this class.
#[test]
fn a_trailing_hyphen_before_any_range_completes_is_clean() {
    assert_eq!(regex_codes("const a = /[a-]/v;"), Vec::<u32>::new());
    assert_eq!(regex_codes("const a = /[ab-]/v;"), Vec::<u32>::new());
}

/// A single completed range, with nothing after it, is ordinary content.
#[test]
fn a_single_range_is_clean() {
    assert_eq!(regex_codes("const a = /[a-b]/v;"), Vec::<u32>::new());
    assert_eq!(regex_codes("const a = /[a-bc-d]/v;"), Vec::<u32>::new());
}

/// A leading `-` that never reattempts a second range, once past its
/// (illegal) leading position, is otherwise ordinary — only the leading `-`
/// itself reports.
#[test]
fn a_leading_hyphen_followed_by_one_clean_range_reports_only_the_leading_one() {
    assert_eq!(regex_codes_at("const a = /[-a-b]/v;"), vec![(1508, 12)]);
    assert_eq!(regex_codes_at("const a = /[-a-]/v;"), vec![(1508, 12)]);
}

/// Two ranges, then one bad trailing `-` after the second range completes:
/// only that final `-` reports, not the legal separators before it.
#[test]
fn a_bad_hyphen_after_two_completed_ranges_reports_once() {
    assert_eq!(regex_codes_at("const a = /[a-bc-d-]/v;"), vec![(1508, 18)]);
}

/// An escaped hyphen is a `ClassEscape`, not a bare `ClassSetCharacter`, and
/// is unaffected regardless of what precedes it.
#[test]
fn an_escaped_hyphen_is_clean() {
    assert_eq!(regex_codes("const a = /[a-b\\-c]/v;"), Vec::<u32>::new());
}

/// Sibling top-level classes do not share state: the second class's trailing
/// `-` has not seen any range complete in *its own* scope.
#[test]
fn sibling_classes_do_not_share_state() {
    assert_eq!(regex_codes("const a = /[a-b][c-]/v;"), Vec::<u32>::new());
}

/// A `-` immediately after a range-bounded-by-class-escape report (TS1516)
/// is that check's own concern, not this one — TS1508 must not also fire.
/// (tsz additionally emits TS1517 here where the oracle does not — a
/// pre-existing gap in the unrelated range-order check, orthogonal to this
/// fix; both assertions only need to show 1508 is absent.)
#[test]
fn a_range_bounded_by_a_class_escape_is_not_also_ts1508() {
    assert_eq!(regex_codes("const a = /[\\d-a]/v;"), vec![1516]);
    assert_eq!(regex_codes("const a = /[\\p{L}-a]/v;"), vec![1516, 1517]);
}

/// A mixed-operator report (TS1519) on a bare `-` inside a
/// subtraction/intersection-committed class is that check's own concern; it
/// is reached through the range-separator step, not the fresh-atom step this
/// fix guards, so TS1508 must not also fire.
#[test]
fn a_mixed_operator_hyphen_is_not_also_ts1508() {
    assert_eq!(regex_codes("const a = /[a--b-c]/v;"), vec![1519]);
}

/// `--` (subtraction with no operand) is TS1520's concern; a doubled hyphen
/// is an operator, not this check's fresh-atom `-`. (The oracle reports
/// TS1520 twice for `/[--]/v` — once for each side of the bare operator —
/// where tsz reports it once; a pre-existing gap in that unrelated operand
/// check, orthogonal to this fix. Both assertions only need to show 1508 is
/// absent.)
#[test]
fn a_doubled_hyphen_is_not_also_ts1508() {
    assert_eq!(regex_codes("const a = /[a--]/v;"), vec![1520]);
    assert_eq!(regex_codes("const a = /[--]/v;"), vec![1520]);
}

/// TS1508 is a `v`-only diagnostic: under `u` and under Annex B the same
/// hyphen positions are ordinary class content.
#[test]
fn bare_hyphen_position_is_v_only() {
    assert_eq!(regex_codes("const a = /[-a]/u;"), Vec::<u32>::new());
    assert_eq!(regex_codes("const a = /[-a]/;"), Vec::<u32>::new());
    assert_eq!(regex_codes("const a = /[a-]/u;"), Vec::<u32>::new());
    assert_eq!(regex_codes("const a = /[a-]/;"), Vec::<u32>::new());
    assert_eq!(regex_codes("const a = /[a-b-c]/u;"), Vec::<u32>::new());
    assert_eq!(regex_codes("const a = /[a-b-c]/;"), Vec::<u32>::new());
}
