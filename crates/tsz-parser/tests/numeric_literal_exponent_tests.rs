//! Locks in TS1124 ("Digit expected") emission for numeric-literal exponents.
//! tsc only emits TS1124 when the exponent has no actual digits (e.g. `1e+`,
//! `1e_`, `1e+_`). When a digit is present but a separator is misplaced
//! (e.g. `0e_0`), the scanner emits TS6188 alone — TS1124 is **not** also
//! emitted, since the exponent is well-formed except for the rejected `_`.
//!
//! Regression: `parser.numericSeparators.decmialNegative.ts` files 9.ts,
//! 15.ts, 22.ts, 28.ts, 35.ts, 41.ts. Before the fix, tsz emitted both
//! TS6188 (at the underscore) and TS1124 (at end-of-literal); tsc emits
//! only TS6188.

use crate::parser::state::ParserState;

fn parse_codes(source: &str) -> Vec<u32> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let _root = parser.parse_source_file();
    parser
        .scanner
        .get_scanner_diagnostics()
        .iter()
        .map(|d| d.code)
        .chain(parser.parse_diagnostics.iter().map(|d| d.code))
        .collect()
}

#[test]
fn underscore_after_e_with_digit_emits_only_ts6188() {
    // `0e_0` — separator misplaced after `e`, but a digit follows. tsc
    // emits only TS6188 at the `_`, not TS1124 at end-of-literal.
    let codes = parse_codes("0e_0");
    assert!(
        codes.contains(&6188),
        "expected TS6188 for misplaced underscore in exponent, got: {codes:?}"
    );
    assert!(
        !codes.contains(&1124),
        "TS1124 should NOT fire when an exponent digit is present, got: {codes:?}"
    );
}

#[test]
fn underscore_after_signed_exponent_with_digit_emits_only_ts6188() {
    // `0e+_0` — sign present, then misplaced underscore, then digit.
    let codes = parse_codes("0e+_0");
    assert!(
        codes.contains(&6188),
        "expected TS6188 for misplaced underscore after `+`, got: {codes:?}"
    );
    assert!(
        !codes.contains(&1124),
        "TS1124 should NOT fire when an exponent digit is present, got: {codes:?}"
    );
}

#[test]
fn empty_exponent_still_emits_ts1124() {
    // `1e+` — genuinely missing digit, must still emit TS1124.
    let codes = parse_codes("1e+");
    assert!(
        codes.contains(&1124),
        "TS1124 should fire when exponent has no digits, got: {codes:?}"
    );
}

#[test]
fn underscore_only_exponent_emits_both() {
    // `1e_` — separator at start of exponent and no digit after. tsc emits
    // TS6188 (separator not allowed) and TS1124 (digit expected).
    let codes = parse_codes("1e_");
    assert!(
        codes.contains(&6188),
        "expected TS6188 for misplaced underscore in empty exponent, got: {codes:?}"
    );
    assert!(
        codes.contains(&1124),
        "TS1124 should fire when exponent ends with no digit, got: {codes:?}"
    );
}

#[test]
fn parser_state_reset_clears_scanner_diagnostics_and_high_water_mark() {
    // Devin review on PR #1521: ParserState::reset must clear both the
    // scanner's accumulated diagnostics vec AND the high-water mark used by
    // parse_error_at's dedup. Otherwise a reused parser (LSP `update_source`
    // path) carries stale entries; if a new-parse parser error happens to
    // share a `start` with the LAST stale scanner diagnostic, dedup wrongly
    // suppresses the new error.

    // Parse 1 — produce some scanner diagnostics (e.g. TS1124 on `1e+`).
    let mut parser = ParserState::new("first.ts".to_string(), "1e+".to_string());
    let _ = parser.parse_source_file();
    let scan_count_after_first = parser.scanner.get_scanner_diagnostics().len();
    assert!(
        scan_count_after_first > 0,
        "first parse must produce at least one scanner diagnostic"
    );

    // Reset for parse 2 — must wipe scanner diagnostics and HWM.
    parser.reset("second.ts".to_string(), "let x = 1.5e+10;".to_string());
    assert_eq!(
        parser.scanner.get_scanner_diagnostics().len(),
        0,
        "reset must clear scanner_diagnostics so reused-parser callers don't see stale entries"
    );
    assert_eq!(
        parser.scanner_diagnostics_high_water_mark, 0,
        "reset must clear the high-water mark (otherwise the parser-side dedup at parse_error_at sees a non-zero floor)"
    );

    // Parse 2 — well-formed source, must produce zero scanner diagnostics.
    let _ = parser.parse_source_file();
    let codes: Vec<u32> = parser
        .scanner
        .get_scanner_diagnostics()
        .iter()
        .map(|d| d.code)
        .chain(parser.parse_diagnostics.iter().map(|d| d.code))
        .collect();
    assert!(
        !codes.contains(&1124) && !codes.contains(&6188),
        "second parse of clean source must not surface stale-scanner-diagnostic codes, got: {codes:?}"
    );
}

#[test]
fn well_formed_decimal_with_exponent_emits_no_diagnostics() {
    // Sanity: `1.5e+10` is fully well-formed.
    let codes = parse_codes("let x = 1.5e+10;");
    assert!(
        !codes.contains(&1124) && !codes.contains(&6188) && !codes.contains(&6189),
        "well-formed numeric literal should produce no separator/digit diagnostics, got: {codes:?}"
    );
}
