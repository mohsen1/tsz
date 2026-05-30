use super::*;

/// Helper: create a scanner that skips trivia and collect all tokens.
fn scan_all(source: &str) -> Vec<(SyntaxKind, String)> {
    let mut scanner = ScannerState::new(source.to_string(), true);
    let mut tokens = Vec::new();
    loop {
        let kind = scanner.scan();
        if kind == SyntaxKind::EndOfFileToken {
            break;
        }
        tokens.push((kind, scanner.get_token_value()));
    }
    tokens
}

/// Helper: create a scanner that preserves trivia and collect all tokens.
fn scan_all_with_trivia(source: &str) -> Vec<(SyntaxKind, String)> {
    let mut scanner = ScannerState::new(source.to_string(), false);
    let mut tokens = Vec::new();
    loop {
        let kind = scanner.scan();
        if kind == SyntaxKind::EndOfFileToken {
            break;
        }
        tokens.push((kind, scanner.get_token_text()));
    }
    tokens
}

// ── Empty input ───────────────────────────────────────────────────

#[test]
fn empty_input_returns_eof() {
    let mut scanner = ScannerState::new(String::new(), true);
    assert_eq!(scanner.scan(), SyntaxKind::EndOfFileToken);
}

// ── Identifiers ───────────────────────────────────────────────────

#[test]
fn scan_identifiers() {
    let tokens = scan_all("foo bar _baz $qux");
    assert_eq!(tokens.len(), 4);
    assert_eq!(tokens[0], (SyntaxKind::Identifier, "foo".to_string()));
    assert_eq!(tokens[1], (SyntaxKind::Identifier, "bar".to_string()));
    assert_eq!(tokens[2], (SyntaxKind::Identifier, "_baz".to_string()));
    assert_eq!(tokens[3], (SyntaxKind::Identifier, "$qux".to_string()));
}

#[test]
fn scan_identifier_with_other_id_continue_middle_dot() {
    let tokens = scan_all("a·b");
    assert_eq!(tokens.len(), 1);
    assert_eq!(tokens[0], (SyntaxKind::Identifier, "a·b".to_string()));
}

#[test]
fn scan_es2015_braced_astral_escape_as_identifier_start() {
    let mut scanner = ScannerState::new(r"\u{102A7}tail".to_string(), true);
    scanner.set_language_version(ScriptTarget::ES2015);

    assert_eq!(scanner.scan(), SyntaxKind::Identifier);
    assert_eq!(scanner.get_token_value_ref(), "𐊧tail");
    assert_eq!(scanner.get_token_text(), r"\u{102A7}tail");
}

#[test]
fn scan_es5_braced_astral_escape_remains_invalid_identifier_start() {
    let mut scanner = ScannerState::new(r"\u{102A7}tail".to_string(), true);
    scanner.set_language_version(ScriptTarget::ES5);

    assert_eq!(scanner.scan(), SyntaxKind::Unknown);
    assert_eq!(scanner.get_token_text(), "\\");
}

// ── Keywords ──────────────────────────────────────────────────────

#[test]
fn scan_keywords() {
    let tokens = scan_all("if else while for const let var");
    assert_eq!(tokens.len(), 7);
    assert_eq!(tokens[0].0, SyntaxKind::IfKeyword);
    assert_eq!(tokens[1].0, SyntaxKind::ElseKeyword);
    assert_eq!(tokens[2].0, SyntaxKind::WhileKeyword);
    assert_eq!(tokens[3].0, SyntaxKind::ForKeyword);
    assert_eq!(tokens[4].0, SyntaxKind::ConstKeyword);
    assert_eq!(tokens[5].0, SyntaxKind::LetKeyword);
    assert_eq!(tokens[6].0, SyntaxKind::VarKeyword);
}

// ── Numeric literals ──────────────────────────────────────────────

