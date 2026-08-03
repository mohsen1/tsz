//! Numeric backreference validation inside regular expression literals
//! (TS1533 / TS1534).
//!
//! A decimal escape outside a character class is a backreference. It is legal
//! only when the pattern actually contains a capturing group with that number,
//! counted over the *whole* pattern — forward references and references across
//! alternatives are both legal, so the count cannot be accumulated as the walk
//! goes. Every row below is pinned against `typescript@7.0.2`.
use crate::parser::ParserState;
use crate::parser::test_fixture::parse_source;

fn codes(parser: &ParserState) -> Vec<u32> {
    parser.get_diagnostics().iter().map(|d| d.code).collect()
}

fn regex_codes(source: &str) -> Vec<u32> {
    let (parser, _root) = parse_source(source);
    codes(&parser)
}

/// TS1533 carries the capturing-group count as `{0}`; assert on the rendered
/// number so an off-by-one in the count is visible, not just the code.
fn regex_message(source: &str) -> String {
    let (parser, _root) = parse_source(source);
    parser
        .get_diagnostics()
        .first()
        .map_or_else(String::new, |d| d.message.clone())
}

// ---------------------------------------------------------------------------
// TS1534 — no capturing group anywhere in the pattern
// ---------------------------------------------------------------------------

#[test]
fn backreference_with_no_capturing_group_reports_ts1534() {
    assert_eq!(regex_codes("const a = /\\1/u;"), vec![1534]);
}

/// tsc validates backreferences with and without the Unicode flags, so this
/// must not be gated on `u`/`v`.
#[test]
fn backreference_with_no_capturing_group_reports_ts1534_without_unicode_flag() {
    assert_eq!(regex_codes("const a = /\\1/;"), vec![1534]);
}

#[test]
fn backreference_with_no_capturing_group_reports_ts1534_under_unicode_sets_flag() {
    assert_eq!(regex_codes("const a = /\\1/v;"), vec![1534]);
}

/// `\8` and `\9` are decimal escapes too, not literal characters.
#[test]
fn high_digit_backreference_with_no_capturing_group_reports_ts1534() {
    assert_eq!(regex_codes("const a = /\\8/;"), vec![1534]);
}

#[test]
fn non_capturing_group_does_not_satisfy_a_backreference() {
    assert_eq!(regex_codes("const a = /(?:a)\\1/u;"), vec![1534]);
}

#[test]
fn lookahead_group_does_not_satisfy_a_backreference() {
    assert_eq!(regex_codes("const a = /(?!a)\\1/u;"), vec![1534]);
}

#[test]
fn lookbehind_group_does_not_satisfy_a_backreference() {
    assert_eq!(regex_codes("const a = /(?<=a)\\1/u;"), vec![1534]);
}

/// A `(` inside a character class is a literal character, not a group.
#[test]
fn open_paren_inside_character_class_is_not_a_capturing_group() {
    assert_eq!(regex_codes("const a = /[(]\\1/u;"), vec![1534]);
}

/// An escaped `(` is a literal character, not a group.
#[test]
fn escaped_open_paren_is_not_a_capturing_group() {
    assert_eq!(regex_codes("const a = /\\(\\1/u;"), vec![1534]);
}

// ---------------------------------------------------------------------------
// TS1533 — the group number exceeds the capturing-group count
// ---------------------------------------------------------------------------

#[test]
fn backreference_past_the_group_count_reports_ts1533() {
    assert_eq!(regex_codes("const a = /(a)\\2/u;"), vec![1533]);
}

#[test]
fn backreference_past_the_group_count_reports_ts1533_without_unicode_flag() {
    assert_eq!(regex_codes("const a = /(a)\\2/;"), vec![1533]);
}

#[test]
fn backreference_past_the_group_count_reports_the_actual_count() {
    assert!(
        regex_message("const a = /(a)(b)\\3/u;").contains("only 2 capturing groups"),
        "TS1533 must render the real capturing-group count, got: {}",
        regex_message("const a = /(a)(b)\\3/u;")
    );
}

