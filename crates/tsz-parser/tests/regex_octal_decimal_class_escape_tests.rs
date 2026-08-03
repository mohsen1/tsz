//! Legacy octal (TS1487/TS1536) and decimal (TS1537) escape validation for
//! digit escapes reached by a regular-expression literal's character-class
//! walker (`crates/tsz-parser/src/parser/state_expressions_literals_regex.rs`,
//! `scan_character_escape`'s `\0`..`\9` arms).
//!
//! `\1`..`\9` at atom position (outside a class) are backreferences, a
//! distinct family (#16291/#16296) this suite does not touch — see
//! `atom_position_backreference_digits_are_not_judged_by_this_family` below,
//! which pins that this family must not reach them.
//!
//! Every row is pinned against `typescript@7.0.2`.
use crate::parser::test_fixture::parse_source;

fn codes(source: &str) -> Vec<u32> {
    let (parser, _root) = parse_source(source);
    parser.get_diagnostics().iter().map(|d| d.code).collect()
}

fn message(source: &str) -> String {
    let (parser, _root) = parse_source(source);
    parser
        .get_diagnostics()
        .first()
        .map_or_else(String::new, |d| d.message.clone())
}

// ---------------------------------------------------------------------------
// `\0` — legal everywhere unless followed by another digit
// ---------------------------------------------------------------------------

#[test]
fn bare_null_escape_in_a_character_class_is_legal() {
    assert!(codes(r"const a = /[\0]/;").is_empty());
}

/// `\0` followed by a non-digit is still the NUL escape, not the start of an
/// octal run.
#[test]
fn null_escape_followed_by_a_non_digit_is_legal() {
    assert!(codes(r"const a = /[\0a]/;").is_empty());
}

#[test]
fn null_escape_followed_by_a_digit_reports_ts1487_in_a_character_class() {
    assert_eq!(codes(r"const a = /[\01]/;"), vec![1487]);
}

/// The suggested replacement renders the actual octal digits consumed
/// (`\x00`), not the trailing non-octal `8`.
#[test]
fn null_escape_followed_by_a_non_octal_digit_still_reports_ts1487() {
    assert_eq!(codes(r"const a = /[\08]/;"), vec![1487]);
    assert!(
        message(r"const a = /[\08]/;").contains("\\x00"),
        "must suggest \\x00: {}",
        message(r"const a = /[\08]/;")
    );
}

/// `\0` followed by a digit is TS1487 at atom position too — this half of
/// the rule is not specific to character classes.
#[test]
fn null_escape_followed_by_a_digit_reports_ts1487_outside_a_character_class() {
    assert_eq!(codes(r"const a = /\01/;"), vec![1487]);
}

#[test]
fn bare_null_escape_outside_a_character_class_is_legal() {
    assert!(codes(r"const a = /\0/;").is_empty());
}

// ---------------------------------------------------------------------------
// TS1536 — a leading `1`-`7` digit in a character class is a legacy octal
// escape
// ---------------------------------------------------------------------------

#[test]
fn single_octal_digit_in_a_character_class_reports_ts1536() {
    assert_eq!(codes(r"const a = /[\1]/;"), vec![1536]);
    assert_eq!(codes(r"const a = /[\7]/;"), vec![1536]);
}

/// Leading `0`-`3` may pull up to 3 octal digits: `\123` is one escape with
/// value 0o123 = 0x53.
#[test]
fn leading_zero_to_three_consumes_up_to_three_octal_digits() {
    assert_eq!(codes(r"const a = /[\123]/;"), vec![1536]);
    assert!(
        message(r"const a = /[\123]/;").contains("\\x53"),
        "must render the full 3-digit octal value: {}",
        message(r"const a = /[\123]/;")
    );
}

/// Leading `4`-`7` may only pull 2 octal digits: `\400` is `\40` (value
/// 0o40 = 0x20) followed by a literal `0`, not a 3-digit escape.
#[test]
fn leading_four_to_seven_consumes_only_two_octal_digits() {
    assert_eq!(codes(r"const a = /[\400]/;"), vec![1536]);
    assert!(
        message(r"const a = /[\400]/;").contains("\\x20"),
        "must truncate to the 2-digit octal value: {}",
        message(r"const a = /[\400]/;")
    );
    assert_eq!(codes(r"const a = /[\777]/;"), vec![1536]);
    assert!(
        message(r"const a = /[\777]/;").contains("\\x3f"),
        "must truncate to the 2-digit octal value: {}",
        message(r"const a = /[\777]/;")
    );
}