#[test]
fn scan_numeric_literals() {
    let tokens = scan_all("0 42 3.14 0xFF 0b1010 0o777 1_000");
    assert_eq!(tokens.len(), 7);
    for (kind, _) in &tokens {
        assert_eq!(*kind, SyntaxKind::NumericLiteral);
    }
    assert_eq!(tokens[0].1, "0");
    assert_eq!(tokens[1].1, "42");
    assert_eq!(tokens[2].1, "3.14");
    assert_eq!(tokens[3].1, "0xFF");
    assert_eq!(tokens[4].1, "0b1010");
    assert_eq!(tokens[5].1, "0o777");
    assert_eq!(tokens[6].1, "1_000");
}

#[test]
fn scan_bigint_suffix_on_failed_exponent_preserves_full_text() {
    // Regression for `identifierStartAfterNumericLiteral`: when a numeric
    // literal has a failed exponent immediately followed by `n` (e.g.
    // `3en`, `123en`), tsc treats the `n` as a recovered BigInt suffix
    // and the literal text spans the full `<digits>en` range. Emit must
    // preserve that source spelling — printing `3e` instead drops the
    // `n` and produces `3e[null];` where tsc produces `3en[null];`.
    let tokens = scan_all("3en 123en");
    assert_eq!(tokens.len(), 2);
    assert_eq!(tokens[0].0, SyntaxKind::NumericLiteral);
    assert_eq!(tokens[0].1, "3en");
    assert_eq!(tokens[1].0, SyntaxKind::NumericLiteral);
    assert_eq!(tokens[1].1, "123en");

    // Sanity: `1ee` is a different shape — the trailing `e` is not an
    // `n` BigInt suffix, so the scanner resets pos before `e` and the
    // numeric token is just `1e`. Emit then renders `1e; e;`.
    let tokens = scan_all("1ee");
    assert_eq!(tokens.len(), 2);
    assert_eq!(tokens[0].0, SyntaxKind::NumericLiteral);
    assert_eq!(tokens[0].1, "1e");
    assert_eq!(tokens[1].0, SyntaxKind::Identifier);
    assert_eq!(tokens[1].1, "e");
}

// ── String literals ───────────────────────────────────────────────

