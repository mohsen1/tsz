//! `ClassSetReservedDoublePunctuator` (TS1522) inside a `v`-flag character
//! class: a handful of ASCII punctuators are reserved when doubled, so a typo
//! like `[!!]` (meant to escape one of them) is caught instead of silently
//! matching two literal characters.
//!
//! `&&`/`--` are excluded — those are the defined class-set operators
//! (intersection/subtraction), not reserved punctuators. The exact reserved
//! set was derived empirically against `typescript@7.0.2`
//! (`--noEmit --strict --target esnext`) by doubling every ASCII punctuation
//! character not already structural to a character class (`(`, `)`, `[`, `]`,
//! `{`, `}`, `/`, `-`, `\`, `|`): only `!`, `#`, `%`, `*`, `+`, `,`, `.`, `:`,
//! `;`, `<`, `=`, `>`, `?`, `@`, a backtick, and `~` report TS1522 when doubled.
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
// Every reserved punctuator reports when doubled, under `v` only
// ---------------------------------------------------------------------------

#[test]
fn every_reserved_double_punctuator_reports_ts1522_under_v() {
    for ch in [
        '!', '#', '%', '*', '+', ',', '.', ':', ';', '<', '=', '>', '?', '@', '`', '~',
    ] {
        let source = format!("const a = /[a{ch}{ch}b]/v;");
        assert_eq!(
            regex_codes(&source),
            vec![1522],
            "expected TS1522 for doubled {ch:?}"
        );
    }
}

#[test]
fn reserved_double_punctuator_offset_lands_on_the_first_character() {
    // `const a = /[a!!b]/v;` — offset 13 is the first `!`.
    assert_eq!(regex_codes_at("const a = /[a!!b]/v;"), vec![(1522, 13)]);
}

// ---------------------------------------------------------------------------
// Negative controls: class-set operators are not reserved punctuators
// ---------------------------------------------------------------------------

#[test]
fn intersection_and_subtraction_operators_are_not_reserved_double_punctuators() {
    assert_eq!(regex_codes("const a = /[a&&b]/v;"), Vec::<u32>::new());
    assert_eq!(regex_codes("const a = /[a--b]/v;"), Vec::<u32>::new());
}

// ---------------------------------------------------------------------------
// Negative controls: not every doubled ASCII punctuator is reserved
// ---------------------------------------------------------------------------

#[test]
fn punctuators_outside_the_reserved_set_stay_clean_when_doubled() {
    for ch in ['$', '^', '"', '\'', '_'] {
        let source = format!("const a = /[x{ch}{ch}y]/v;");
        assert_eq!(
            regex_codes(&source),
            Vec::<u32>::new(),
            "expected no diagnostic for doubled {ch:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// The rule is `v`-only: `u` and no-flag both leave doubled punctuators alone
// ---------------------------------------------------------------------------

#[test]
fn reserved_double_punctuator_check_is_v_only() {
    assert_eq!(regex_codes("const a = /[a!!b]/u;"), Vec::<u32>::new());
    assert_eq!(regex_codes("const a = /[a!!b]/;"), Vec::<u32>::new());
}

// ---------------------------------------------------------------------------
// Multiple occurrences and odd counts
// ---------------------------------------------------------------------------

#[test]
fn multiple_occurrences_each_report() {
    assert_eq!(regex_codes("const a = /[a!!b!!c]/v;"), vec![1522, 1522]);
}

/// Three in a row consumes the first pair as the reserved double and leaves
/// the third as an ordinary literal atom — matching tsc's own recovery, which
/// reports the construct exactly once rather than once per adjacent pair.
#[test]
fn three_in_a_row_reports_once() {
    assert_eq!(regex_codes("const a = /[a!!!b]/v;"), vec![1522]);
}

// ---------------------------------------------------------------------------
// Interaction with negation and nesting
// ---------------------------------------------------------------------------

#[test]
fn reserved_double_punctuator_inside_a_negated_class_reports() {
    assert_eq!(regex_codes("const a = /[^a!!b]/v;"), vec![1522]);
}

#[test]
fn reserved_double_punctuator_inside_a_nested_class_reports() {
    assert_eq!(regex_codes("const a = /[[a!!b]]/v;"), vec![1522]);
}

// ---------------------------------------------------------------------------
// A lone reserved punctuator (not doubled) is unaffected
// ---------------------------------------------------------------------------

#[test]
fn a_single_reserved_punctuator_is_clean() {
    assert_eq!(regex_codes("const a = /[a!b]/v;"), Vec::<u32>::new());
}
