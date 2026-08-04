//! Capturing-group name validation inside regular expression literals
//! (TS1514 / TS1515, plus the `'>' expected.` that closes the name).
//!
//! `(?<name>` and `\k<name>` share one name scan in tsc (`scanGroupName`).
//! It reports TS1514 when no identifier is present and — for a declaration
//! only — TS1515 when the name is already visible in the current alternative
//! or in an enclosing one. Sibling alternatives are mutually exclusive, so a
//! plain "have I seen this name" set over-reports; the scopes have to be
//! pushed and popped per alternative. Every row below is pinned against a real
//! `tsc` run at `--target esnext`.
use crate::parser::ParserState;
use crate::parser::test_fixture::parse_source;

fn codes(parser: &ParserState) -> Vec<u32> {
    parser.get_diagnostics().iter().map(|d| d.code).collect()
}

fn regex_codes(source: &str) -> Vec<u32> {
    let (parser, _root) = parse_source(source);
    codes(&parser)
}

/// The reported span matters: TS1515 points at the *second* occurrence of the
/// name, not at the group or at the first declaration.
fn regex_spans(source: &str) -> Vec<(u32, u32, u32)> {
    let (parser, _root) = parse_source(source);
    parser
        .get_diagnostics()
        .iter()
        .map(|d| (d.code, d.start, d.length))
        .collect()
}

// ---------------------------------------------------------------------------
// TS1514 — no capturing group name where one is required
// ---------------------------------------------------------------------------

#[test]
fn empty_declaration_group_name_reports_ts1514() {
    assert_eq!(regex_codes("const a = /(?<>x)/u;"), vec![1514]);
}

/// Neither code is gated on the Unicode flags; tsc validates group names in
/// Annex-B mode too.
#[test]
fn empty_declaration_group_name_reports_ts1514_without_unicode_flag() {
    assert_eq!(regex_codes("const a = /(?<>x)/;"), vec![1514]);
}

#[test]
fn empty_declaration_group_name_reports_ts1514_under_unicode_sets_flag() {
    assert_eq!(regex_codes("const a = /(?<>x)/v;"), vec![1514]);
}

#[test]
fn empty_backreference_group_name_reports_ts1514() {
    assert_eq!(regex_codes("const a = /\\k<>/u;"), vec![1514]);
}

/// A digit is an identifier *part* but not an identifier *start*, so nothing is
/// consumed and the name is empty. tsc's own same-position diagnostic dedup
/// then swallows the `'>' expected.` that would otherwise follow at the same
/// offset — one diagnostic, not two.
#[test]
fn digit_leading_group_name_reports_only_ts1514() {
    assert_eq!(regex_codes("const a = /(?<1a>x)/u;"), vec![1514]);
}

#[test]
fn ts1514_is_reported_at_the_name_position_with_zero_length() {
    assert_eq!(regex_spans("const a = /(?<>x)/u;"), vec![(1514, 14, 0)]);
}

// ---------------------------------------------------------------------------
// TS1514 negatives — names an identifier scan must accept
// ---------------------------------------------------------------------------

#[test]
fn dollar_leading_group_name_is_accepted() {
    assert_eq!(regex_codes("const a = /(?<$x>y)/u;"), Vec::<u32>::new());
}

#[test]
fn underscore_leading_group_name_is_accepted() {
    assert_eq!(regex_codes("const a = /(?<_x>y)/u;"), Vec::<u32>::new());
}

/// Group names are full ECMAScript identifiers, not `\w+`.
#[test]
fn non_ascii_group_name_is_accepted() {
    assert_eq!(regex_codes("const a = /(?<\u{e4}>y)/u;"), Vec::<u32>::new());
}

/// A `\u` escape is legal in continuation position, and tsc decodes it before
/// comparing names.
#[test]
fn unicode_escape_in_group_name_continuation_is_accepted() {
    assert_eq!(
        regex_codes("const a = /(?<a\\u0062>y)/u;"),
        Vec::<u32>::new()
    );
}

/// ...but not in *start* position, where `\` is simply not an identifier start.
#[test]
fn unicode_escape_at_group_name_start_reports_ts1514() {
    assert_eq!(regex_codes("const a = /(?<\\u0061>y)/u;"), vec![1514]);
}

// ---------------------------------------------------------------------------
// TS1515 — the same name twice in one alternative
// ---------------------------------------------------------------------------

#[test]
fn duplicate_group_name_in_one_alternative_reports_ts1515() {
    assert_eq!(regex_codes("const a = /(?<a>x)(?<a>y)/u;"), vec![1515]);
}

#[test]
fn duplicate_group_name_reports_ts1515_without_unicode_flag() {
    assert_eq!(regex_codes("const a = /(?<a>x)(?<a>y)/;"), vec![1515]);
}