/// A multi-digit escape is one backreference to group 10, not `\1` followed by
/// a literal `0`.
#[test]
fn multi_digit_backreference_is_read_as_one_group_number() {
    assert_eq!(regex_codes("const a = /(a)\\10/u;"), vec![1533]);
}

/// The count spans the whole pattern, so a reference in one alternative sees
/// a group declared in another.
#[test]
fn group_count_spans_alternatives() {
    assert_eq!(regex_codes("const a = /(a)|\\2/u;"), vec![1533]);
    assert!(regex_codes("const a = /[a-z]|(q)\\1/u;").is_empty());
}

// ---------------------------------------------------------------------------
// Negative cases — valid backreferences must stay silent
// ---------------------------------------------------------------------------

#[test]
fn valid_backreference_is_accepted() {
    assert!(regex_codes("const a = /(a)\\1/u;").is_empty());
    assert!(regex_codes("const a = /(a)\\1/;").is_empty());
}

/// A forward reference is legal: the group is counted before the walk judges
/// any escape.
#[test]
fn forward_backreference_is_accepted() {
    assert!(regex_codes("const a = /\\1(a)/u;").is_empty());
}

/// A named group is a capturing group and contributes to the number space.
#[test]
fn named_capturing_group_counts_toward_the_group_number() {
    assert!(regex_codes("const a = /(?<n>a)\\1/u;").is_empty());
    assert!(regex_codes("const a = /(?<n>a)(?<m>b)\\2/u;").is_empty());
}

/// Nesting does not collapse the count: `(b(c(d)))` is three groups, so `\3`
/// resolves and `\4` does not.
#[test]
fn nested_groups_are_all_counted() {
    assert!(regex_codes("const a = /a(b(c(d)))\\3/u;").is_empty());
    assert_eq!(regex_codes("const a = /a(b(c(d)))\\4/u;"), vec![1533]);
}

#[test]
fn group_inside_lookbehind_is_still_a_capturing_group() {
    assert!(regex_codes("const a = /(?<=(a))\\1/u;").is_empty());
}

/// A capturing group whose body is a literal `(` still counts once.
#[test]
fn group_containing_an_escaped_paren_counts_once() {
    assert!(regex_codes("const a = /(\\()\\1/u;").is_empty());
}

/// `\0` is the NUL escape, not a backreference.
#[test]
fn null_escape_is_not_a_backreference() {
    assert!(regex_codes("const a = /\\0/u;").is_empty());
}

/// Inside a character class a decimal escape is not a backreference, so the
/// backreference rule must not reach it.
#[test]
fn decimal_escape_inside_character_class_is_not_judged_as_a_backreference() {
    let inside = regex_codes("const a = /(a)[\\1]/u;");
    assert!(
        !inside.contains(&1533) && !inside.contains(&1534),
        "character-class decimal escapes are TS1536/TS1537 territory, got {inside:?}"
    );
}

/// Ordinary real-world patterns must stay clean — this is the regression the
/// whole-pattern count exists to prevent.
#[test]
fn realistic_patterns_report_nothing() {
    for source in [
        "const a = /^(\\d{3})-(\\d{4})$/;",
        "const a = /(\\w+)\\s+\\1/;",
        "const a = /(?<year>\\d{4})-(?<mo>\\d{2})/u;",
        "const a = /([a-z])(?:x)\\1/;",
        "const a = /(?:(a)|(b))\\2/u;",
        "const a = /x{2,3}(y)\\1/u;",
        "const a = /(a)\\1{2,}/u;",
        "const a = /(a)[\\d]\\1/u;",
    ] {
        assert!(
            regex_codes(source).is_empty(),
            "expected no diagnostics for {source}, got {:?}",
            regex_codes(source)
        );
    }
}
