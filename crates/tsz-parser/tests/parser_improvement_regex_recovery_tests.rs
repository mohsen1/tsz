//! Tests for parser improvements to reduce TS1005 and TS2300 false positives — regex recovery.

use crate::parser::test_fixture::parse_source;
use tsz_common::diagnostics::diagnostic_codes;
use tsz_common::position::LineMap;

#[test]
fn test_regex_extended_unicode_escape_without_u_or_v_reports_ts1538() {
    let source = r#"
const regexes: RegExp[] = [
  /\u{10000}[\u{10000}]/,
  /\u{10000}[\u{10000}]/u,
  /\u{10000}[\u{10000}]/v,
];
"#;
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let ts1538_count = diagnostics
        .iter()
        .filter(|d| {
            d.code
                == diagnostic_codes::UNICODE_ESCAPE_SEQUENCES_ARE_ONLY_AVAILABLE_WHEN_THE_UNICODE_U_FLAG_OR_THE_UNICO
        })
        .count();

    assert_eq!(
        ts1538_count, 2,
        "Expected exactly two TS1538 diagnostics for regexes without /u or /v, got {diagnostics:?}"
    );
}

#[test]
fn test_regex_extended_unicode_escape_above_max_does_not_report_ts1198() {
    // tsc treats out-of-range `\u{...}` inside regex literals as a runtime
    // concern and does not emit TS1198 even with the `u` flag. Match that
    // behavior — the parser must skip past the braced escape without
    // validating its code-point range.
    let source = r#"
const regexes: RegExp[] = [
  /\u{110000}/u,
  /[\u{110000}]/u,
];
"#;
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let ts1198: Vec<_> = diagnostics
        .iter()
        .filter(|d| {
            d.code
                == diagnostic_codes::AN_EXTENDED_UNICODE_ESCAPE_VALUE_MUST_BE_BETWEEN_0X0_AND_0X10FFFF_INCLUSIVE
        })
        .collect();

    assert!(
        ts1198.is_empty(),
        "Expected no TS1198 inside regex literals to match tsc, got {diagnostics:?}"
    );
}

#[test]
fn test_regex_character_class_range_order_reports_ts1517() {
    let source = r#"
const regexes: RegExp[] = [
  /[𝘈-𝘡][𝘡-𝘈]/,
  /[𝘈-𝘡][𝘡-𝘈]/u,
  /[𝘈-𝘡][𝘡-𝘈]/v,

  /[\u{1D608}-\u{1D621}][\u{1D621}-\u{1D608}]/,
  /[\u{1D608}-\u{1D621}][\u{1D621}-\u{1D608}]/u,
  /[\u{1D608}-\u{1D621}][\u{1D621}-\u{1D608}]/v,

  /[\uD835\uDE08-\uD835\uDE21][\uD835\uDE21-\uD835\uDE08]/,
  /[\uD835\uDE08-\uD835\uDE21][\uD835\uDE21-\uD835\uDE08]/u,
  /[\uD835\uDE08-\uD835\uDE21][\uD835\uDE21-\uD835\uDE08]/v,
];
"#;
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let ts1517_count = diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::RANGE_OUT_OF_ORDER_IN_CHARACTER_CLASS)
        .count();

    assert_eq!(
        ts1517_count, 11,
        "Expected exactly eleven TS1517 diagnostics for out-of-order regex ranges, got {diagnostics:?}"
    );
}

#[test]
fn test_regex_unicode_set_class_operators_follow_v_mode_rules() {
    let source = r#"
const q = /[\q{ab}]/v;
const sub = /[a--b]/v;
const missing = /[a&&]/v;
"#;
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let codes: Vec<_> = diagnostics.iter().map(|d| d.code).collect();

    assert!(
        !codes
            .contains(&diagnostic_codes::THIS_CHARACTER_CANNOT_BE_ESCAPED_IN_A_REGULAR_EXPRESSION),
        "Expected valid v-mode \\q string disjunction to avoid TS1535, got {diagnostics:?}"
    );
    assert!(
        !codes.contains(&diagnostic_codes::RANGE_OUT_OF_ORDER_IN_CHARACTER_CLASS),
        "Expected v-mode set subtraction to avoid legacy TS1517, got {diagnostics:?}"
    );

    let ts1520: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::EXPECTED_A_CLASS_SET_OPERAND)
        .collect();
    assert_eq!(
        ts1520.len(),
        1,
        "Expected exactly one TS1520 for the trailing intersection, got {diagnostics:?}"
    );
    let expected_start = source.rfind("]/v;").expect("trailing class close") as u32;
    assert_eq!(
        ts1520[0].start, expected_start,
        "Expected TS1520 at the missing operand before ']', got {diagnostics:?}"
    );
}

