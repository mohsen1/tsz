//! Nested character classes inside a `ClassSetExpression` (the `v` flag).
//!
//! Under `v` (Unicode Sets) a `[` inside a character class opens a *nested*
//! class rather than contributing a literal `[`, and a class that has committed
//! to a `--`/`&&` class-set operator no longer admits ranges. Without `v` the
//! nested form is not grammar at all, so both rules must stay flag-gated.
//!
//! Every row below is pinned against `typescript@7.0.2`
//! (`--noEmit --strict --target esnext`), including the reported column.
use crate::parser::test_fixture::parse_source;

fn regex_codes(source: &str) -> Vec<u32> {
    let (parser, _root) = parse_source(source);
    parser.get_diagnostics().iter().map(|d| d.code).collect()
}

/// `(code, zero-based offset)` pairs — the offset is what the CLI renders as a
/// column, and a nested-class fix that lands the code on the wrong operand is
/// only visible here.
fn regex_codes_at(source: &str) -> Vec<(u32, u32)> {
    let (parser, _root) = parse_source(source);
    parser
        .get_diagnostics()
        .iter()
        .map(|d| (d.code, d.start))
        .collect()
}

// ---------------------------------------------------------------------------
// A nested class is grammar under `v` and only under `v`
// ---------------------------------------------------------------------------

#[test]
fn nested_class_under_v_flag_is_clean() {
    assert_eq!(regex_codes("const a = /[a[b]]/v;"), Vec::<u32>::new());
}

/// The same pattern under `u` keeps tsc's TS1508: outside `v` the inner `[` is
/// an unescaped literal and the trailing `]` has no class to close.
#[test]
fn nested_class_under_u_flag_still_reports_ts1508() {
    assert_eq!(regex_codes("const a = /[a[b]]/u;"), vec![1508]);
}

/// Annex B (no `u`/`v`) accepts the shape outright, so nothing may fire.
#[test]
fn nested_class_without_unicode_flags_is_clean() {
    assert_eq!(regex_codes("const a = /[a[b]]/;"), Vec::<u32>::new());
}

#[test]
fn adjacent_and_repeated_nested_classes_are_clean() {
    assert_eq!(regex_codes("const a = /[[a][b]]/v;"), Vec::<u32>::new());
    assert_eq!(regex_codes("const a = /[[[a]]]/v;"), Vec::<u32>::new());
    assert_eq!(regex_codes("const a = /[a[b]c]/v;"), Vec::<u32>::new());
}

#[test]
fn empty_and_negated_nested_classes_are_clean() {
    assert_eq!(regex_codes("const a = /[[]]/v;"), Vec::<u32>::new());
    assert_eq!(regex_codes("const a = /[a[]]/v;"), Vec::<u32>::new());
    assert_eq!(regex_codes("const a = /[[^a]]/v;"), Vec::<u32>::new());
    assert_eq!(regex_codes("const a = /[^[a][b]]/v;"), Vec::<u32>::new());
}

#[test]
fn nested_class_operands_of_class_set_operators_are_clean() {
    assert_eq!(regex_codes("const a = /[a--[b]]/v;"), Vec::<u32>::new());
    assert_eq!(regex_codes("const a = /[a&&[b]]/v;"), Vec::<u32>::new());
    assert_eq!(regex_codes("const a = /[[a]&&[b]]/v;"), Vec::<u32>::new());
}

#[test]
fn class_escapes_beside_a_nested_class_are_clean() {
    assert_eq!(regex_codes("const a = /[\\d[a]]/v;"), Vec::<u32>::new());
    assert_eq!(regex_codes("const a = /[[a]\\d]/v;"), Vec::<u32>::new());
    assert_eq!(regex_codes("const a = /[[a-z]]/v;"), Vec::<u32>::new());
}

/// A `]` that closes nothing is still TS1508 — the fix must consume exactly the
/// nested class, not swallow the rest of the pattern.
#[test]
fn stray_bracket_after_a_nested_class_still_reports_ts1508() {
    // `/[[a]]]/v` — offset 16 is the third `]`, the one with no open class.
    assert_eq!(regex_codes_at("const a = /[[a]]]/v;"), vec![(1508, 16)]);
}

// ---------------------------------------------------------------------------
// A nested class cannot bound a range (TS1516), and the report lands on the
// offending bound
// ---------------------------------------------------------------------------

#[test]
fn nested_class_as_range_maximum_reports_ts1516_on_the_maximum() {
    // `/[a-[b]]/v` — offset 14 is the `[` that opens the nested class.
    assert_eq!(regex_codes_at("const a = /[a-[b]]/v;"), vec![(1516, 14)]);
}

#[test]
fn nested_class_as_range_minimum_reports_ts1516_on_the_minimum() {
    // `/[[a]-b]/v` — offset 12 is the `[` that opens the nested class.
    assert_eq!(regex_codes_at("const a = /[[a]-b]/v;"), vec![(1516, 12)]);
}

#[test]
fn nested_classes_on_both_range_bounds_report_ts1516_twice() {
    assert_eq!(
        regex_codes_at("const a = /[[a]-[b]]/v;"),
        vec![(1516, 12), (1516, 16)]
    );
}

// ---------------------------------------------------------------------------
// A committed `ClassSetExpression` admits no ranges, so neither the range-bound
// check (TS1516) nor the range-order check (TS1517) may fire inside one
// ---------------------------------------------------------------------------

#[test]
fn hyphen_after_a_class_set_operator_is_not_a_range() {
    assert_eq!(regex_codes("const a = /[a--b-c]/v;"), Vec::<u32>::new());
    assert_eq!(regex_codes("const a = /[a&&b-c]/v;"), Vec::<u32>::new());
    assert_eq!(regex_codes("const a = /[[a]--b-c]/v;"), Vec::<u32>::new());
}

/// The witness that first exposed this: a descending pair inside a subtraction
/// is not a range, so TS1517 must not fire on it.
#[test]
fn descending_pair_inside_a_subtraction_does_not_report_ts1517() {
    assert_eq!(
        regex_codes("const a = /[[a]--\\P{L}-_]/v;"),
        Vec::<u32>::new()
    );
}

/// Suppression is scoped to the class that carries the operator: a plain class
/// elsewhere in the same pattern keeps its range diagnostics.
#[test]
fn class_set_suppression_does_not_leak_to_a_sibling_class() {
    assert_eq!(regex_codes("const a = /[a--b][z-a]/v;"), vec![1517]);
}

/// And it does not leak into a nested class either.
#[test]
fn class_set_suppression_does_not_leak_into_a_nested_class() {
    assert_eq!(regex_codes("const a = /[a--[z-a]]/v;"), vec![1517]);
}

/// Ranges keep working in a `v`-mode class that never uses an operator.
#[test]
fn plain_range_under_v_flag_is_unaffected() {
    assert_eq!(regex_codes("const a = /[a-b]/v;"), Vec::<u32>::new());
    assert_eq!(regex_codes("const a = /[z-a]/v;"), vec![1517]);
}

/// `--`/`&&` are `v`-only spellings; under `u` a `-` keeps its range meaning and
/// the descending pair must still report.
#[test]
fn class_set_suppression_is_v_only() {
    assert_eq!(regex_codes("const a = /[z-a]/u;"), vec![1517]);
}
