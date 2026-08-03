//! Mixing class-set operators inside one `v`-mode character class (TS1519).
//!
//! Under `v` the first operator a class uses fixes its production — union (a
//! range or a bare `-`), subtraction (`--`) or intersection (`&&`) — and a
//! later operator of a different kind is an error. The commitment is scoped to
//! one class: a nested class recurses and gets its own, which is what makes
//! `/[[a--b]&&c]/v` legal.
//!
//! Every row is pinned against `typescript@7.0.2`
//! (`--noEmit --strict --pretty false --target esnext`), on code *and* column.
//! `tsc` renders 1-based columns; this harness reports zero-based offsets, so
//! each expectation below is the oracle's column minus one.
use crate::parser::test_fixture::parse_source;

fn regex_codes(source: &str) -> Vec<u32> {
    let (parser, _root) = parse_source(source);
    parser.get_diagnostics().iter().map(|d| d.code).collect()
}

/// `(code, zero-based offset)` — the offset is what the CLI renders as a
/// column, and an operator-mixing fix that reports on the wrong operand is only
/// visible here.
fn regex_codes_at(source: &str) -> Vec<(u32, u32)> {
    let (parser, _root) = parse_source(source);
    parser
        .get_diagnostics()
        .iter()
        .map(|d| (d.code, d.start))
        .collect()
}

// ---------------------------------------------------------------------------
// Two different class-set operators in one class
// ---------------------------------------------------------------------------

#[test]
fn subtraction_then_intersection_reports_on_the_intersection() {
    // `/[a--b&&c]/v` — offset 16 is the `&&`, the operator that disagrees.
    // (tsc renders it as column 17; the harness offset is zero-based.)
    assert_eq!(regex_codes_at("const a = /[a--b&&c]/v;"), vec![(1519, 16)]);
}

#[test]
fn intersection_then_subtraction_reports_on_the_subtraction() {
    assert_eq!(regex_codes_at("const a = /[a&&b--c]/v;"), vec![(1519, 16)]);
}

/// The leading `^` shifts every offset by one; an off-by-one in the report
/// position is otherwise invisible.
#[test]
fn negated_class_reports_at_the_shifted_offset() {
    assert_eq!(regex_codes_at("const a = /[^a--b&&c]/v;"), vec![(1519, 17)]);
}

/// A nested class is an operand, so the operator after it still belongs to the
/// outer class and still mixes.
#[test]
fn operator_after_a_nested_class_operand_still_mixes() {
    assert_eq!(
        regex_codes_at("const a = /[[a]--b&&c]/v;"),
        vec![(1519, 18)]
    );
}

/// tsc reports the mixture once, on the first operator that disagrees — not
/// once per subsequent operator.
#[test]
fn a_mixed_class_reports_exactly_once() {
    assert_eq!(
        regex_codes_at("const a = /[a--b&&c--d]/v;"),
        vec![(1519, 16)]
    );
}

// ---------------------------------------------------------------------------
// A range commits the class to union, so a class-set operator after one mixes
// ---------------------------------------------------------------------------

#[test]
fn range_then_subtraction_reports_on_the_subtraction() {
    // `/[a-b--c]/v` — offset 15 is the `--`; the range `a-b` came first.
    assert_eq!(regex_codes_at("const a = /[a-b--c]/v;"), vec![(1519, 15)]);
}

#[test]
fn range_then_intersection_reports_on_the_intersection() {
    assert_eq!(regex_codes_at("const a = /[a-b&&c]/v;"), vec![(1519, 15)]);
}

// ---------------------------------------------------------------------------
// A bare `-` inside a committed set expression is a union operator, not a range
// ---------------------------------------------------------------------------

#[test]
fn hyphen_after_a_subtraction_reports_on_the_hyphen() {
    // `/[a--b-c]/v` — offset 16 is the bare `-`, which is a mixed union
    // operator here rather than a range separator.
    assert_eq!(regex_codes_at("const a = /[a--b-c]/v;"), vec![(1519, 16)]);
}

#[test]
fn hyphen_after_an_intersection_reports_on_the_hyphen() {
    assert_eq!(regex_codes_at("const a = /[a&&b-c]/v;"), vec![(1519, 16)]);
}

// ---------------------------------------------------------------------------
// Negative controls: a repeated operator is legal, and nesting scopes the
// commitment
// ---------------------------------------------------------------------------

/// Repeating the *same* operator is not mixing — a fix keyed on "a second
/// operator appeared" regresses these two.
#[test]
fn a_repeated_operator_is_not_mixing() {
    assert_eq!(regex_codes("const a = /[a--b--c]/v;"), Vec::<u32>::new());
    assert_eq!(regex_codes("const a = /[a&&b&&c]/v;"), Vec::<u32>::new());
}

/// Each class carries its own commitment, so an inner operator and a different
/// outer one do not mix. A fix that hoists the state out of `scan_class_ranges`
/// regresses these two.
#[test]
fn nesting_scopes_the_commitment() {
    assert_eq!(regex_codes("const a = /[[a--b]&&c]/v;"), Vec::<u32>::new());
    assert_eq!(regex_codes("const a = /[a--[b&&c]]/v;"), Vec::<u32>::new());
}

/// A class that uses one operator, or none, is unaffected.
#[test]
fn a_single_operator_or_none_is_clean() {
    assert_eq!(regex_codes("const a = /[a--b]/v;"), Vec::<u32>::new());
    assert_eq!(regex_codes("const a = /[a&&b]/v;"), Vec::<u32>::new());
    assert_eq!(regex_codes("const a = /[a-b]/v;"), Vec::<u32>::new());
    assert_eq!(regex_codes("const a = /[abc]/v;"), Vec::<u32>::new());
}

/// `--`/`&&` are `v`-only spellings: under `u` and under Annex B the same text
/// is ordinary class content, so TS1519 must never fire outside `v`.
#[test]
fn mixing_is_v_only() {
    assert_eq!(regex_codes("const a = /[a--b&&c]/u;"), Vec::<u32>::new());
    assert_eq!(regex_codes("const a = /[a--b&&c]/;"), Vec::<u32>::new());
    assert_eq!(regex_codes("const a = /[a&&b--c]/;"), Vec::<u32>::new());
}

/// The range-order pass (TS1517) is driven by its own walk and must keep
/// working inside a class that also carries an operator — this is the row
/// #16301 added, re-asserted here so a TS1519 fix cannot silently take it out.
#[test]
fn range_order_diagnostics_survive_alongside_an_operator() {
    assert_eq!(regex_codes("const a = /[a--b][z-a]/v;"), vec![1517]);
    assert_eq!(regex_codes("const a = /[a--[z-a]]/v;"), vec![1517]);
}