#[test]
fn test_regex_hyphen_after_range_is_literal() {
    let source = "const idSuffixPattern = /^([a-z][a-z0-9-]*)(:[a-z0-9-.]*)?$/i;";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    assert!(
        diagnostics
            .iter()
            .all(|d| d.code != diagnostic_codes::RANGE_OUT_OF_ORDER_IN_CHARACTER_CLASS),
        "Hyphen after an already-consumed range should be literal: {diagnostics:?}"
    );
}

#[test]
fn test_regex_hex_escape_range_start_does_not_report_ts1517() {
    let source = r"const pattern = /[\x2D-9A-Z\\_a-z\xF8-\u02C1]/u;";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    assert!(
        diagnostics
            .iter()
            .all(|d| d.code != diagnostic_codes::RANGE_OUT_OF_ORDER_IN_CHARACTER_CLASS),
        "Hex escapes should be decoded as one range atom before range-order checks: {diagnostics:?}"
    );
}

#[test]
fn test_unicode_regex_trailing_hyphen_class_does_not_report_ts1508() {
    let source = r#"
const unicode = /[a-]/u;
const unicode_sets = /[a-]/v;
"#;
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    assert!(
        diagnostics.iter().all(
            |d| d.code != diagnostic_codes::UNEXPECTED_DID_YOU_MEAN_TO_ESCAPE_IT_WITH_BACKSLASH
        ),
        "Trailing hyphen before a class close should be a literal, got {diagnostics:?}"
    );
}

#[test]
fn test_regex_character_class_escape_does_not_report_ts1517() {
    let source = r#"
/(#?-?\d*\.\d\w*%?)|(@?#?[\w-?]+%?)/g;
"#;
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    assert!(
        diagnostics
            .iter()
            .all(|d| d.code != diagnostic_codes::RANGE_OUT_OF_ORDER_IN_CHARACTER_CLASS),
        "Character class escapes like \\w should not trigger TS1517: {diagnostics:?}"
    );
}

#[test]
fn test_regex_annexb_p_escape_does_not_consume_following_escape() {
    // Annex B (no /u flag): `\P` without braces is the literal character `P`.
    // Previously, scan_character_class_escape returned None for this case
    // after advancing pos past `P`, causing the caller to over-consume the
    // following backslash. That mis-parsed `\P\w-_` as `P`, `w`, `-`, `_`
    // and then mis-detected `w-_` as an out-of-order range (TS1517).
    let source = "const a = /\\P[\\P\\w-_]/;\n";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    assert!(
        diagnostics
            .iter()
            .all(|d| d.code != diagnostic_codes::RANGE_OUT_OF_ORDER_IN_CHARACTER_CLASS),
        "Annex B `\\P` should not cause TS1517 on following character class atoms: {diagnostics:?}"
    );
}

#[test]
fn test_regex_non_bmp_inline_flags_emit_unknown_flag_diagnostics() {
    let source = r"
const 𝘳𝘦𝘨𝘦𝘹 = /(?𝘴𝘪-𝘮:^𝘧𝘰𝘰.)/𝘨𝘮𝘶;
";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let ts1499_count = diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::UNKNOWN_REGULAR_EXPRESSION_FLAG)
        .count();

    assert_eq!(
        ts1499_count, 6,
        "Expected six TS1499 diagnostics for unknown inline and trailing non-BMP flags, got {diagnostics:?}"
    );
}

#[test]
fn test_regex_missing_parenthesis_reports_ts1005_at_regex_end() {
    let source = "// @target: es2015\nvar x = /fo(o/;";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let expected_pos = source.rfind('/').expect("unterminated regex slash") as u32;
    let ts1005 = diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::EXPECTED && d.message == "')' expected.")
        .collect::<Vec<_>>();

    assert_eq!(
        ts1005.len(),
        1,
        "Expected exactly one missing ')' diagnostic: {diagnostics:?}"
    );
    assert_eq!(ts1005[0].start, expected_pos);
}

#[test]
fn test_unterminated_regex_class_suppresses_missing_bracket() {
    let source = "let r = /[a/;\n";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();
    let slash_pos = source.find('/').expect("regex slash") as u32;

    assert!(
        diagnostics.iter().any(|d| {
            d.code == diagnostic_codes::UNTERMINATED_REGULAR_EXPRESSION_LITERAL
                && d.start == slash_pos
        }),
        "expected TS1161 at regex slash, got {diagnostics:?}"
    );
    assert!(
        !diagnostics
            .iter()
            .any(|d| d.code == diagnostic_codes::EXPECTED && d.message == "']' expected."),
        "unterminated regex class should not also emit missing bracket diagnostic, got {codes:?}: {diagnostics:?}"
    );
}

