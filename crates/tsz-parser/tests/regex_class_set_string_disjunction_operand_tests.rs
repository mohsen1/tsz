//! `\q` as a class-set operand under the Unicode Sets (`v`) flag.
//!
//! `ClassSetOperand ::= '\q{' ClassStringDisjunctionContents '}'` — a `\q`
//! that is not followed by `{` is not a string disjunction at all. `tsc`
//! reports TS1521 at the backslash and then treats the operand as the single
//! character `q`, so no class-bounded-range diagnostic follows it.
//!
//! Every expectation below is pinned against `typescript@7.0.2`
//! (`tsc --noEmit --strict --target esnext`), the version
//! `scripts/conformance/typescript-versions.json` pairs with the current
//! corpus.

use crate::parser::test_fixture::parse_source;
use tsz_common::diagnostics::diagnostic_codes;
use tsz_common::position::LineMap;

/// Collects every parser diagnostic as `(line, column, code)`, 1-based, in
/// source order — the same fingerprint shape `tsc`'s CLI prints.
fn diagnostic_fingerprints(source: &str) -> Vec<(u32, u32, u32)> {
    let (parser, _root) = parse_source(source);
    let line_map = LineMap::build(source);

    let mut fingerprints: Vec<(u32, u32, u32)> = parser
        .get_diagnostics()
        .iter()
        .map(|d| {
            let pos = line_map.offset_to_position(d.start, source);
            (pos.line + 1, pos.character + 1, d.code)
        })
        .collect();
    fingerprints.sort_unstable();
    fingerprints
}

#[test]
fn test_class_set_q_operand_without_braces_reports_ts1521() {
    // Column 5 on each line is the backslash of `\q`: two spaces of indent,
    // `/` at 3, `[` at 4, `\` at 5.
    let source = r"
const regexes: RegExp[] = [
  /[\q]/v,
  /[\qa]/v,
];
";
    let expected: Vec<(u32, u32, u32)> = vec![
        (
            3,
            5,
            diagnostic_codes::Q_MUST_BE_FOLLOWED_BY_STRING_ALTERNATIVES_ENCLOSED_IN_BRACES,
        ),
        (
            4,
            5,
            diagnostic_codes::Q_MUST_BE_FOLLOWED_BY_STRING_ALTERNATIVES_ENCLOSED_IN_BRACES,
        ),
    ];

    assert_eq!(
        diagnostic_fingerprints(source),
        expected,
        "a `\\q` not followed by `{{` is TS1521 at the backslash"
    );
}

#[test]
fn test_class_set_q_operand_with_braces_is_clean() {
    // A well-formed string disjunction — including the single-character form,
    // which cannot match a string and so is legal inside a negated class.
    let source = r"
const regexes: RegExp[] = [
  /[\q{ab}]/v,
  /[\q{a}]/v,
  /[\q{}]/v,
  /[\q{ab|c}]/v,
  /[\q{ab}--\q{a}]/v,
  /[^\q{a}]/v,
];
";
    assert_eq!(
        diagnostic_fingerprints(source),
        Vec::new(),
        "`\\q{{...}}` is a well-formed class-set operand"
    );
}

#[test]
fn test_class_set_q_operand_without_braces_in_every_operand_position() {
    let source = r"
const regexes: RegExp[] = [
  /[a\q]/v,
  /[^\q]/v,
  /[[\q]]/v,
  /[\q&&a]/v,
  /[\q\q]/v,
];
";
    const TS1521: u32 =
        diagnostic_codes::Q_MUST_BE_FOLLOWED_BY_STRING_ALTERNATIVES_ENCLOSED_IN_BRACES;
    let expected: Vec<(u32, u32, u32)> = vec![
        (3, 6, TS1521), // after a leading character
        (4, 6, TS1521), // inside a negated class
        (5, 6, TS1521), // inside a nested class
        (6, 5, TS1521), // left operand of an intersection
        (7, 5, TS1521), // both operands report independently
        (7, 7, TS1521),
    ];

    assert_eq!(
        diagnostic_fingerprints(source),
        expected,
        "TS1521 fires once per malformed `\\q`, wherever the operand appears"
    );
}

#[test]
fn test_class_set_q_operand_without_braces_is_a_single_character_operand() {
    // `tsc` returns the character `q` after reporting, so `\q` bounding a
    // range does NOT additionally draw TS1516 (a class-bounded range) — the
    // only diagnostic is TS1521. The `\q{...}` form is a genuine class and
    // does draw TS1516, which is what keeps the two apart.
    let source = r"
const regexes: RegExp[] = [
  /[\q-z]/v,
  /[\q{a}-z]/v,
];
";
    let expected: Vec<(u32, u32, u32)> = vec![
        (
            3,
            5,
            diagnostic_codes::Q_MUST_BE_FOLLOWED_BY_STRING_ALTERNATIVES_ENCLOSED_IN_BRACES,
        ),
        (
            4,
            5,
            diagnostic_codes::A_CHARACTER_CLASS_RANGE_MUST_NOT_BE_BOUNDED_BY_ANOTHER_CHARACTER_CLASS,
        ),
    ];

    assert_eq!(
        diagnostic_fingerprints(source),
        expected,
        "a reported `\\q` degrades to the single character `q`, not to a class"
    );
}

#[test]
fn test_class_set_q_operand_negative_cases_are_unchanged() {
    // `\q` is a class-set concept: it exists only under `v`, and only inside a
    // character class. Neither neighbouring shape may start reporting TS1521.
    let source = r"
const regexes: RegExp[] = [
  /[\q]/u,
  /[\q]/,
  /\q/v,
  /[^\q{ab}]/v,
];
";
    let expected: Vec<(u32, u32, u32)> = vec![
        (
            3,
            5,
            diagnostic_codes::THIS_CHARACTER_CANNOT_BE_ESCAPED_IN_A_REGULAR_EXPRESSION,
        ),
        // `/[\q]/` (no flag) is Annex B — `\q` is an identity escape, clean.
        (5, 4, diagnostic_codes::Q_IS_ONLY_AVAILABLE_INSIDE_CHARACTER_CLASS),
        (
            6,
            6,
            diagnostic_codes::ANYTHING_THAT_WOULD_POSSIBLY_MATCH_MORE_THAN_A_SINGLE_CHARACTER_IS_INVALID_INSID,
        ),
    ];

    assert_eq!(
        diagnostic_fingerprints(source),
        expected,
        "TS1521 is scoped to a class-set operand under the `v` flag"
    );
}
