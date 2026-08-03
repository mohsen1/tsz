//! Unit tests for the regular-expression grammar band of
//! `is_non_suppressing_parse_error`, plus a tripwire that keeps the band from
//! silently reopening.
//!
//! tsc never puts a regex grammar diagnostic in `parseDiagnostics`: its regex
//! validation runs from the checker, which re-scans the literal through
//! `scanner.scanRange`, so a malformed pattern cannot participate in
//! `hasParseDiagnostics()` suppression. tsz validates the pattern in
//! `crates/tsz-parser/src/parser/state_expressions_literals_regex.rs` during
//! parsing instead, which puts the same diagnostics in `parse_diagnostics` —
//! where they set `has_syntax_parse_errors` and delete unrelated real
//! diagnostics from the whole file.
//!
//! Every code asserted below was pinned against `typescript@7.0.2` with a
//! fixture pairing the regex literal with TS1039, TS2304, TS2322 and TS2339.
//! tsc reports all four companions in every case; the same fixture with a
//! genuine structural error (`const broken = ;`, TS1109) drops all four.

use super::*;

/// Codes emitted by `state_expressions_literals_regex.rs` that are unique to
/// the regex grammar walk, each with the oracle witness it was pinned on.
///
/// Codes that walk shares with non-regex contexts (TS1005, TS1125, TS1161,
/// TS1198) are excluded on purpose: this predicate is keyed on the code, not
/// on the emitting site, and each of those is a real parse failure elsewhere.
const REGEX_GRAMMAR_CODES: &[(u32, &str)] = &[
    (1487, r"/[\0]/u"),
    (1499, "/a/q"),
    (1500, "/a/gg"),
    (1502, "/a/uv"),
    (1505, "/a{1,/u"),
    (1506, "/a{2,1}/"),
    (1507, "/{1}/u"),
    (1508, "/[a[b]]/u"),
    (1510, r"/\k/u"),
    (1512, r"/\c1/u"),
    (1516, r"/[a-\d]/u"),
    (1517, "/[b-a]/"),
    (1519, r"/[a&&\d--\w]/v"),
    (1520, "/[a--]/v"),
    (1522, "/[a!!b]/v"),
    (1523, r"/\p{=x}/u"),
    (1524, r"/\p{Foo=Bar}/u"),
    (1525, r"/\p{Script=}/u"),
    (1526, r"/\p{Script=NotAScript}/u"),
    (1527, r"/\p{}/u"),
    (1528, r"/\p{RGI_Emoji}/u"),
    (1529, r"/\p{NotAThing}/u"),
    (1530, r"/\p{L}/"),
    (1531, r"/\p/u"),
    (1533, r"/(a)\2/"),
    (1534, r"/\1/"),
    (1535, r"/\y/u"),
    (1536, r"/[\1]/u"),
    (1537, r"/[\8]/u"),
    (1538, r"/\u{61}/"),
];

#[test]
fn every_regex_grammar_code_is_non_suppressing() {
    for &(code, witness) in REGEX_GRAMMAR_CODES {
        assert!(
            is_non_suppressing_parse_error(code),
            "TS{code} ({witness}) is emitted by tsz's regex grammar walk into \
             parse_diagnostics, but tsc reports it from the checker and keeps \
             every companion diagnostic in the file. Without an entry in \
             is_non_suppressing_parse_error it sets has_syntax_parse_errors and \
             deletes unrelated TS1039/TS2304/TS2322/TS2339 from the whole file."
        );
    }
}