#[test]
fn test_unterminated_regex_with_angle_text_reports_ts1161() {
    for source in ["const r = /<x>;\n", "const r = /a<x>;\n"] {
        let (parser, _root) = parse_source(source);

        let diagnostics = parser.get_diagnostics();
        let ts1161 = diagnostics
            .iter()
            .filter(|d| d.code == diagnostic_codes::UNTERMINATED_REGULAR_EXPRESSION_LITERAL)
            .collect::<Vec<_>>();

        assert_eq!(
            ts1161.len(),
            1,
            "Expected one TS1161 for ordinary regex angle text in {source:?}, got {diagnostics:?}"
        );
        assert_eq!(ts1161[0].start, source.find('/').unwrap() as u32);
    }
}

#[test]
fn test_regex_annex_b_diagnostic_positions_match_tsc() {
    let source = r#"
const regexes: RegExp[] = [
  /\q\u\i\c\k\_\f\o\x\-\j\u\m\p\s/,
  /[\q\u\i\c\k\_\f\o\x\-\j\u\m\p\s]/,
  /\P[\P\w-_]/,

  // Compare to
  /\q\u\i\c\k\_\f\o\x\-\j\u\m\p\s/u,
  /[\q\u\i\c\k\_\f\o\x\-\j\u\m\p\s]/u,
  /\P[\P\w-_]/u,
];

const regexesWithBraces: RegExp[] = [
  /{??/,
  /{,??/,
  /{,1??/,
  /{1??/,
  /{1,??/,
  /{1,2??/,
  /{2,1??/,
  /{}??/,
  /{,}??/,
  /{,1}??/,
  /{1}??/,
  /{1,}??/,
  /{1,2}??/,
  /{2,1}??/,

  // Compare to
  /{??/u,
  /{,??/u,
  /{,1??/u,
  /{1??/u,
  /{1,??/u,
  /{1,2??/u,
  /{2,1??/u,
  /{}??/u,
  /{,}??/u,
  /{,1}??/u,
  /{1}??/u,
  /{1,}??/u,
  /{1,2}??/u,
  /{2,1}??/u,
];
"#;
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let line_map = LineMap::build(source);

    let mut fingerprints: Vec<(u32, u32, u32, String)> = diagnostics
        .iter()
        .filter(|d| {
            matches!(
                d.code,
                diagnostic_codes::EXPECTED
                    | diagnostic_codes::INCOMPLETE_QUANTIFIER_DIGIT_EXPECTED
                    | diagnostic_codes::NUMBERS_OUT_OF_ORDER_IN_QUANTIFIER
                    | diagnostic_codes::THERE_IS_NOTHING_AVAILABLE_FOR_REPETITION
                    | diagnostic_codes::THIS_CHARACTER_CANNOT_BE_ESCAPED_IN_A_REGULAR_EXPRESSION
            )
        })
        .map(|d| {
            let pos = line_map.offset_to_position(d.start, source);
            (d.code, pos.line + 1, pos.character + 1, d.message.clone())
        })
        .collect();
    fingerprints.sort();

    let mut expected = vec![
        (diagnostic_codes::EXPECTED, 32, 7, "'}' expected."),
        (diagnostic_codes::EXPECTED, 33, 6, "'}' expected."),
        (diagnostic_codes::EXPECTED, 34, 7, "'}' expected."),
        (diagnostic_codes::EXPECTED, 35, 8, "'}' expected."),
        (diagnostic_codes::EXPECTED, 36, 8, "'}' expected."),
        (
            diagnostic_codes::INCOMPLETE_QUANTIFIER_DIGIT_EXPECTED,
            32,
            5,
            "Incomplete quantifier. Digit expected.",
        ),
        (
            diagnostic_codes::INCOMPLETE_QUANTIFIER_DIGIT_EXPECTED,
            38,
            5,
            "Incomplete quantifier. Digit expected.",
        ),
        (
            diagnostic_codes::INCOMPLETE_QUANTIFIER_DIGIT_EXPECTED,
            39,
            5,
            "Incomplete quantifier. Digit expected.",
        ),
        (
            diagnostic_codes::NUMBERS_OUT_OF_ORDER_IN_QUANTIFIER,
            27,
            5,
            "Numbers out of order in quantifier.",
        ),
        (
            diagnostic_codes::NUMBERS_OUT_OF_ORDER_IN_QUANTIFIER,
            36,
            5,
            "Numbers out of order in quantifier.",
        ),
        (
            diagnostic_codes::NUMBERS_OUT_OF_ORDER_IN_QUANTIFIER,
            43,
            5,
            "Numbers out of order in quantifier.",
        ),
    ];

    for (line, column) in [
        (24, 4),
        (24, 8),
        (25, 4),
        (25, 9),
        (26, 4),
        (26, 10),
        (27, 4),
        (27, 10),
        (32, 4),
        (32, 8),
        (33, 4),
        (33, 7),
        (34, 4),
        (34, 8),
        (35, 4),
        (35, 9),
        (36, 4),
        (36, 9),
        (38, 4),
        (38, 8),
        (39, 4),
        (39, 9),
        (40, 4),
        (40, 8),
        (41, 4),
        (41, 9),
        (42, 4),
        (42, 10),
        (43, 4),
        (43, 10),
    ] {
        expected.push((
            diagnostic_codes::THERE_IS_NOTHING_AVAILABLE_FOR_REPETITION,
            line,
            column,
            "There is nothing available for repetition.",
        ));
    }

    for (line, column) in [
        (8, 4),
        (8, 14),
        (8, 18),
        (8, 24),
        (9, 5),
        (9, 13),
        (9, 15),
        (9, 19),
        (9, 25),
    ] {
        expected.push((
            diagnostic_codes::THIS_CHARACTER_CANNOT_BE_ESCAPED_IN_A_REGULAR_EXPRESSION,
            line,
            column,
            "This character cannot be escaped in a regular expression.",
        ));
    }

    let mut expected: Vec<(u32, u32, u32, String)> = expected
        .into_iter()
        .map(|(code, line, column, message)| (code, line, column, message.to_string()))
        .collect();
    expected.sort();

    assert_eq!(
        fingerprints, expected,
        "Annex B regex diagnostic positions should match tsc, got: {diagnostics:?}"
    );
}

#[test]
fn test_regex_named_capturing_groups_do_not_emit_unexpected_paren() {
    let source = r#"const re = /(?<year>\d{4})-(?<month>\d{2})-(?<day>\d{2})/u;"#;
    let (parser, _root) = parse_source(source);
    let diagnostics = parser.get_diagnostics();
    let ts1508: Vec<_> = diagnostics.iter().filter(|d| d.code == 1508).collect();
    assert!(
        ts1508.is_empty(),
        "Expected valid named capturing groups to avoid TS1508, got {diagnostics:?}"
    );
}

#[test]
fn test_regex_unicode_brace_escape_variants_do_not_emit_ts1125() {
    let source = r#"
const a = /\u{-DDDD}/gu;
const b = /\u{r}\u{n}\u{t}/gu;
const c = /\u{}/gu;
"#;
    let (parser, _root) = parse_source(source);
    let diagnostics = parser.get_diagnostics();
    let ts1125: Vec<_> = diagnostics.iter().filter(|d| d.code == 1125).collect();
    assert!(
        ts1125.is_empty(),
        "Expected brace-form regex unicode escapes to avoid TS1125, got {diagnostics:?}"
    );
}

#[test]
fn test_regex_hex_escape_with_numeric_separator_no_ts1125() {
    // Regression for conformance test
    // `conformance/parser/ecmascript2021/numericSeparators/parser.numericSeparators.unicodeEscape.ts`:
    // tsc accepts `_` as a numeric-separator placeholder inside regex `\x` and
    // `\u` escapes (deferring strict hex grammar to the regex runtime), and
    // emits NO TS1125 for `/\xf_f/u` or `/\u_ffff/u`. We previously rejected
    // `_` at every hex-digit slot in the parser-level regex escape validator.
    let source = "/\\xf_f/u\n/\\uff_ff/u\n/\\u_ffff/u\n";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    assert!(
        !diagnostics
            .iter()
            .any(|d| d.code == diagnostic_codes::HEXADECIMAL_DIGIT_EXPECTED),
        "regex `\\x`/`\\u` escapes with `_` separator must not emit TS1125, got {diagnostics:?}"
    );
}

#[test]
fn test_regex_hex_escape_keeps_real_hex_digit_validation() {
    // Sanity guard: `_` relaxation must not silence genuine non-hex chars.
    // For `/\u\i\c/` the `\u` is followed by `\` (not hex, not `_`), so TS1125
    // must still fire — matching tsc's `regularExpressionAnnexB.ts`.
    let source = "/\\u\\i\\c/\n";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == diagnostic_codes::HEXADECIMAL_DIGIT_EXPECTED),
        "regex `\\u\\i...` must still emit TS1125 for non-hex non-separator chars, got {diagnostics:?}"
    );
}
