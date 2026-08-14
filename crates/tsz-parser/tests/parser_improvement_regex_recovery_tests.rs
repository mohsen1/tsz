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
fn test_regex_extended_unicode_escape_above_max_reports_ts1198() {
    // Under the `u` flag, `\u{...}` is a code-point escape and tsc (7.0.2) emits
    // TS1198 when its value exceeds 0x10FFFF (conformance
    // `unicodeExtendedEscapesInRegularExpressions07/12`). The prior expectation
    // that regex escapes are never range-checked reflected a stale tsc 6.0 cache.
    // The character-class form (`/[\u{110000}]/u`) is validated by the full
    // regex-grammar validator (task #74), so only the top-level escape fires here.
    let source = r#"
const regexes: RegExp[] = [
  /\u{110000}/u,
  /[\u{110000}]/u,
];
"#;
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let ts1198 = diagnostics
        .iter()
        .filter(|d| {
            d.code
                == diagnostic_codes::AN_EXTENDED_UNICODE_ESCAPE_VALUE_MUST_BE_BETWEEN_0X0_AND_0X10FFFF_INCLUSIVE
        })
        .count();

    assert_eq!(
        ts1198, 1,
        "Expected TS1198 for the top-level out-of-range `\\u{{}}` escape, got {diagnostics:?}"
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
fn test_regex_q_escape_outside_character_class_reports_ts1511_under_v_flag() {
    // tsc: `\q` is a `v`-mode-only character-class atom (ECMA-262
    // `ClassSetCharacter :: \q{...}`). Used at atom position outside a class
    // under the `v` flag it is still reserved, but tsc names the specific
    // reason (TS1511) rather than the generic TS1535 fallback.
    let source = r"const outside = /\q{abc}/v;";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let codes: Vec<_> = diagnostics.iter().map(|d| d.code).collect();

    assert!(
        codes.contains(&diagnostic_codes::Q_IS_ONLY_AVAILABLE_INSIDE_CHARACTER_CLASS),
        "Expected TS1511 for \\q outside a character class under /v, got {diagnostics:?}"
    );
    assert!(
        !codes
            .contains(&diagnostic_codes::THIS_CHARACTER_CANNOT_BE_ESCAPED_IN_A_REGULAR_EXPRESSION),
        "TS1511 should replace the generic TS1535 fallback for this shape, got {diagnostics:?}"
    );
}

#[test]
fn test_regex_q_escape_stays_ts1535_without_v_flag_or_inside_class() {
    // Negative/adjacent matrix for the TS1511 fix above: only "atom
    // position + `v` flag" gets the specific code. Every other combination
    // must keep reporting the pre-existing generic TS1535 (or, inside a
    // `v`-mode class, no diagnostic at all — `\q{...}` is valid there).
    let source = r"
const uOnlyOutsideClass = /\q{abc}/u;
const noFlagOutsideClass = /\q/;
const uOnlyInsideClass = /[\q{abc}]/u;
const vModeInsideClass = /[\q{abc}]/v;
";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let codes: Vec<_> = diagnostics.iter().map(|d| d.code).collect();

    assert!(
        !codes.contains(&diagnostic_codes::Q_IS_ONLY_AVAILABLE_INSIDE_CHARACTER_CLASS),
        "TS1511 must not fire without /v at atom position or inside a class, got {diagnostics:?}"
    );
    let ts1535_count = diagnostics
        .iter()
        .filter(|d| {
            d.code == diagnostic_codes::THIS_CHARACTER_CANNOT_BE_ESCAPED_IN_A_REGULAR_EXPRESSION
        })
        .count();
    assert_eq!(
        ts1535_count, 2,
        "Expected TS1535 for the `u`-only outside-class use and the `u`-only \
         in-class use (no-flag outside-class and `v`-mode in-class are both \
         valid \\q shapes), got {diagnostics:?}"
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
fn test_regex_empty_braced_unicode_escape_reports_ts1125() {
    // `\u{}` under the `u` flag has no hex digits, so tsc emits TS1125
    // (conformance `unicodeExtendedEscapesInRegularExpressions19`). The malformed
    // non-empty forms (`\u{-DDDD}`, `\u{r}`) additionally require the TS1508
    // "Unexpected …" regex-grammar diagnostic (task #74) and are not yet reported,
    // so the empty form is the only TS1125 here.
    let source = r#"
const a = /\u{-DDDD}/gu;
const b = /\u{r}\u{n}\u{t}/gu;
const c = /\u{}/gu;
"#;
    let (parser, _root) = parse_source(source);
    let diagnostics = parser.get_diagnostics();
    let ts1125 = diagnostics.iter().filter(|d| d.code == 1125).count();
    assert_eq!(
        ts1125, 1,
        "Expected exactly one TS1125 (from the empty `\\u{{}}`), got {diagnostics:?}"
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

#[test]
fn test_regex_trailing_flag_scan_uses_es_identifier_part() {
    // tsc terminates regex-flag scanning with `isIdentifierPart`
    // (`scanner.ts`), so any ES `ID_Continue` code point after the flags is
    // consumed and reported as TS1499. U+00B7 (Other_ID_Continue) and U+309B
    // (Other_ID_Start, NFKC-unstable so absent from XID tables) are both
    // identifier parts; `char::is_alphabetic` rejected both.
    for (source, label) in [
        ("let r = /foo/g\u{00B7};\n", "U+00B7 MIDDLE DOT"),
        (
            "let r = /foo/\u{309B};\n",
            "U+309B KATAKANA-HIRAGANA VOICED SOUND MARK",
        ),
    ] {
        let (parser, _root) = parse_source(source);

        let diagnostics = parser.get_diagnostics();
        let ts1499_count = diagnostics
            .iter()
            .filter(|d| d.code == diagnostic_codes::UNKNOWN_REGULAR_EXPRESSION_FLAG)
            .count();

        assert_eq!(
            ts1499_count, 1,
            "{label} after regex flags must emit exactly one TS1499, got {diagnostics:?}"
        );
    }
}

#[test]
fn test_regex_escaped_backslash_does_not_seed_phantom_unicode_or_hex_escape() {
    // In a regex literal, `\\` is a single literal-backslash atom; the chars
    // after it are literals, not a `\u`/`\x` escape. The regex escape validator
    // previously did not pair backslashes, so the second `\` of `\\u005` seeded
    // a phantom `\u` validation -> false TS1125. These are clean under tsc.
    for source in [
        "const r = /\\\\u005[Ff]/;\n", // /\\u005[Ff]/
        "const r = /[\\\\u005]/;\n",   // /[\\u005]/
        "const r = /\\\\xA/;\n",       // /\\xA/
        // destr's proto/constructor guard regex shape (escaped backslashes).
        "const r = /\"(?:_|\\\\u005[Ff])(?:_|\\\\u005[Ff])(?:p|\\\\u0070)\"/;\n",
    ] {
        let (parser, _root) = parse_source(source);
        let diagnostics = parser.get_diagnostics();
        assert!(
            !diagnostics
                .iter()
                .any(|d| d.code == diagnostic_codes::HEXADECIMAL_DIGIT_EXPECTED),
            "escaped backslash `\\\\` must not seed a phantom `\\u`/`\\x` escape (TS1125) in {source:?}, got {diagnostics:?}"
        );
    }
}

#[test]
fn test_regex_plain_escapes_and_valid_escapes_stay_clean() {
    // Unchanged-clean controls: ordinary regex and valid escapes never emit
    // TS1125, before or after the backslash-pairing fix.
    for source in [
        "const r = /A/;\n",
        "const r = /\\u{1F600}/u;\n",
        "const r = /\\xAB/;\n",
    ] {
        let (parser, _root) = parse_source(source);
        let diagnostics = parser.get_diagnostics();
        assert!(
            !diagnostics
                .iter()
                .any(|d| d.code == diagnostic_codes::HEXADECIMAL_DIGIT_EXPECTED),
            "valid regex escape must stay clean of TS1125 in {source:?}, got {diagnostics:?}"
        );
    }
}

#[test]
fn test_regex_genuine_incomplete_escapes_still_report_ts1125() {
    // Genuine incomplete `\u`/`\x` escapes (unescaped backslash) must STILL
    // fire TS1125 after the backslash-pairing fix.
    for source in [
        "const r = /\\u00G1/;\n", // non-hex G at slot 3
        "const r = /\\u00/;\n",   // truncated \u
        "const r = /\\xZZ/;\n",   // non-hex Z after \x
    ] {
        let (parser, _root) = parse_source(source);
        let diagnostics = parser.get_diagnostics();
        assert!(
            diagnostics
                .iter()
                .any(|d| d.code == diagnostic_codes::HEXADECIMAL_DIGIT_EXPECTED),
            "genuine incomplete regex `\\u`/`\\x` escape must still emit TS1125 in {source:?}, got {diagnostics:?}"
        );
    }
}

#[test]
fn test_string_context_unicode_escape_validation_unchanged() {
    // The backslash-pairing fix is scoped to the regex escape loop only;
    // string-literal escapes are validated on a different path and stay
    // unchanged. `"\u005"` is a genuinely incomplete string `\u` escape.
    let source = "const s = \"\\u005\";\n";
    let (parser, _root) = parse_source(source);
    let diagnostics = parser.get_diagnostics();
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == diagnostic_codes::HEXADECIMAL_DIGIT_EXPECTED),
        "incomplete string `\\u` escape must still emit TS1125, got {diagnostics:?}"
    );
}

#[test]
fn test_regex_trailing_non_identifier_codepoint_ends_flag_scan() {
    // Negative guard: U+2117 SOUND RECORDING COPYRIGHT is not in ES
    // `ID_Continue`, so tsc ends the flag scan before it and never reports
    // TS1499 for it.
    let source = "let r = /foo/g\u{2117};\n";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    assert!(
        !diagnostics
            .iter()
            .any(|d| d.code == diagnostic_codes::UNKNOWN_REGULAR_EXPRESSION_FLAG),
        "non-ID_Continue code point after flags must not emit TS1499, got {diagnostics:?}"
    );
}

/// `\p{…}` / `\P{…}` Unicode property value expressions, pinned against
/// `typescript@7.0.2` (`--noEmit --strict --target esnext --module esnext
/// --lib esnext`). Every fingerprint below — including the two TS1508
/// follow-ons on the malformed tails, and the clean rows that must stay
/// silent — was taken from an oracle run over this exact source.
#[test]
fn test_regex_unicode_property_escape_diagnostics_match_tsc() {
    let source = r#"
const unicodePropertyEscapes: RegExp[] = [
  /\p{L}/u,
  /\p{Script=Latin}/u,
  /\p{General_Category=Letter}/u,
  /\p{}/u,
  /\P{}/u,
  /\p{=Letter}/u,
  /\p{Script=}/u,
  /\p{=}/u,
  /\p{RGI_Emoji}/u,
  /\p{Basic_Emoji}/u,
  /[\p{RGI_Emoji}]/u,
  /\p{L}/,
  /\P{L}/,
  /\p{}/,
  /\p{RGI_Emoji}/,
  /a\p{L}b/,
  /\p{Ω}/u,
  /\p{ Script=Latin }/u,
];
"#;
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let line_map = LineMap::build(source);

    let mut actual: Vec<(u32, u32, u32)> = diagnostics
        .iter()
        .map(|d| {
            let pos = line_map.offset_to_position(d.start, source);
            (pos.line + 1, pos.character + 1, d.code)
        })
        .collect();
    actual.sort_unstable();

    let expected: Vec<(u32, u32, u32)> = vec![
        (6, 7, diagnostic_codes::EXPECTED_A_UNICODE_PROPERTY_NAME_OR_VALUE),
        (7, 7, diagnostic_codes::EXPECTED_A_UNICODE_PROPERTY_NAME_OR_VALUE),
        (8, 7, diagnostic_codes::EXPECTED_A_UNICODE_PROPERTY_NAME),
        (9, 14, diagnostic_codes::EXPECTED_A_UNICODE_PROPERTY_VALUE),
        (10, 7, diagnostic_codes::EXPECTED_A_UNICODE_PROPERTY_NAME),
        (10, 8, diagnostic_codes::EXPECTED_A_UNICODE_PROPERTY_VALUE),
        (11, 7, diagnostic_codes::ANY_UNICODE_PROPERTY_THAT_WOULD_POSSIBLY_MATCH_MORE_THAN_A_SINGLE_CHARACTER_IS_O),
        (12, 7, diagnostic_codes::ANY_UNICODE_PROPERTY_THAT_WOULD_POSSIBLY_MATCH_MORE_THAN_A_SINGLE_CHARACTER_IS_O),
        (13, 8, diagnostic_codes::ANY_UNICODE_PROPERTY_THAT_WOULD_POSSIBLY_MATCH_MORE_THAN_A_SINGLE_CHARACTER_IS_O),
        (14, 4, diagnostic_codes::UNICODE_PROPERTY_VALUE_EXPRESSIONS_ARE_ONLY_AVAILABLE_WHEN_THE_UNICODE_U_FLAG_OR),
        (15, 4, diagnostic_codes::UNICODE_PROPERTY_VALUE_EXPRESSIONS_ARE_ONLY_AVAILABLE_WHEN_THE_UNICODE_U_FLAG_OR),
        (16, 4, diagnostic_codes::UNICODE_PROPERTY_VALUE_EXPRESSIONS_ARE_ONLY_AVAILABLE_WHEN_THE_UNICODE_U_FLAG_OR),
        (16, 7, diagnostic_codes::EXPECTED_A_UNICODE_PROPERTY_NAME_OR_VALUE),
        (17, 4, diagnostic_codes::UNICODE_PROPERTY_VALUE_EXPRESSIONS_ARE_ONLY_AVAILABLE_WHEN_THE_UNICODE_U_FLAG_OR),
        (17, 7, diagnostic_codes::ANY_UNICODE_PROPERTY_THAT_WOULD_POSSIBLY_MATCH_MORE_THAN_A_SINGLE_CHARACTER_IS_O),
        (18, 5, diagnostic_codes::UNICODE_PROPERTY_VALUE_EXPRESSIONS_ARE_ONLY_AVAILABLE_WHEN_THE_UNICODE_U_FLAG_OR),
        (19, 7, diagnostic_codes::EXPECTED_A_UNICODE_PROPERTY_NAME_OR_VALUE),
        (19, 8, diagnostic_codes::UNEXPECTED_DID_YOU_MEAN_TO_ESCAPE_IT_WITH_BACKSLASH),
        (20, 7, diagnostic_codes::EXPECTED_A_UNICODE_PROPERTY_NAME_OR_VALUE),
        (20, 21, diagnostic_codes::UNEXPECTED_DID_YOU_MEAN_TO_ESCAPE_IT_WITH_BACKSLASH),
    ];

    assert_eq!(
        actual, expected,
        "Unicode property escape diagnostics should match tsc exactly, got: {diagnostics:?}"
    );
}

/// The Unicode Sets (`v`) flag makes the properties-of-strings legal, so the
/// same sources that draw TS1528 under `u` must stay silent under `v`.
#[test]
fn test_regex_unicode_property_name_value_validation_matches_tsc() {
    // Adjacent-case matrix for TS1524/TS1526/TS1529 (unknown Unicode
    // property name / value / name-or-value), oracle-pinned against
    // typescript@7.0.2 alongside the pre-existing TS1523/1525/1527/1528/1530
    // siblings covered above: unknown name via each alias, unknown value
    // under a known name, an unknown lone name-or-value, a known Script
    // *value* used bare (invalid — Script only validates through the
    // `Name=Value` form), and clean positive cases through every alias
    // (`General_Category`/`gc`, `Script`/`sc`, `Script_Extensions`/`scx`,
    // a binary property).
    let source = r#"
const validation: RegExp[] = [
  /\p{Foo=Bar}/u,
  /\p{gc=Bar}/u,
  /\p{Script=NotAScript}/u,
  /\p{NotAThing}/u,
  /\p{Latin}/u,
  /\p{Script=Latin}/u,
  /\p{sc=Latin}/u,
  /\p{General_Category=Letter}/u,
  /\p{gc=Letter}/u,
  /\p{Alphabetic}/u,
  /\p{Script_Extensions=Han}/u,
  /\p{scx=Han}/u,
];
"#;
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let line_map = LineMap::build(source);

    let mut actual: Vec<(u32, u32, u32)> = diagnostics
        .iter()
        .map(|d| {
            let pos = line_map.offset_to_position(d.start, source);
            (pos.line + 1, pos.character + 1, d.code)
        })
        .collect();
    actual.sort_unstable();

    let expected: Vec<(u32, u32, u32)> = vec![
        (3, 7, diagnostic_codes::UNKNOWN_UNICODE_PROPERTY_NAME), // \p{Foo=Bar}, name alias unknown
        (4, 10, diagnostic_codes::UNKNOWN_UNICODE_PROPERTY_VALUE), // \p{gc=Bar}, known name, unknown value
        (5, 14, diagnostic_codes::UNKNOWN_UNICODE_PROPERTY_VALUE), // \p{Script=NotAScript}
        (
            6,
            7,
            diagnostic_codes::UNKNOWN_UNICODE_PROPERTY_NAME_OR_VALUE,
        ), // \p{NotAThing}
        (
            7,
            7,
            diagnostic_codes::UNKNOWN_UNICODE_PROPERTY_NAME_OR_VALUE,
        ), // \p{Latin}: a bare Script *value* only validates through `Script=`
    ];

    assert_eq!(
        actual, expected,
        "TS1524/TS1526/TS1529 adjacent-case matrix mismatch, got: {diagnostics:?}"
    );
}

#[test]
fn test_regex_properties_of_strings_are_accepted_under_the_v_flag() {
    let source = r#"
const underV: RegExp[] = [
  /\p{RGI_Emoji}/v,
  /\p{Basic_Emoji}/v,
  /\p{Emoji_Keycap_Sequence}/v,
  /\p{RGI_Emoji_Modifier_Sequence}/v,
  /\p{RGI_Emoji_Flag_Sequence}/v,
  /\p{RGI_Emoji_Tag_Sequence}/v,
  /\p{RGI_Emoji_ZWJ_Sequence}/v,
  /[\p{RGI_Emoji}]/v,
];
"#;
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();

    assert!(
        diagnostics.is_empty(),
        "Properties of strings are legal under the v flag, got: {diagnostics:?}"
    );
}

/// Codes per witness, in emission order, so the modifier-group tests below can
/// assert an exact sequence rather than "contains".
fn regex_codes(source: &str, target: tsz_common::ScriptTarget) -> Vec<u32> {
    let (parser, _root) =
        crate::parser::test_fixture::parse_source_with_language_version(source, target);
    parser.get_diagnostics().iter().map(|d| d.code).collect()
}

#[test]
fn test_subpattern_modifier_group_rejects_non_toggleable_flags_with_ts1509() {
    // Only `i`, `m` and `s` may be toggled within a subpattern; `d`, `g`, `u`,
    // `v` and `y` are whole-pattern flags. Checked on both sides of the minus
    // so the second run is not silently exempt.
    for witness in [
        "const r = /(?d:x)/;",
        "const r = /(?g:x)/;",
        "const r = /(?u:x)/;",
        "const r = /(?v:x)/;",
        "const r = /(?y:x)/;",
        "const r = /(?-d:x)/;",
        "const r = /(?-g:x)/;",
        "const r = /(?-y:x)/;",
    ] {
        assert_eq!(
            regex_codes(witness, tsz_common::ScriptTarget::ES2022),
            vec![
                diagnostic_codes::THIS_REGULAR_EXPRESSION_FLAG_CANNOT_BE_TOGGLED_WITHIN_A_SUBPATTERN
            ],
            "{witness} should report exactly one TS1509"
        );
    }
}

#[test]
fn test_subpattern_modifier_group_minus_without_any_flag_reports_ts1504() {
    // TS1504 is positional in tsc: it fires only when the whole prelude
    // consumed nothing but the minus sign, so a flag on either side clears it.
    assert_eq!(
        regex_codes("const r = /(?-:x)/;", tsz_common::ScriptTarget::ES2022),
        vec![diagnostic_codes::SUBPATTERN_FLAGS_MUST_BE_PRESENT_WHEN_THERE_IS_A_MINUS_SIGN],
    );
    assert_eq!(
        regex_codes("const r = /(?i-:x)/;", tsz_common::ScriptTarget::ES2022),
        Vec::<u32>::new(),
        "flags before the minus satisfy the rule"
    );
    assert_eq!(
        regex_codes("const r = /(?-i:x)/;", tsz_common::ScriptTarget::ES2022),
        Vec::<u32>::new(),
        "flags after the minus satisfy the rule"
    );
    assert_eq!(
        regex_codes("const r = /(?i-m:x)/;", tsz_common::ScriptTarget::ES2022),
        Vec::<u32>::new(),
        "flags on both sides are the ordinary legal form"
    );
}

#[test]
fn test_subpattern_modifier_group_duplicate_flags_report_ts1500() {
    // The second run is seeded with the first run's flags, so `(?i-i:` is a
    // duplicate rather than a set-then-clear toggle.
    assert_eq!(
        regex_codes("const r = /(?ii:x)/;", tsz_common::ScriptTarget::ES2022),
        vec![diagnostic_codes::DUPLICATE_REGULAR_EXPRESSION_FLAG],
    );
    assert_eq!(
        regex_codes("const r = /(?i-i:x)/;", tsz_common::ScriptTarget::ES2022),
        vec![diagnostic_codes::DUPLICATE_REGULAR_EXPRESSION_FLAG],
    );
    assert_eq!(
        regex_codes(
            "const r = /(?ims-ims:x)/;",
            tsz_common::ScriptTarget::ES2022
        ),
        vec![
            diagnostic_codes::DUPLICATE_REGULAR_EXPRESSION_FLAG,
            diagnostic_codes::DUPLICATE_REGULAR_EXPRESSION_FLAG,
            diagnostic_codes::DUPLICATE_REGULAR_EXPRESSION_FLAG,
        ],
    );
}

#[test]
fn test_subpattern_modifier_group_without_colon_reports_ts1005() {
    // A group that opened `(?` stays a modifier group even when malformed, so
    // the missing `:` is reported instead of the prelude being re-scanned as
    // pattern characters.
    assert_eq!(
        regex_codes("const r = /(?i)/;", tsz_common::ScriptTarget::ES2022),
        vec![diagnostic_codes::EXPECTED],
    );
    assert_eq!(
        regex_codes("const r = /(?P<n>x)/;", tsz_common::ScriptTarget::ES2022),
        vec![
            diagnostic_codes::UNKNOWN_REGULAR_EXPRESSION_FLAG,
            diagnostic_codes::EXPECTED,
        ],
        "`P` is an unknown flag, then the `<` is where the `:` should have been"
    );
    assert_eq!(
        regex_codes("const r = /(?-)/;", tsz_common::ScriptTarget::ES2022),
        vec![
            diagnostic_codes::SUBPATTERN_FLAGS_MUST_BE_PRESENT_WHEN_THERE_IS_A_MINUS_SIGN,
            diagnostic_codes::EXPECTED,
        ],
    );
    assert_eq!(
        regex_codes("const r = /(?/;", tsz_common::ScriptTarget::ES2022),
        vec![diagnostic_codes::EXPECTED],
        "end of body is not a group terminator — tsc still wants the `:`"
    );
}

#[test]
fn test_subpattern_modifier_group_is_not_a_capturing_group() {
    // `\1` has nothing to refer to when the only group is a modifier group.
    assert_eq!(
        regex_codes("const r = /(?i:x)\\1/;", tsz_common::ScriptTarget::ES2022),
        vec![
            diagnostic_codes::THIS_BACKREFERENCE_REFERS_TO_A_GROUP_THAT_DOES_NOT_EXIST_THERE_ARE_NO_CAPTURING
        ],
    );
    assert_eq!(
        regex_codes("const r = /(?i:(x))\\1/;", tsz_common::ScriptTarget::ES2022),
        Vec::<u32>::new(),
        "a real group nested inside a modifier group still counts"
    );
}

#[test]
fn test_subpattern_modifier_group_dot_all_flag_follows_target_availability() {
    // Of the three toggleable flags only `s` is version-gated, and TS1509
    // rejects every other flag before availability is ever consulted.
    assert_eq!(
        regex_codes("const r = /(?s:x)/;", tsz_common::ScriptTarget::ES2015),
        vec![diagnostic_codes::THIS_REGULAR_EXPRESSION_FLAG_IS_ONLY_AVAILABLE_WHEN_TARGETING_OR_LATER],
    );
    assert_eq!(
        regex_codes("const r = /(?s:x)/;", tsz_common::ScriptTarget::ES2018),
        Vec::<u32>::new(),
    );
    assert_eq!(
        regex_codes("const r = /(?im:x)/;", tsz_common::ScriptTarget::ES2015),
        Vec::<u32>::new(),
        "`i` and `m` are not version-gated"
    );
}

#[test]
fn test_non_modifier_group_forms_are_unaffected() {
    // The `(?` arm now runs unconditionally, so every neighbouring group form
    // has to stay silent — including the degenerate `(?:` modifier group.
    for witness in [
        "const r = /(?:x)/;",
        "const r = /(?=x)/;",
        "const r = /(?!x)/;",
        "const r = /(?<=x)/;",
        "const r = /(?<!x)/;",
        "const r = /(?<n>x)\\k<n>/;",
        "const r = /(x)(?:y)\\1/;",
        "const r = /(?i:x)*/;",
        "const r = /a(?i:b)c/u;",
        "const r = /(?i:(?m:x))/;",
    ] {
        assert_eq!(
            regex_codes(witness, tsz_common::ScriptTarget::ES2022),
            Vec::<u32>::new(),
            "{witness} is legal and must stay clean"
        );
    }
}

// Regression for the `regExpWithOpenBracketInCharClass` / `regularExpressionScanning`
// conformance family: under the `v` (unicodeSets) flag, an unescaped `[` inside a
// character class opens a NESTED class that needs its own closing `]`, so
// `missing_regex_closing_token`'s balance check must track class nesting DEPTH
// under `v`, not a single in-class boolean. Without `v`, a nested `[` is an
// ordinary class member and never needs its own close.
#[test]
fn test_unclosed_nested_class_under_unicode_sets_flag_reports_ts1005() {
    // `[[]` under `v`: outer `[` opens, inner `[` opens a nested class, the
    // lone `]` closes only the inner one — the outer class is never closed.
    assert_eq!(
        regex_codes("const r = /[[]/v;", tsz_common::ScriptTarget::ES2024),
        vec![diagnostic_codes::EXPECTED],
        "outer class left open by a nested class must report ']' expected"
    );
}

#[test]
fn test_same_bracket_shape_without_v_flag_is_not_a_nested_class() {
    // Same `[[]` byte shape, but without `v` a nested `[` is just an ordinary
    // class member: the single `]` closes the (only) class and the regex is
    // well-formed. `u` alone (no `v`) must behave the same way.
    for witness in ["const r = /[[]/;", "const r = /[[]/u;"] {
        assert_eq!(
            regex_codes(witness, tsz_common::ScriptTarget::ES2024),
            Vec::<u32>::new(),
            "{witness}: nested-looking `[` without `v` must not require a second `]`"
        );
    }
}

#[test]
fn test_balanced_nested_class_under_unicode_sets_flag_is_clean() {
    // `[[]]` under `v`: outer opens, inner opens and closes, outer closes.
    // Fully balanced — no missing-token diagnostic.
    assert_eq!(
        regex_codes("const r = /[[]]/v;", tsz_common::ScriptTarget::ES2024),
        Vec::<u32>::new(),
        "fully balanced nested class under v must stay clean"
    );
}

#[test]
fn test_deeply_nested_unclosed_class_under_unicode_sets_flag_reports_ts1005() {
    // `[[[]]` under `v`: three opens, two closes — one level (the outermost)
    // is left open. Depth tracking, not a boolean, is required to see this.
    assert_eq!(
        regex_codes("const r = /[[[]]/v;", tsz_common::ScriptTarget::ES2024),
        vec![diagnostic_codes::EXPECTED],
        "one unclosed level in a deeper nest must still report ']' expected"
    );
    // `[[[]]]` under `v`: three opens, three closes — fully balanced.
    assert_eq!(
        regex_codes("const r = /[[[]]]/v;", tsz_common::ScriptTarget::ES2024),
        Vec::<u32>::new(),
        "a fully balanced deeper nest under v must stay clean"
    );
}

#[test]
fn test_realistic_class_set_expression_does_not_falsely_report_unclosed_class() {
    // A real-world `unicodeSets` class-set expression (from the
    // `regularExpressionScanning` conformance fixture) with heavy nesting and
    // subtraction/intersection operators. It has its own set of dedicated
    // class-set diagnostics (`--`/`&&` operand errors, including the
    // `'&&'/'--' expected.` stray-operand report that a committed set-op class
    // draws — TS1005, same code as `']' expected.`), but the OUTER class
    // balance itself is correct and must not additionally trigger the generic
    // `']' expected` unterminated-class fallback. Assert on the message so the
    // legitimate `'--' expected.` reports (which `tsc` 7.0.2 also emits here)
    // are not conflated with the false-positive this test guards against.
    let source = r"const r = /[a--b[--][\d++[]]&&[[&0-9--]&&[\p{L}]--\P{L}-_-]]&&&\q{foo}[0---9][&&q&&&\q{bar}&&]/v;";
    let (parser, _root) = crate::parser::test_fixture::parse_source_with_language_version(
        source,
        tsz_common::ScriptTarget::ES2024,
    );
    assert!(
        !parser
            .get_diagnostics()
            .iter()
            .any(|d| d.code == diagnostic_codes::EXPECTED && d.message == "']' expected."),
        "balanced outer class in a heavily-nested class-set expression must not report ']' expected"
    );
}
