//! A bare, unescaped `ClassSetSyntaxCharacter` in a `v`-mode character class
//! (TS1508).
//!
//! ECMAScript's `ClassSetCharacter` production forbids the unescaped
//! `ClassSetSyntaxCharacter`s `( ) [ ] { } / - \ |`. Four of those are claimed
//! by the surrounding grammar and so are legal in place: `[` opens a nested
//! class, `]` terminates the class, `\` begins an escape, and `-` separates a
//! range. The remaining `( ) { } / |` — and a `-` that is *not* a range
//! separator (covered by `regex_class_set_bare_hyphen_tests`) — have no meaning
//! as a class-set atom, so `tsc` reports TS1508 ("Unexpected '{0}'. Did you
//! mean to escape it with backslash?") and recovers by treating the character
//! as a literal.
//!
//! The report is anchored on the offending character and fires once per
//! occurrence, in class-set, nested-class, negated-class and range-bound
//! position alike. Every row is pinned against `typescript@7.0.2`
//! (`--noEmit --strict --pretty false --target es2024 --lib es2024`), on code
//! *and* column. `tsc` renders 1-based columns; this harness reports zero-based
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
// Each syntax character, alone as the class's only content. Offset 12 is the
// character (`const a = /[` is 12 bytes).
// ---------------------------------------------------------------------------

#[test]
fn each_lone_syntax_character_reports() {
    assert_eq!(regex_codes_at("const a = /[(]/v;"), vec![(1508, 12)]);
    assert_eq!(regex_codes_at("const a = /[)]/v;"), vec![(1508, 12)]);
    assert_eq!(regex_codes_at("const a = /[{]/v;"), vec![(1508, 12)]);
    assert_eq!(regex_codes_at("const a = /[}]/v;"), vec![(1508, 12)]);
    assert_eq!(regex_codes_at("const a = /[|]/v;"), vec![(1508, 12)]);
    assert_eq!(regex_codes_at("const a = /[/]/v;"), vec![(1508, 12)]);
}

/// A syntax character between ordinary atoms reports at its own offset, and the
/// atoms around it stay clean.
#[test]
fn a_syntax_character_among_atoms_reports_at_its_offset() {
    // `/[a(b]/v` — offset 13 is the `(`.
    assert_eq!(regex_codes_at("const a = /[a(b]/v;"), vec![(1508, 13)]);
    // `/[a|b]/v` — the `|` is not a class-set operator, so it is a bare atom.
    assert_eq!(regex_codes_at("const a = /[a|b]/v;"), vec![(1508, 13)]);
}

/// Two occurrences in one class each report independently.
#[test]
fn two_syntax_characters_both_report() {
    // `/[a(b(c]/v` — the two `(` at offsets 13 and 15.
    assert_eq!(
        regex_codes_at("const a = /[a(b(c]/v;"),
        vec![(1508, 13), (1508, 15)]
    );
}

// ---------------------------------------------------------------------------
// Every class position the operand scan reaches: nested class, negated class,
// range bound, and around a class-set operator.
// ---------------------------------------------------------------------------

/// A nested class is its own scope; the report lands at the inner offset.
#[test]
fn a_nested_class_reports_at_its_own_offset() {
    // `/[[(]]/v` — offset 13 is the `(` inside the nested `[...]`.
    assert_eq!(regex_codes_at("const a = /[[(]]/v;"), vec![(1508, 13)]);
}

/// The leading `^` shifts every offset by one; an off-by-one in the report
/// position is otherwise invisible.
#[test]
fn a_negated_class_reports_at_the_shifted_offset() {
    // `/[^(]/v` — offset 13 is the `(` after the negation `^`.
    assert_eq!(regex_codes_at("const a = /[^(]/v;"), vec![(1508, 13)]);
}

/// A syntax character as a range's upper bound draws TS1508 in addition to the
/// range-order diagnostic the bound itself provokes.
#[test]
fn a_range_upper_bound_reports_alongside_range_order() {
    // `/[a-(]/v` — `a`..`(` is out of order (TS1517 at 12), and the `(` bound
    // is itself an illegal syntax character (TS1508 at 14).
    assert_eq!(
        regex_codes_at("const a = /[a-(]/v;"),
        vec![(1517, 12), (1508, 14)]
    );
}

/// A syntax character as a range's lower bound reports TS1508; here the range
/// `(`..`a` is in order, so no TS1517 accompanies it.
#[test]
fn a_range_lower_bound_reports_without_range_order() {
    // `/[(-a]/v` — offset 12 is the `(`.
    assert_eq!(regex_codes_at("const a = /[(-a]/v;"), vec![(1508, 12)]);
}

/// Operands on either side of a class-set operator (`&&`, `--`) each report.
#[test]
fn operands_around_a_class_set_operator_report() {
    // `/[(&&)]/v` — the `(` operand at 12 and the `)` operand at 15.
    assert_eq!(
        regex_codes_at("const a = /[(&&)]/v;"),
        vec![(1508, 12), (1508, 15)]
    );
    // `/[(--)]/v` — the subtraction operator, same operand positions.
    assert_eq!(
        regex_codes_at("const a = /[(--)]/v;"),
        vec![(1508, 12), (1508, 15)]
    );
}

// ---------------------------------------------------------------------------
// Negative controls
// ---------------------------------------------------------------------------

/// An escaped syntax character is a `ClassEscape`, not a bare
/// `ClassSetCharacter`, and is unaffected.
#[test]
fn an_escaped_syntax_character_is_clean() {
    assert_eq!(regex_codes("const a = /[\\(]/v;"), Vec::<u32>::new());
    assert_eq!(regex_codes("const a = /[\\)]/v;"), Vec::<u32>::new());
    assert_eq!(regex_codes("const a = /[\\{]/v;"), Vec::<u32>::new());
    assert_eq!(regex_codes("const a = /[\\|]/v;"), Vec::<u32>::new());
    assert_eq!(regex_codes("const a = /[\\/]/v;"), Vec::<u32>::new());
}

/// `[` opens a nested class and `]` terminates the class, so neither is a bare
/// syntax character; the surrounding productions consume them.
#[test]
fn nested_class_open_and_class_terminator_are_not_ts1508() {
    // `/[[a]b]/v` — the inner `[a]` is a nested class, the outer `]` closes.
    assert_eq!(regex_codes("const a = /[[a]b]/v;"), Vec::<u32>::new());
}

/// TS1508 for these characters is a `v`-only diagnostic: under `u` and under
/// Annex B they are ordinary class content.
#[test]
fn syntax_characters_are_v_only() {
    for pattern in [
        "const a = /[(]/u;",
        "const a = /[(]/;",
        "const a = /[)]/u;",
        "const a = /[{]/;",
        "const a = /[|]/u;",
        "const a = /[a|b]/;",
        "const a = /[/]/u;",
    ] {
        assert_eq!(regex_codes(pattern), Vec::<u32>::new(), "{pattern}");
    }
}

/// Sibling top-level classes do not share state.
#[test]
fn sibling_classes_each_report_their_own() {
    // `/[(][)]/v` — the `(` at 12 and the `)` at 15.
    assert_eq!(
        regex_codes_at("const a = /[(][)]/v;"),
        vec![(1508, 12), (1508, 15)]
    );
}