/// Nesting the second declaration inside a non-capturing group does not make
/// the two mutually exclusive.
#[test]
fn duplicate_group_name_nested_in_a_non_capturing_group_reports_ts1515() {
    assert_eq!(regex_codes("const a = /(?<a>x)(?:(?<a>y))/u;"), vec![1515]);
}

/// An enclosing alternative's names stay visible inside the group it contains.
#[test]
fn group_name_shadowing_its_own_enclosing_group_reports_ts1515() {
    assert_eq!(regex_codes("const a = /(?<a>(?<a>x))/u;"), vec![1515]);
}

/// The enclosing scope is visible from *any* alternative of a nested
/// disjunction, not just the first.
#[test]
fn duplicate_group_name_in_a_nested_alternative_reports_ts1515() {
    assert_eq!(regex_codes("const a = /(?<a>x|(?<a>y))/u;"), vec![1515]);
}

/// Only the second of two same-named groups in one alternative is reported,
/// and it is reported against the name span, not the whole group.
#[test]
fn ts1515_is_reported_at_the_second_name_span() {
    assert_eq!(
        regex_spans("const a = /(?<a>x)(?<a>y)/u;"),
        vec![(1515, 21, 1)]
    );
}

/// Names compare by their decoded value, so an escaped spelling still collides.
#[test]
fn escaped_and_plain_spellings_of_one_name_report_ts1515() {
    assert_eq!(
        regex_codes("const a = /(?<a\\u0062>x)(?<ab>y)/u;"),
        vec![1515]
    );
}

#[test]
fn braced_unicode_escape_spelling_of_one_name_reports_ts1515() {
    assert_eq!(
        regex_codes("const a = /(?<a\\u{62}>x)(?<ab>y)/u;"),
        vec![1515]
    );
}

// ---------------------------------------------------------------------------
// TS1515 negatives — mutually exclusive alternatives, and non-declarations
// ---------------------------------------------------------------------------

/// The headline exemption: sibling alternatives can never both match, so they
/// may reuse a name.
#[test]
fn same_group_name_in_sibling_alternatives_is_accepted() {
    assert_eq!(
        regex_codes("const a = /(?<a>x)|(?<a>y)/u;"),
        Vec::<u32>::new()
    );
}

#[test]
fn same_group_name_across_three_alternatives_is_accepted() {
    assert_eq!(
        regex_codes("const a = /(?<a>x)|(?<a>y)|(?<a>z)/u;"),
        Vec::<u32>::new()
    );
}

#[test]
fn same_group_name_nested_in_a_sibling_alternative_is_accepted() {
    assert_eq!(
        regex_codes("const a = /(?<a>x)|(?:(?<a>y))/u;"),
        Vec::<u32>::new()
    );
}

/// A name reused within one alternative still reports even when an earlier
/// alternative is clean, so leaving an alternative must restore the enclosing
/// scope rather than drop everything.
#[test]
fn duplicate_group_name_in_the_second_alternative_reports_ts1515() {
    assert_eq!(
        regex_codes("const a = /(?<a>x)|(?<b>y)(?<b>z)/u;"),
        vec![1515]
    );
}

#[test]
fn distinct_group_names_are_accepted() {
    assert_eq!(
        regex_codes("const a = /(?<a>x)(?<b>y)/u;"),
        Vec::<u32>::new()
    );
}

/// Lookbehind assertions are not capturing groups and declare no name.
#[test]
fn lookbehind_assertions_declare_no_group_name() {
    assert_eq!(
        regex_codes("const a = /(?<=a)(?<b>x)/u;"),
        Vec::<u32>::new()
    );
    assert_eq!(
        regex_codes("const a = /(?<!a)(?<b>x)/u;"),
        Vec::<u32>::new()
    );
}

/// A back-reference mentions a name, it does not declare one.
#[test]
fn backreference_to_a_declared_name_is_accepted() {
    assert_eq!(
        regex_codes("const a = /(?<a>x)\\k<a>/u;"),
        Vec::<u32>::new()
    );
}

/// `(?<a>` inside a character class is literal text, so it neither declares a
/// name nor collides with one.
#[test]
fn group_syntax_inside_a_character_class_declares_no_name() {
    assert_eq!(
        regex_codes("const a = /(?<a>x)[(?<a>y)]/u;"),
        Vec::<u32>::new()
    );
}

// ---------------------------------------------------------------------------
// The closing `>`
// ---------------------------------------------------------------------------

/// tsc's `scanExpectedChar` follows every group-name scan. A name that does
/// not end in `>` gets `'>' expected.` at the offending character.
#[test]
fn declaration_group_name_without_a_closing_angle_bracket_reports_ts1005() {
    assert_eq!(regex_codes("const a = /(?<a b>x)/u;"), vec![1005]);
}

#[test]
fn backreference_group_name_without_a_closing_angle_bracket_reports_ts1005() {
    assert_eq!(regex_codes("const a = /(?<a>x)\\k<a/u;"), vec![1005]);
}