#[test]
fn scan_string_literals() {
    let tokens = scan_all(r#""hello" 'world'"#);
    assert_eq!(tokens.len(), 2);
    assert_eq!(tokens[0].0, SyntaxKind::StringLiteral);
    assert_eq!(tokens[0].1, "hello");
    assert_eq!(tokens[1].0, SyntaxKind::StringLiteral);
    assert_eq!(tokens[1].1, "world");
}

#[test]
fn scan_invalid_hex_escape_emits_ts1125() {
    // Regression for `compiler/stringLiteralsErrors.ts`: tsc emits
    // TS1125 "Hexadecimal digit expected." for `\x` followed by fewer
    // than two hex digits. Anchor is at the position the scan halted
    // (the first non-hex char or the closing quote).
    let mut scanner = ScannerState::new(r#""\x0""#.to_string(), true);
    loop {
        if scanner.scan() == SyntaxKind::EndOfFileToken {
            break;
        }
    }
    let diags = &scanner.scanner_diagnostics;
    // `"` is at byte 0, `\` at 1, `x` at 2, `0` at 3, closing `"` at 4.
    // After consuming the single hex digit `0`, scanner halts at the
    // closing quote (byte 4). tsc emits the error there.
    assert!(
        diags
            .iter()
            .any(|d| d.code == diagnostic_codes::HEXADECIMAL_DIGIT_EXPECTED && d.pos == 4),
        "expected TS1125 at the closing quote (byte 4), got: {diags:?}"
    );

    let mut scanner2 = ScannerState::new(r#""\xmm""#.to_string(), true);
    loop {
        if scanner2.scan() == SyntaxKind::EndOfFileToken {
            break;
        }
    }
    let diags2 = &scanner2.scanner_diagnostics;
    // `\xmm`: scanner halts at the first `m` (byte 3) since it's not hex.
    assert!(
        diags2
            .iter()
            .any(|d| d.code == diagnostic_codes::HEXADECIMAL_DIGIT_EXPECTED && d.pos == 3),
        "expected TS1125 at the first non-hex char (byte 3), got: {diags2:?}"
    );

    // Sanity guard: a well-formed `\x41` ('A') must not gain a diagnostic.
    let mut scanner3 = ScannerState::new(r#""\x41""#.to_string(), true);
    loop {
        if scanner3.scan() == SyntaxKind::EndOfFileToken {
            break;
        }
    }
    assert!(
        !scanner3
            .scanner_diagnostics
            .iter()
            .any(|d| d.code == diagnostic_codes::HEXADECIMAL_DIGIT_EXPECTED),
        "well-formed `\\x41` must not emit TS1125, got: {:?}",
        scanner3.scanner_diagnostics
    );
}

// ── Template literals ─────────────────────────────────────────────

#[test]
fn scan_no_substitution_template() {
    let tokens = scan_all("`hello world`");
    assert_eq!(tokens.len(), 1);
    assert_eq!(tokens[0].0, SyntaxKind::NoSubstitutionTemplateLiteral);
    assert_eq!(tokens[0].1, "hello world");
}

// ── JSX attribute string literals ─────────────────────────────────

/// Regression for #3977: `scan_jsx_string_literal` advanced by one byte
/// after pushing a character, so multi-byte UTF-8 chars were re-decoded
/// from a continuation-byte offset and pushed again. The token value of
/// `"é"` became `"éé"`, breaking JSX attribute string-literal types.
#[test]
fn scan_jsx_attribute_value_preserves_non_ascii() {
    // 2-byte UTF-8 (é = U+00E9, 0xC3 0xA9).
    let mut s = ScannerState::new("\"é\"".to_string(), true);
    assert_eq!(s.scan_jsx_attribute_value(), SyntaxKind::StringLiteral);
    assert_eq!(s.get_token_value(), "é");

    // 3-byte UTF-8 (中 = U+4E2D).
    let mut s = ScannerState::new("\"中文\"".to_string(), true);
    assert_eq!(s.scan_jsx_attribute_value(), SyntaxKind::StringLiteral);
    assert_eq!(s.get_token_value(), "中文");

    // 4-byte UTF-8 (😀 = U+1F600, surrogate-pair scalar).
    let mut s = ScannerState::new("\"a😀b\"".to_string(), true);
    assert_eq!(s.scan_jsx_attribute_value(), SyntaxKind::StringLiteral);
    assert_eq!(s.get_token_value(), "a😀b");

    // Mixed widths and single quotes also exercised by JSX attributes.
    let mut s = ScannerState::new("'héllo 中 😀'".to_string(), true);
    assert_eq!(s.scan_jsx_attribute_value(), SyntaxKind::StringLiteral);
    assert_eq!(s.get_token_value(), "héllo 中 😀");
}

// ── Punctuation ───────────────────────────────────────────────────

#[test]
fn scan_punctuation() {
    let tokens = scan_all("{ } ( ) [ ] ; , . ...");
    let kinds: Vec<SyntaxKind> = tokens.iter().map(|(k, _)| *k).collect();
    assert_eq!(
        kinds,
        vec![
            SyntaxKind::OpenBraceToken,
            SyntaxKind::CloseBraceToken,
            SyntaxKind::OpenParenToken,
            SyntaxKind::CloseParenToken,
            SyntaxKind::OpenBracketToken,
            SyntaxKind::CloseBracketToken,
            SyntaxKind::SemicolonToken,
            SyntaxKind::CommaToken,
            SyntaxKind::DotToken,
            SyntaxKind::DotDotDotToken,
        ]
    );
}

// ── Operators ─────────────────────────────────────────────────────

#[test]
fn scan_comparison_operators() {
    // Note: > is always scanned as GreaterThanToken; the parser calls
    // re_scan_greater_token() to disambiguate >= / >> / >>> etc.
    let tokens = scan_all("== != === !== < <=");
    let kinds: Vec<SyntaxKind> = tokens.iter().map(|(k, _)| *k).collect();
    assert_eq!(
        kinds,
        vec![
            SyntaxKind::EqualsEqualsToken,
            SyntaxKind::ExclamationEqualsToken,
            SyntaxKind::EqualsEqualsEqualsToken,
            SyntaxKind::ExclamationEqualsEqualsToken,
            SyntaxKind::LessThanToken,
            SyntaxKind::LessThanEqualsToken,
        ]
    );
}

#[test]
fn scan_greater_than_is_single_token() {
    // The scanner always produces GreaterThanToken for '>'.
    // Multi-char variants (>=, >>, >>>, >>=, >>>=) require re_scan_greater_token().
    let tokens = scan_all(">");
    assert_eq!(tokens.len(), 1);
    assert_eq!(tokens[0].0, SyntaxKind::GreaterThanToken);

    // ">=" is scanned as ">" then "=" by the raw scanner
    let tokens = scan_all(">=");
    assert_eq!(tokens[0].0, SyntaxKind::GreaterThanToken);
}

#[test]
fn scan_assignment_operators() {
    // Exclude >>= and >>>= which need re_scan_greater_token from parser context
    let tokens = scan_all("= += -= *= **= /= %= <<= &= |= ^= ||= &&= ??=");
    let kinds: Vec<SyntaxKind> = tokens.iter().map(|(k, _)| *k).collect();
    assert_eq!(
        kinds,
        vec![
            SyntaxKind::EqualsToken,
            SyntaxKind::PlusEqualsToken,
            SyntaxKind::MinusEqualsToken,
            SyntaxKind::AsteriskEqualsToken,
            SyntaxKind::AsteriskAsteriskEqualsToken,
            SyntaxKind::SlashEqualsToken,
            SyntaxKind::PercentEqualsToken,
            SyntaxKind::LessThanLessThanEqualsToken,
            SyntaxKind::AmpersandEqualsToken,
            SyntaxKind::BarEqualsToken,
            SyntaxKind::CaretEqualsToken,
            SyntaxKind::BarBarEqualsToken,
            SyntaxKind::AmpersandAmpersandEqualsToken,
            SyntaxKind::QuestionQuestionEqualsToken,
        ]
    );
}

#[test]
fn scan_logical_operators() {
    let tokens = scan_all("&& || ?? !");
    let kinds: Vec<SyntaxKind> = tokens.iter().map(|(k, _)| *k).collect();
    assert_eq!(
        kinds,
        vec![
            SyntaxKind::AmpersandAmpersandToken,
            SyntaxKind::BarBarToken,
            SyntaxKind::QuestionQuestionToken,
            SyntaxKind::ExclamationToken,
        ]
    );
}

#[test]
fn scan_arrow_function() {
    let tokens = scan_all("=>");
    assert_eq!(tokens.len(), 1);
    assert_eq!(tokens[0].0, SyntaxKind::EqualsGreaterThanToken);
}

// ── Trivia handling ───────────────────────────────────────────────

#[test]
fn trivia_skip_mode() {
    let tokens = scan_all("  a  b  ");
    assert_eq!(tokens.len(), 2);
    assert_eq!(tokens[0].1, "a");
    assert_eq!(tokens[1].1, "b");
}

#[test]
fn trivia_preserve_mode() {
    let tokens = scan_all_with_trivia(" a ");
    assert_eq!(tokens.len(), 3);
    assert_eq!(tokens[0].0, SyntaxKind::WhitespaceTrivia);
    assert_eq!(tokens[1].0, SyntaxKind::Identifier);
    assert_eq!(tokens[2].0, SyntaxKind::WhitespaceTrivia);
}

#[test]
fn newline_trivia_preserved() {
    let tokens = scan_all_with_trivia("a\nb");
    assert_eq!(tokens.len(), 3);
    assert_eq!(tokens[0].0, SyntaxKind::Identifier);
    assert_eq!(tokens[1].0, SyntaxKind::NewLineTrivia);
    assert_eq!(tokens[2].0, SyntaxKind::Identifier);
}

// ── Comments ──────────────────────────────────────────────────────

#[test]
fn single_line_comment_skipped() {
    let tokens = scan_all("a // comment\nb");
    assert_eq!(tokens.len(), 2);
    assert_eq!(tokens[0].1, "a");
    assert_eq!(tokens[1].1, "b");
}

#[test]
fn multi_line_comment_skipped() {
    let tokens = scan_all("a /* comment */ b");
    assert_eq!(tokens.len(), 2);
    assert_eq!(tokens[0].1, "a");
    assert_eq!(tokens[1].1, "b");
}

#[test]
fn single_line_comment_terminates_at_unicode_line_separator() {
    let tokens = scan_all("a // comment\u{2028}b");
    assert_eq!(tokens.len(), 2, "tokens were: {tokens:?}");
    assert_eq!(tokens[0].1, "a");
    assert_eq!(tokens[1].1, "b");
}

#[test]
fn single_line_comment_terminates_at_unicode_paragraph_separator() {
    let tokens = scan_all("a // comment\u{2029}b");
    assert_eq!(tokens.len(), 2, "tokens were: {tokens:?}");
    assert_eq!(tokens[0].1, "a");
    assert_eq!(tokens[1].1, "b");
}

#[test]
fn directive_comment_does_not_swallow_next_line_via_unicode_line_separator() {
    let mut scanner =
        ScannerState::new("// @ts-expect-error\u{2028}const ok = 1;".to_string(), true);
    assert_eq!(scanner.scan(), SyntaxKind::ConstKeyword);
    assert!(scanner.has_preceding_line_break());
}

#[test]
fn multi_line_comment_unicode_line_separator_sets_preceding_line_break() {
    let mut scanner = ScannerState::new("a /*\u{2028}*/ b".to_string(), true);
    scanner.scan(); // "a"
    assert!(!scanner.has_preceding_line_break());
    scanner.scan(); // "b"
    assert!(scanner.has_preceding_line_break());
}

// ── Scanner state methods ─────────────────────────────────────────

#[test]
fn scanner_position_tracking() {
    let mut scanner = ScannerState::new("abc def".to_string(), true);
    scanner.scan(); // "abc"
    assert_eq!(scanner.get_token_start(), 0);
    assert_eq!(scanner.get_token_end(), 3);

    scanner.scan(); // "def"
    assert_eq!(scanner.get_token_start(), 4);
    assert_eq!(scanner.get_token_end(), 7);
}

#[test]
fn scanner_set_text() {
    let mut scanner = ScannerState::new("abc".to_string(), true);
    scanner.scan();
    assert_eq!(scanner.get_token_value(), "abc");

    scanner.set_text("xyz".to_string(), None, None);
    scanner.scan();
    assert_eq!(scanner.get_token_value(), "xyz");
}

#[test]
fn scanner_reset_token_state() {
    let mut scanner = ScannerState::new("ab cd".to_string(), true);
    scanner.scan(); // "ab"
    scanner.scan(); // "cd"
    scanner.reset_token_state(0);
    scanner.scan();
    assert_eq!(scanner.get_token_value(), "ab");
}

// ── Preceding line break ──────────────────────────────────────────

#[test]
fn preceding_line_break_detection() {
    let mut scanner = ScannerState::new("a\nb".to_string(), true);
    scanner.scan(); // "a"
    assert!(!scanner.has_preceding_line_break());
    scanner.scan(); // "b"
    assert!(scanner.has_preceding_line_break());
}

// ── BigInt literals ───────────────────────────────────────────────

#[test]
fn scan_bigint_literal() {
    let tokens = scan_all("42n 0xFFn");
    assert_eq!(tokens.len(), 2);
    assert_eq!(tokens[0].0, SyntaxKind::BigIntLiteral);
    assert_eq!(tokens[1].0, SyntaxKind::BigIntLiteral);
}

// ── Optional chaining ─────────────────────────────────────────────

#[test]
fn scan_optional_chaining() {
    let tokens = scan_all("a?.b");
    let kinds: Vec<SyntaxKind> = tokens.iter().map(|(k, _)| *k).collect();
    assert_eq!(
        kinds,
        vec![
            SyntaxKind::Identifier,
            SyntaxKind::QuestionDotToken,
            SyntaxKind::Identifier,
        ]
    );
}

// ── Hash/private identifier ───────────────────────────────────────

#[test]
fn scan_private_identifier() {
    let tokens = scan_all("#field");
    assert_eq!(tokens.len(), 1);
    assert_eq!(tokens[0].0, SyntaxKind::PrivateIdentifier);
    assert_eq!(tokens[0].1, "#field");
}

// ── Helper function tests ─────────────────────────────────────────

#[test]
fn is_line_break_chars() {
    assert!(is_line_break(CharacterCodes::LINE_FEED));
    assert!(is_line_break(CharacterCodes::CARRIAGE_RETURN));
    assert!(is_line_break(CharacterCodes::LINE_SEPARATOR));
    assert!(is_line_break(CharacterCodes::PARAGRAPH_SEPARATOR));
    assert!(!is_line_break(CharacterCodes::SPACE));
    assert!(!is_line_break(CharacterCodes::TAB));
}

#[test]
fn is_white_space_single_line_chars() {
    assert!(is_white_space_single_line(CharacterCodes::SPACE));
    assert!(is_white_space_single_line(CharacterCodes::TAB));
    assert!(is_white_space_single_line(CharacterCodes::FORM_FEED));
    assert!(is_white_space_single_line(
        CharacterCodes::NON_BREAKING_SPACE
    ));
    assert!(!is_white_space_single_line(CharacterCodes::LINE_FEED));
    assert!(!is_white_space_single_line(CharacterCodes::CARRIAGE_RETURN));
    assert!(!is_white_space_single_line(0x41)); // 'A'
}

#[test]
fn is_identifier_start_chars() {
    assert!(is_identifier_start(CharacterCodes::LOWER_A));
    assert!(is_identifier_start(CharacterCodes::UPPER_Z));
    assert!(is_identifier_start(CharacterCodes::UNDERSCORE));
    assert!(is_identifier_start(CharacterCodes::DOLLAR));
    assert!(!is_identifier_start(CharacterCodes::_0));
    assert!(!is_identifier_start(CharacterCodes::SPACE));
    assert!(!is_identifier_start(CharacterCodes::PLUS));
}

#[test]
fn is_identifier_part_rejects_subscript_digits() {
    // U+2081 SUBSCRIPT ONE is No (Number, other), NOT Nd — must be rejected
    assert!(!is_identifier_part(0x2081)); // ₁
    assert!(!is_identifier_part(0x2082)); // ₂
    assert!(!is_identifier_part(0x00B2)); // ² SUPERSCRIPT TWO (No)
    assert!(!is_identifier_part(0x00B3)); // ³ SUPERSCRIPT THREE (No)
    assert!(!is_identifier_part(0x00BC)); // ¼ VULGAR FRACTION ONE QUARTER (No)
    // Nd digits should be accepted
    assert!(is_identifier_part(0x0966)); // Devanagari digit zero (Nd)
    assert!(is_identifier_part(0x0660)); // Arabic-Indic digit zero (Nd)
    assert!(is_identifier_part(0xFF10)); // Fullwidth digit zero (Nd)
    // ASCII digits should be accepted
    assert!(is_identifier_part(0x30)); // '0'
    assert!(is_identifier_part(0x39)); // '9'
    // Letters should be accepted
    assert!(is_identifier_part(0x61)); // 'a'
    // Other_ID_Continue characters should be accepted
    assert!(is_identifier_part(0x00B7)); // · MIDDLE DOT
    assert!(is_identifier_part(0x0387)); // · GREEK ANO TELEIA
    assert!(is_identifier_part(0x1369)); // ፩ ETHIOPIC DIGIT ONE
    assert!(is_identifier_part(0x19DA)); // ᧚ NEW TAI LUE THAM DIGIT ONE
}

#[test]
fn is_digit_chars() {
    assert!(is_digit(CharacterCodes::_0));
    assert!(is_digit(CharacterCodes::_9));
    assert!(!is_digit(CharacterCodes::LOWER_A));
    assert!(!is_digit(CharacterCodes::SPACE));
}

#[test]
fn is_regex_flag_chars() {
    assert!(is_regex_flag(CharacterCodes::LOWER_G));
    assert!(is_regex_flag(CharacterCodes::LOWER_I));
    assert!(is_regex_flag(CharacterCodes::LOWER_M));
    assert!(is_regex_flag(CharacterCodes::LOWER_S));
    assert!(is_regex_flag(CharacterCodes::LOWER_U));
    assert!(is_regex_flag(CharacterCodes::LOWER_V));
    assert!(is_regex_flag(CharacterCodes::LOWER_Y));
    assert!(is_regex_flag(CharacterCodes::LOWER_D));
    assert!(!is_regex_flag(CharacterCodes::LOWER_A));
    assert!(!is_regex_flag(CharacterCodes::LOWER_Z));
}

// ── Scanner snapshot/restore ──────────────────────────────────────

#[test]
fn scanner_snapshot_and_restore() {
    let mut scanner = ScannerState::new("a + b".to_string(), true);
    scanner.scan(); // "a"
    let snapshot = scanner.save_state();
    scanner.scan(); // "+"
    scanner.scan(); // "b"
    assert_eq!(scanner.get_token_value(), "b");
    scanner.restore_state(snapshot);
    assert_eq!(scanner.get_token(), SyntaxKind::Identifier);
    // After restoring, scanning again should give "+"
    let next = scanner.scan();
    assert_eq!(next, SyntaxKind::PlusToken);
}

// ── Rescan methods ────────────────────────────────────────────────

#[test]
fn rescan_greater_than_token() {
    // The scanner always scans ">" as GreaterThanToken.
    // re_scan_greater_token() is used by the parser to check if it could be >=, >>, etc.
    let mut scanner = ScannerState::new(">= x".to_string(), true);
    scanner.scan(); // scans ">"
    assert_eq!(scanner.get_token(), SyntaxKind::GreaterThanToken);

    // Rescan to get >=
    let rescanned = scanner.re_scan_greater_token();
    assert_eq!(rescanned, SyntaxKind::GreaterThanEqualsToken);
}

// ── Numeric separator diagnostics ────────────────────────────────
//
// These lock in tsc parity for `scanHexDigits`/`scanNumberFragment`:
// each invalid `_` produces its own diagnostic, distinguished by whether
// the preceding character was a *valid* separator (TS6189) or not
// (TS6188). Same-position emissions collapse to the first, mirroring
// tsc's `parseErrorAtPosition` skip-if-same-start dedup.

fn scan_separator_diagnostics(source: &str) -> Vec<(usize, u32)> {
    let mut scanner = ScannerState::new(source.to_string(), true);
    loop {
        let kind = scanner.scan();
        if kind == SyntaxKind::EndOfFileToken {
            break;
        }
    }
    scanner
        .get_scanner_diagnostics()
        .iter()
        .map(|d| (d.pos, d.code))
        .collect()
}

#[test]
fn separator_three_leading_underscores_emits_three_ts6188() {
    // `0x___0111010_0101_1` — tsc reports TS6188 at each of the three
    // leading underscores. Previously we only recorded the first one.
    let diags = scan_separator_diagnostics("0x___0111010_0101_1");
    let ts6188 = diagnostic_codes::NUMERIC_SEPARATORS_ARE_NOT_ALLOWED_HERE;
    assert_eq!(
        diags,
        vec![(2, ts6188), (3, ts6188), (4, ts6188)],
        "expected three TS6188 at positions 2,3,4 for `0x___...`",
    );
}

#[test]
fn separator_trailing_double_underscore_emits_one_ts6189() {
    // `0X0110_0110__` — tsc emits TS6189 at the second trailing `_`
    // and tsc's parser dedups the would-be TS6188 trailing-underscore
    // companion at the same byte. Mirror the dedup at the scanner.
    let diags = scan_separator_diagnostics("0X0110_0110__");
    let ts6189 = diagnostic_codes::MULTIPLE_CONSECUTIVE_NUMERIC_SEPARATORS_ARE_NOT_PERMITTED;
    assert_eq!(diags, vec![(12, ts6189)]);
}

#[test]
fn separator_consecutive_after_valid_emits_ts6189() {
    // `0x01__11` — first `_` valid, second `_` is consecutive → TS6189.
    let diags = scan_separator_diagnostics("0x01__11");
    let ts6189 = diagnostic_codes::MULTIPLE_CONSECUTIVE_NUMERIC_SEPARATORS_ARE_NOT_PERMITTED;
    assert_eq!(diags, vec![(5, ts6189)]);
}

#[test]
fn separator_leading_emits_ts6188() {
    // `0x_110` — leading `_` after the prefix is invalid (TS6188).
    let diags = scan_separator_diagnostics("0x_110");
    let ts6188 = diagnostic_codes::NUMERIC_SEPARATORS_ARE_NOT_ALLOWED_HERE;
    assert_eq!(diags, vec![(2, ts6188)]);
}

#[test]
fn separator_trailing_single_underscore_emits_ts6188() {
    // `0x00_` — trailing `_` only fires the post-loop trailing check.
    let diags = scan_separator_diagnostics("0x00_");
    let ts6188 = diagnostic_codes::NUMERIC_SEPARATORS_ARE_NOT_ALLOWED_HERE;
    assert_eq!(diags, vec![(4, ts6188)]);
}

#[test]
fn separator_valid_does_not_emit() {
    // `1_000_000` and `0xFF_FF` are well-formed.
    assert!(scan_separator_diagnostics("1_000_000").is_empty());
    assert!(scan_separator_diagnostics("0xFF_FF").is_empty());
}

#[test]
fn separator_after_leading_zero_emits_ts6188() {
    // `0_0` — separator immediately after a leading zero in an unprefixed
    // numeric literal is a legacy-octal-style pattern that tsc rejects
    // with TS6188 at the `_` position. The literal still tokenizes as a
    // NumericLiteral so the rest of the parse can proceed; only the
    // separator-not-allowed-here diagnostic is added.
    let ts6188 = diagnostic_codes::NUMERIC_SEPARATORS_ARE_NOT_ALLOWED_HERE;
    assert_eq!(scan_separator_diagnostics("0_0"), vec![(1, ts6188)]);
    assert_eq!(scan_separator_diagnostics("0_1"), vec![(1, ts6188)]);
    assert_eq!(scan_separator_diagnostics("0_8"), vec![(1, ts6188)]);
    // Also fires when followed by a fraction or exponent.
    assert_eq!(scan_separator_diagnostics("0_0.5_5"), vec![(1, ts6188)]);
    assert_eq!(scan_separator_diagnostics("0_0e5_5"), vec![(1, ts6188)]);
}

#[test]
fn separator_after_leading_zero_followed_by_double_underscore() {
    // Regression: `parser.numericSeparators.decmialNegative.ts` files 18,
    // 31, 44 (`0__0.0e0`, `0__0.0e+0`, `0__0.0e-0`). The leading-zero
    // TS6188 rule must fire when the `_` after `0` is followed by another
    // `_` (which itself starts a consecutive-separator run that ends in
    // a digit), not just by a digit. tsc emits both TS6188 at the first
    // `_` AND TS6189 at the second `_` for these inputs.
    let ts6188 = diagnostic_codes::NUMERIC_SEPARATORS_ARE_NOT_ALLOWED_HERE;
    let ts6189 = diagnostic_codes::MULTIPLE_CONSECUTIVE_NUMERIC_SEPARATORS_ARE_NOT_PERMITTED;
    assert_eq!(
        scan_separator_diagnostics("0__0.0e0"),
        vec![(1, ts6188), (2, ts6189)]
    );
    assert_eq!(
        scan_separator_diagnostics("0__0.0e+0"),
        vec![(1, ts6188), (2, ts6189)]
    );
    assert_eq!(
        scan_separator_diagnostics("0__0.0e-0"),
        vec![(1, ts6188), (2, ts6189)]
    );
}