/// Tripwire. The regex validator's diagnostic surface must not grow without
/// someone deciding whether the new code suppresses.
///
/// This asserts on the set of `diagnostic_codes::` constants the validator
/// references, not on compiler behaviour, so it is a review gate rather than a
/// predicate: when it fails, oracle-probe the new code against
/// `typescript@7.0.2` and either add it to `is_non_suppressing_parse_error` and
/// to `REGEX_GRAMMAR_CODES` above, or record here why it suppresses.
#[test]
fn regex_validator_diagnostic_surface_is_audited() {
    const VALIDATOR_SOURCE: &str =
        include_str!("../../../../tsz-parser/src/parser/state_expressions_literals_regex.rs");

    /// Constants the validator shares with non-regex parse failures. These stay
    /// out of `is_non_suppressing_parse_error` because the code, not the site,
    /// is what the predicate keys on.
    const SHARED_WITH_REAL_PARSE_FAILURES: &[&str] = &[
        "EXPECTED",
        "HEXADECIMAL_DIGIT_EXPECTED",
        "UNTERMINATED_REGULAR_EXPRESSION_LITERAL",
        "AN_EXTENDED_UNICODE_ESCAPE_VALUE_MUST_BE_BETWEEN_0X0_AND_0X10FFFF_INCLUSIVE",
    ];

    /// Regex-only constants, each already audited against the oracle and
    /// carried by `is_non_suppressing_parse_error`.
    const AUDITED_REGEX_ONLY: &[&str] = &[
        "OCTAL_ESCAPE_SEQUENCES_ARE_NOT_ALLOWED_USE_THE_SYNTAX",
        "UNKNOWN_REGULAR_EXPRESSION_FLAG",
        "INCOMPLETE_QUANTIFIER_DIGIT_EXPECTED",
        "NUMBERS_OUT_OF_ORDER_IN_QUANTIFIER",
        "THERE_IS_NOTHING_AVAILABLE_FOR_REPETITION",
        "UNEXPECTED_DID_YOU_MEAN_TO_ESCAPE_IT_WITH_BACKSLASH",
        "K_MUST_BE_FOLLOWED_BY_A_CAPTURING_GROUP_NAME_ENCLOSED_IN_ANGLE_BRACKETS",
        "C_MUST_BE_FOLLOWED_BY_AN_ASCII_LETTER",
        "A_CHARACTER_CLASS_RANGE_MUST_NOT_BE_BOUNDED_BY_ANOTHER_CHARACTER_CLASS",
        "A_CHARACTER_CLASS_MUST_NOT_CONTAIN_A_RESERVED_DOUBLE_PUNCTUATOR_DID_YOU_MEAN_TO",
        "OPERATORS_MUST_NOT_BE_MIXED_WITHIN_A_CHARACTER_CLASS_WRAP_IT_IN_A_NESTED_CLASS_I",
        "RANGE_OUT_OF_ORDER_IN_CHARACTER_CLASS",
        "EXPECTED_A_CLASS_SET_OPERAND",
        "EXPECTED_A_UNICODE_PROPERTY_NAME",
        "UNKNOWN_UNICODE_PROPERTY_NAME",
        "EXPECTED_A_UNICODE_PROPERTY_VALUE",
        "UNKNOWN_UNICODE_PROPERTY_VALUE",
        "EXPECTED_A_UNICODE_PROPERTY_NAME_OR_VALUE",
        "UNKNOWN_UNICODE_PROPERTY_NAME_OR_VALUE",
        "ANY_UNICODE_PROPERTY_THAT_WOULD_POSSIBLY_MATCH_MORE_THAN_A_SINGLE_CHARACTER_IS_O",
        "UNICODE_PROPERTY_VALUE_EXPRESSIONS_ARE_ONLY_AVAILABLE_WHEN_THE_UNICODE_U_FLAG_OR",
        "MUST_BE_FOLLOWED_BY_A_UNICODE_PROPERTY_VALUE_EXPRESSION_ENCLOSED_IN_BRACES",
        "THIS_BACKREFERENCE_REFERS_TO_A_GROUP_THAT_DOES_NOT_EXIST_THERE_ARE_ONLY_CAPTURIN",
        "THIS_BACKREFERENCE_REFERS_TO_A_GROUP_THAT_DOES_NOT_EXIST_THERE_ARE_NO_CAPTURING",
        "THIS_CHARACTER_CANNOT_BE_ESCAPED_IN_A_REGULAR_EXPRESSION",
        "OCTAL_ESCAPE_SEQUENCES_AND_BACKREFERENCES_ARE_NOT_ALLOWED_IN_A_CHARACTER_CLASS_I",
        "DECIMAL_ESCAPE_SEQUENCES_AND_BACKREFERENCES_ARE_NOT_ALLOWED_IN_A_CHARACTER_CLASS",
        "UNICODE_ESCAPE_SEQUENCES_ARE_ONLY_AVAILABLE_WHEN_THE_UNICODE_U_FLAG_OR_THE_UNICO",
    ];

    let mut referenced: Vec<&str> = VALIDATOR_SOURCE
        .match_indices("diagnostic_codes::")
        .map(|(at, marker)| {
            let rest = &VALIDATOR_SOURCE[at + marker.len()..];
            let end = rest
                .find(|c: char| !c.is_ascii_uppercase() && !c.is_ascii_digit() && c != '_')
                .unwrap_or(rest.len());
            &rest[..end]
        })
        .collect();
    referenced.sort_unstable();
    referenced.dedup();

    let unaudited: Vec<&str> = referenced
        .iter()
        .copied()
        .filter(|name| {
            !AUDITED_REGEX_ONLY.contains(name) && !SHARED_WITH_REAL_PARSE_FAILURES.contains(name)
        })
        .collect();

    assert!(
        unaudited.is_empty(),
        "state_expressions_literals_regex.rs emits diagnostic code constant(s) \
         {unaudited:?} that no one has classified. tsz emits these at PARSE time \
         but tsc emits the whole regex grammar family at CHECK time, so an \
         unclassified code silently suppresses every other diagnostic in any file \
         containing the offending literal. Probe it against typescript@7.0.2 with \
         a companion fixture (TS1039 + TS2304 + TS2322 + TS2339), then add it to \
         AUDITED_REGEX_ONLY and is_non_suppressing_parse_error, or to \
         SHARED_WITH_REAL_PARSE_FAILURES with the reason."
    );

    // Non-vacuity: the scan must actually find the band, not silently match zero.
    assert!(
        referenced.len() >= AUDITED_REGEX_ONLY.len(),
        "scan found only {} constants; the extraction is broken",
        referenced.len()
    );
}

/// A regex grammar diagnostic must not set `has_syntax_parse_errors`, while a
/// real structural error still must. This is the behaviour the band protects.
#[test]
fn regex_grammar_diagnostic_does_not_flag_syntax_parse_errors() {
    for &(code, witness) in REGEX_GRAMMAR_CODES {
        assert!(
            is_non_suppressing_parse_error(code),
            "TS{code} ({witness}) must not flag has_syntax_parse_errors"
        );
    }
    // Discriminating control: the codes tsc really does report from its parser
    // stay suppressing, so the band above is not just "everything passes".
    for code in [1005u32, 1109, 1125, 1128, 1161, 1198] {
        assert!(
            !is_non_suppressing_parse_error(code),
            "TS{code} is a real parse failure in tsc and must keep suppressing"
        );
    }
}