/// A 2-digit run entirely within the leading-0-3 budget still stops at the
/// digits actually present.
#[test]
fn short_octal_run_uses_only_the_digits_present() {
    assert_eq!(codes(r"const a = /[\12]/;"), vec![1536]);
    assert!(
        message(r"const a = /[\12]/;").contains("\\x0a"),
        "0o12 == 0x0a: {}",
        message(r"const a = /[\12]/;")
    );
}

/// Not gated on the `u` flag — tsc validates this with and without Unicode
/// mode.
#[test]
fn octal_class_escape_reports_ts1536_under_the_unicode_flag() {
    assert_eq!(codes(r"const a = /[\1]/u;"), vec![1536]);
}

/// A class-escape octal digit at the start of a range is still judged before
/// the `-` is parsed.
#[test]
fn octal_class_escape_at_the_start_of_a_range_reports_ts1536() {
    assert_eq!(codes(r"const a = /[\1-3]/;"), vec![1536]);
}

/// Two independent octal escapes in the same class each report.
#[test]
fn two_octal_class_escapes_each_report() {
    assert_eq!(codes(r"const a = /[\1\2]/;"), vec![1536, 1536]);
}

// ---------------------------------------------------------------------------
// TS1537 — a leading `8`/`9` digit in a character class is a decimal escape
// ---------------------------------------------------------------------------

#[test]
fn leading_eight_or_nine_in_a_character_class_reports_ts1537() {
    assert_eq!(codes(r"const a = /[\8]/;"), vec![1537]);
    assert_eq!(codes(r"const a = /[\9]/;"), vec![1537]);
}

/// The span covers only the first digit — `\89` is `\8` (TS1537) followed by
/// a literal `9`, not a two-digit escape.
#[test]
fn decimal_class_escape_span_is_one_digit_only() {
    assert_eq!(codes(r"const a = /[\89]/;"), vec![1537]);
}

#[test]
fn decimal_class_escape_reports_ts1537_under_the_unicode_flag() {
    assert_eq!(codes(r"const a = /[\8]/u;"), vec![1537]);
}

// ---------------------------------------------------------------------------
// Not gated on strict mode
// ---------------------------------------------------------------------------

#[test]
fn octal_and_decimal_class_escapes_report_without_strict_mode() {
    let (parser, _root) = parse_source(r"const a = /[\1]/;");
    assert!(parser.get_diagnostics().iter().any(|d| d.code == 1536));
    let (parser, _root) = parse_source(r"const a = /[\8]/;");
    assert!(parser.get_diagnostics().iter().any(|d| d.code == 1537));
}

// ---------------------------------------------------------------------------
// Negative / boundary controls
// ---------------------------------------------------------------------------

/// `\1`..`\9` outside a character class are backreferences, a distinct
/// family (#16291/#16296) that this family must not reach — neither TS1536
/// nor TS1537 fires there, whatever backreference validation eventually
/// decides.
#[test]
fn atom_position_backreference_digits_are_not_judged_by_this_family() {
    for source in [
        r"const a = /\1/;",
        r"const a = /\8/;",
        r"const a = /(a)\1/;",
    ] {
        let cs = codes(source);
        assert!(
            !cs.contains(&1536) && !cs.contains(&1537),
            "atom-position digit escapes must stay outside the character-class family, got {cs:?} for {source}"
        );
    }
}

/// Non-digit character-class escapes are unaffected.
#[test]
fn non_digit_character_class_escapes_stay_clean() {
    assert!(codes(r"const a = /[\d\s\w]/;").is_empty());
    assert!(codes(r"const a = /[\x41]/;").is_empty());
}

/// A realistic pattern with a digit-shaped literal (not an escape) inside a
/// class stays clean — this family only judges `\`-prefixed digits.
#[test]
fn realistic_patterns_report_nothing() {
    for source in [
        r"const a = /[0-9]/;",
        r"const a = /[a-zA-Z0-9_]/;",
        r"const a = /^[\d]{3}-[\d]{4}$/;",
    ] {
        assert!(
            codes(source).is_empty(),
            "expected no diagnostics for {source}, got {:?}",
            codes(source)
        );
    }
}
