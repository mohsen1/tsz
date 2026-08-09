//! TS1520 — `Expected a class set operand.` for the `v`-flag class-set
//! operators `--` (subtraction) and `&&` (intersection).
//!
//! Both operators are binary: the grammar is `ClassSetOperand -- ClassSetOperand`
//! and `ClassSetOperand && ClassSetOperand`, so an operator needs an operand on
//! *each* side. tsc reports TS1520 for every missing operand:
//!
//! - a *right* operand goes missing when the operator is immediately followed
//!   by `]` or by another operator (`/[a&&]/v`, `/[a--]/v`), and
//! - a *left* operand goes missing when the operator opens the class or
//!   immediately follows `^` (`/[&&a]/v`, `/[--a]/v`, `/[^--a]/v`).
//!
//! A class that opens on a bare operator therefore draws *two* TS1520s
//! (`/[&&]/v`), one per side, while a class that opens on a normal operand and
//! ends on a bare operator draws one.
//!
//! The reported column is the first character of the missing side's anchor:
//! the operator's first character for a missing left operand, and the token
//! that stood where the right operand should have been (`]` or the next
//! operator) for a missing right operand — matching `typescript@7.0.2`
//! (`--noEmit --strict --target es2024 --lib es2024`).
use crate::parser::test_fixture::parse_source;
use tsz_common::diagnostics::diagnostic_codes;

const TS1520: u32 = diagnostic_codes::EXPECTED_A_CLASS_SET_OPERAND;

fn regex_codes(source: &str) -> Vec<u32> {
    let (parser, _root) = parse_source(source);
    parser.get_diagnostics().iter().map(|d| d.code).collect()
}

/// `(code, zero-based offset)` pairs — the offset is what the CLI renders as a
/// column, and a mis-anchored operand report is only visible here.
fn regex_codes_at(source: &str) -> Vec<(u32, u32)> {
    let (parser, _root) = parse_source(source);
    parser
        .get_diagnostics()
        .iter()
        .map(|d| (d.code, d.start))
        .collect()
}

/// Offset of `needle` in `source`, for anchoring the expected report.
fn at(source: &str, needle: &str) -> u32 {
    source.find(needle).expect("needle in source") as u32
}

// ---------------------------------------------------------------------------
// Missing right operand: the operator is the last thing before `]`
// ---------------------------------------------------------------------------

#[test]
fn a_trailing_operator_reports_a_missing_right_operand() {
    assert_eq!(regex_codes("const a = /[a&&]/v;"), vec![TS1520]);
    assert_eq!(regex_codes("const a = /[a--]/v;"), vec![TS1520]);
}

#[test]
fn a_missing_right_operand_anchors_on_the_closing_bracket() {
    let src = "const a = /[a&&]/v;";
    assert_eq!(regex_codes_at(src), vec![(TS1520, at(src, "]"))]);
    let src = "const a = /[a--]/v;";
    assert_eq!(regex_codes_at(src), vec![(TS1520, at(src, "]"))]);
}

#[test]
fn a_missing_right_operand_after_a_nested_class_still_reports() {
    // `[[a]--]` — the left operand is the nested class `[a]`, the right is gone.
    assert_eq!(regex_codes("const a = /[[a]--]/v;"), vec![TS1520]);
}

// ---------------------------------------------------------------------------
// Missing left operand: the class opens on the operator
// ---------------------------------------------------------------------------

#[test]
fn a_leading_operator_reports_a_missing_left_operand() {
    assert_eq!(regex_codes("const a = /[&&a]/v;"), vec![TS1520]);
    assert_eq!(regex_codes("const a = /[--a]/v;"), vec![TS1520]);
}

#[test]
fn a_missing_left_operand_anchors_on_the_operators_first_character() {
    let src = "const a = /[&&a]/v;";
    assert_eq!(regex_codes_at(src), vec![(TS1520, at(src, "&&"))]);
    let src = "const a = /[--a]/v;";
    assert_eq!(regex_codes_at(src), vec![(TS1520, at(src, "--"))]);
}

#[test]
fn a_leading_operator_after_negation_reports_a_missing_left_operand() {
    // The `^` is not an operand, so `[^--a]` still opens on the operator.
    let src = "const a = /[^--a]/v;";
    assert_eq!(regex_codes(src), vec![TS1520]);
    assert_eq!(regex_codes_at(src), vec![(TS1520, at(src, "--"))]);
}

#[test]
fn a_leading_operator_before_a_nested_class_reports_a_missing_left_operand() {
    // The right operand (`[a]`) is present; only the left is missing.
    let src = "const a = /[--[a]]/v;";
    assert_eq!(regex_codes(src), vec![TS1520]);
    assert_eq!(regex_codes_at(src), vec![(TS1520, at(src, "--"))]);
}

// ---------------------------------------------------------------------------
// Both operands missing: a class that is nothing but a bare operator
// ---------------------------------------------------------------------------

#[test]
fn a_bare_operator_class_reports_both_sides() {
    // Two TS1520s: the leading missing-left, then the trailing missing-right.
    assert_eq!(regex_codes("const a = /[&&]/v;"), vec![TS1520, TS1520]);
    assert_eq!(regex_codes("const a = /[--]/v;"), vec![TS1520, TS1520]);
}

#[test]
fn a_bare_operator_class_anchors_both_reports() {
    let src = "const a = /[&&]/v;";
    assert_eq!(
        regex_codes_at(src),
        vec![(TS1520, at(src, "&&")), (TS1520, at(src, "]"))]
    );
    let src = "const a = /[--]/v;";
    assert_eq!(
        regex_codes_at(src),
        vec![(TS1520, at(src, "--")), (TS1520, at(src, "]"))]
    );
}

// ---------------------------------------------------------------------------
// A leading operator with an operand on the far side is still one report:
// the run of operators commits the class's kind, so only the opening side is
// counted as missing (matching tsc's single leading TS1520).
// ---------------------------------------------------------------------------

#[test]
fn a_leading_operator_with_more_operands_reports_only_the_opening_side() {
    assert_eq!(regex_codes("const a = /[&&a&&b]/v;"), vec![TS1520]);
    assert_eq!(regex_codes("const a = /[--a--b]/v;"), vec![TS1520]);
}

// ---------------------------------------------------------------------------
// Negative controls: a two-sided operator with both operands present is clean
// ---------------------------------------------------------------------------

#[test]
fn a_well_formed_operator_is_clean() {
    assert_eq!(regex_codes("const a = /[a&&b]/v;"), Vec::<u32>::new());
    assert_eq!(regex_codes("const a = /[a--b]/v;"), Vec::<u32>::new());
    assert_eq!(regex_codes("const a = /[[a--b]&&c]/v;"), Vec::<u32>::new());
}

// ---------------------------------------------------------------------------
// The rule is `v`-only: under `u` and Annex B, `-`/`&` at the class edge are
// ordinary class content, not operators, so no operand is ever missing.
// ---------------------------------------------------------------------------

#[test]
fn missing_operand_is_v_only() {
    assert_eq!(regex_codes("const a = /[--a]/u;"), Vec::<u32>::new());
    assert_eq!(regex_codes("const a = /[&&a]/u;"), Vec::<u32>::new());
    assert_eq!(regex_codes("const a = /[--a]/;"), Vec::<u32>::new());
    assert_eq!(regex_codes("const a = /[&&a]/;"), Vec::<u32>::new());
}
