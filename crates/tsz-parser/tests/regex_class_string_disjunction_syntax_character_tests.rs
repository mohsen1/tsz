//! Interior `ClassSetCharacter` validation for a `v`-mode `\q{...}`
//! `ClassStringDisjunction` (TS1508 / TS1522).
//!
//! Follow-up to `regex_class_set_string_disjunction_operand_tests.rs`, which
//! covers the operand's own grammar (`\q` with/without braces). This file
//! covers what was previously unvalidated: the bytes *inside* the braces.
//!
//! `tsc` walks each `|`-separated `ClassString` alternative as a sequence of
//! `ClassSetCharacter`s, same as the enclosing class body, with two context
//! differences: `|` is the alternative separator (legal, not reported), and
//! there is no range or class-set-operator grammar inside `\q{...}`, so `-`
//! is reserved in every position (not only as a fresh atom) and `&&`/`--`
//! are not exempted as operators — `&&` reports TS1522 same as any other
//! doubled reserved punctuator, while `--` is two individual TS1508s (`-`
//! itself is not in `ClassSetReservedDoublePunctuator`).
//!
//! Every expectation below is pinned against `typescript@7.0.2`
//! (`scripts/conformance/oracle.sh`, `--noEmit --strict --target es2024
//! --lib es2024`), the version `scripts/conformance/typescript-versions.json`
//! pairs with the current corpus.
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
// Every reserved `ClassSetSyntaxCharacter` reports TS1508 when unescaped
// ---------------------------------------------------------------------------

#[test]
fn every_reserved_syntax_character_reports_ts1508_inside_q_braces() {
    for ch in ['(', ')', '{', '/', '[', ']', '-'] {
        let source = format!("const a = /[\\q{{a{ch}b}}]/v;");
        assert_eq!(
            regex_codes(&source),
            vec![1508],
            "expected TS1508 for unescaped {ch:?} inside \\q{{...}}"
        );
    }
}

#[test]
fn reserved_syntax_character_offset_lands_on_the_character() {
    // `const a = /[\q{(}]/v;` — offset 15 (0-based) is `(`.
    assert_eq!(regex_codes_at("const a = /[\\q{(}]/v;"), vec![(1508, 15)]);
}

#[test]
fn bare_hyphen_reports_in_every_position_unlike_the_enclosing_class() {
    // Unlike the enclosing class body (where `-` is a legal range separator),
    // `\q{...}` has no range grammar, so every `-` is reserved.
    assert_eq!(regex_codes("const a = /[\\q{-}]/v;"), vec![1508]);
    assert_eq!(regex_codes("const a = /[\\q{a-b}]/v;"), vec![1508]);
    assert_eq!(regex_codes("const a = /[\\q{-a}]/v;"), vec![1508]);
    assert_eq!(regex_codes("const a = /[\\q{a-}]/v;"), vec![1508]);
}

// ---------------------------------------------------------------------------
// `\` escapes any of them; `|` is the separator, not a syntax character
// ---------------------------------------------------------------------------

#[test]
fn escaping_a_reserved_syntax_character_is_clean() {
    for ch in ['(', ')', '{', '/', '[', ']', '-'] {
        let source = format!("const a = /[\\q{{\\{ch}}}]/v;");
        assert_eq!(
            regex_codes(&source),
            Vec::<u32>::new(),
            "expected no diagnostic for escaped {ch:?} inside \\q{{...}}"
        );
    }
}

#[test]
fn pipe_separates_alternatives_without_reporting() {
    assert_eq!(
        regex_codes("const a = /[\\q{a|b|ab}]/v;"),
        Vec::<u32>::new()
    );
    assert_eq!(regex_codes("const a = /[\\q{\\|}]/v;"), Vec::<u32>::new());
}

// ---------------------------------------------------------------------------
// `&&` is a reserved double punctuator here — unlike the enclosing class,
// where it is the intersection operator instead
// ---------------------------------------------------------------------------

#[test]
fn doubled_ampersand_reports_ts1522_inside_q_braces_unlike_the_enclosing_class() {
    assert_eq!(regex_codes("const a = /[\\q{&&}]/v;"), vec![1522]);
    // A single `&` is not reserved.
    assert_eq!(regex_codes("const a = /[\\q{&}]/v;"), Vec::<u32>::new());
}

#[test]
fn doubled_hyphen_is_two_ts1508_not_one_ts1522() {
    // `-` is not in `ClassSetReservedDoublePunctuator`, so a doubled `-`
    // draws one TS1508 per character rather than a merged TS1522 — unlike
    // the enclosing class body, where `--` is the subtraction operator and
    // reports neither.
    assert_eq!(regex_codes("const a = /[\\q{--}]/v;"), vec![1508, 1508]);
}

#[test]
fn other_reserved_double_punctuators_still_report_ts1522() {
    assert_eq!(regex_codes("const a = /[\\q{!!}]/v;"), vec![1522]);
}

#[test]
fn punctuators_outside_the_reserved_set_stay_clean_when_doubled() {
    for ch in ['$', '^'] {
        let source = format!("const a = /[\\q{{{ch}{ch}}}]/v;");
        assert_eq!(
            regex_codes(&source),
            Vec::<u32>::new(),
            "expected no diagnostic for doubled {ch:?} inside \\q{{...}}"
        );
    }
}

// ---------------------------------------------------------------------------
// Multiple alternatives / multiple occurrences
// ---------------------------------------------------------------------------

#[test]
fn only_the_reserved_character_in_a_multi_alternative_disjunction_reports() {
    // `ab` and clean; `c(d` has the one reserved character.
    assert_eq!(regex_codes("const a = /[\\q{ab|c(d}]/v;"), vec![1508]);
}

#[test]
fn multiple_reserved_characters_each_report() {
    assert_eq!(regex_codes("const a = /[\\q{(|)}]/v;"), vec![1508, 1508]);
}

// ---------------------------------------------------------------------------
// Interaction with negation: a lone malformed character is still exactly
// one code point, so it does not additionally draw TS1518
// ---------------------------------------------------------------------------

#[test]
fn negated_class_with_a_reserved_character_reports_only_ts1508() {
    assert_eq!(regex_codes("const a = /[^\\q{(}]/v;"), vec![1508]);
}

// ---------------------------------------------------------------------------
// Negative controls: unaffected outside `\q{...}`, and pre-existing clean
// shapes stay clean
// ---------------------------------------------------------------------------

#[test]
fn well_formed_disjunctions_stay_clean() {
    assert_eq!(regex_codes("const a = /[\\q{ab}]/v;"), Vec::<u32>::new());
    assert_eq!(regex_codes("const a = /[\\q{a}]/v;"), Vec::<u32>::new());
    assert_eq!(regex_codes("const a = /[\\q{}]/v;"), Vec::<u32>::new());
    assert_eq!(regex_codes("const a = /[\\q{ab|c}]/v;"), Vec::<u32>::new());
    assert_eq!(
        regex_codes("const a = /[\\q{ab}--\\q{a}]/v;"),
        Vec::<u32>::new()
    );
}

#[test]
fn reserved_syntax_character_check_is_scoped_to_q_braces() {
    // The same character bare in the surrounding class (not inside `\q{...}`)
    // is a separate, already-covered code path — not this one — and an
    // escaped `(` outside any string disjunction stays clean either way.
    assert_eq!(regex_codes("const a = /[\\(]/v;"), Vec::<u32>::new());
}
