use tsz_scanner::SyntaxKind;
use tsz_scanner::scanner_impl::{RegexFlagErrorKind, ScannerState};

#[test]
fn slash_token_is_lexed_and_re_scanned_as_regex_in_context() {
    let source = "/foo/gim".to_string();
    let mut scanner = ScannerState::new(source, true);

    let token = scanner.scan();
    assert_eq!(token, SyntaxKind::SlashToken);

    let token = scanner.re_scan_slash_token();
    assert_eq!(token, SyntaxKind::RegularExpressionLiteral);
    assert_eq!(scanner.get_token_value(), "/foo/gim");
    assert!(!scanner.is_unterminated());
}

#[test]
fn slash_equals_is_not_promoted_to_regex_by_default_scan() {
    let source = "a /= 2".to_string();
    let mut scanner = ScannerState::new(source, true);

    let _ = scanner.scan(); // identifier
    let token = scanner.scan(); // SlashEqualsToken
    assert_eq!(token, SyntaxKind::SlashEqualsToken);
}

#[test]
fn re_scan_slash_token_reports_duplicate_regex_flags() {
    let source = "/foo/ggi".to_string();
    let mut scanner = ScannerState::new(source, true);

    let _ = scanner.scan();
    let token = scanner.re_scan_slash_token();
    assert_eq!(token, SyntaxKind::RegularExpressionLiteral);
    assert_eq!(scanner.get_token_value(), "/foo/ggi");

    let duplicate_flags = scanner
        .get_regex_flag_errors()
        .iter()
        .filter(|err| matches!(err.kind, RegexFlagErrorKind::Duplicate))
        .count();

    assert_eq!(duplicate_flags, 1);
}

#[test]
fn re_scan_slash_token_reports_invalid_and_incompatible_regex_flags() {
    let source = "/foo/gxu".to_string();
    let mut scanner = ScannerState::new(source, true);

    let _ = scanner.scan();
    let token = scanner.re_scan_slash_token();
    assert_eq!(token, SyntaxKind::RegularExpressionLiteral);
    assert_eq!(scanner.get_token_value(), "/foo/gxu");

    let invalid_flags = scanner
        .get_regex_flag_errors()
        .iter()
        .filter(|err| matches!(err.kind, RegexFlagErrorKind::InvalidFlag))
        .count();
    let incompatible_flags = scanner
        .get_regex_flag_errors()
        .iter()
        .filter(|err| matches!(err.kind, RegexFlagErrorKind::IncompatibleFlags))
        .count();

    assert_eq!(invalid_flags, 1);
    assert_eq!(incompatible_flags, 0);
}

#[test]
fn re_scan_slash_token_detects_incompatible_u_and_v_flags() {
    let source = "/foo/ugv".to_string();
    let mut scanner = ScannerState::new(source, true);

    let _ = scanner.scan();
    let token = scanner.re_scan_slash_token();
    assert_eq!(token, SyntaxKind::RegularExpressionLiteral);
    assert_eq!(scanner.get_token_value(), "/foo/ugv");

    let incompatible_flags = scanner
        .get_regex_flag_errors()
        .iter()
        .filter(|err| matches!(err.kind, RegexFlagErrorKind::IncompatibleFlags))
        .count();

    assert_eq!(incompatible_flags, 1);
}

#[test]
fn re_scan_greater_token_handles_three_token_variants() {
    let source = "x >>>= y".to_string();
    let mut scanner = ScannerState::new(source, true);

    let _ = scanner.scan(); // identifier
    let token = scanner.scan(); // initial >
    assert_eq!(token, SyntaxKind::GreaterThanToken);

    let token = scanner.re_scan_greater_token();
    assert_eq!(
        token,
        SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken
    );
}

#[test]
fn re_scan_greater_token_handles_bitshift_assignment_chain() {
    let source = "a >> b >>> c".to_string();
    let mut scanner = ScannerState::new(source, true);

    let _ = scanner.scan(); // identifier
    let token = scanner.scan(); // first >
    assert_eq!(token, SyntaxKind::GreaterThanToken);
    let token = scanner.re_scan_greater_token();
    assert_eq!(token, SyntaxKind::GreaterThanGreaterThanToken);

    // The scanner should now be positioned after both > characters.
    assert_eq!(scanner.get_token_end(), 4);
}

#[test]
fn template_re_scan_tail_recovers_unterminated_tail() {
    let source = "`a${1 + 2`".to_string();
    let mut scanner = ScannerState::new(source, true);
    let token = scanner.scan();
    assert_eq!(token, SyntaxKind::TemplateHead);

    scanner.set_text("}tail".to_string(), None, None);
    scanner.reset_token_state(0);

    let token = scanner.re_scan_template_token(false);
    assert_eq!(token, SyntaxKind::TemplateTail);
    assert_eq!(scanner.get_token_value(), "tail");
    assert!(scanner.is_unterminated());
}

#[test]
fn scanner_state_snapshot_restore_is_reversible_after_contextual_rescan() {
    let source = "x >>>= y".to_string();
    let mut scanner = ScannerState::new(source, true);

    let _ = scanner.scan(); // identifier
    let snapshot = scanner.save_state();

    let token = scanner.scan(); // initial >
    assert_eq!(token, SyntaxKind::GreaterThanToken);
    assert_eq!(
        scanner.re_scan_greater_token(),
        SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken
    );

    scanner.restore_state(snapshot);
    let token = scanner.scan(); // should return to the same greater-than token path
    assert_eq!(token, SyntaxKind::GreaterThanToken);
}

#[test]
fn template_rescan_handles_unicode_micro_sign() {
    let source = "const value = `${500}µs`;".to_string();
    let mut scanner = ScannerState::new(source, true);

    loop {
        let token = scanner.scan();
        if token == SyntaxKind::TemplateHead || token == SyntaxKind::NoSubstitutionTemplateLiteral {
            break;
        }
        assert!(
            token != SyntaxKind::EndOfFileToken,
            "failed to reach template token"
        );
    }

    let token = scanner.re_scan_template_head_or_no_substitution_template();
    assert_eq!(token, SyntaxKind::TemplateHead);
    assert_eq!(scanner.get_token_value(), "");

    // The parser would scan the expression and then re-scan at `}`.
    scanner.set_text("}µs`".to_string(), None, None);
    scanner.reset_token_state(0);
    let tail = scanner.re_scan_template_token(false);
    assert_eq!(tail, SyntaxKind::TemplateTail);
    assert_eq!(scanner.get_token_value(), "µs");
}

#[test]
fn template_scan_preserves_escape_sequences() {
    let source = "`hello\\nworld`".to_string();
    let mut scanner = ScannerState::new(source, true);
    let token = scanner.scan();
    assert_eq!(token, SyntaxKind::NoSubstitutionTemplateLiteral);
    assert_eq!(scanner.get_token_value(), "hello\nworld");

    let source = "`hello\\${string}`".to_string();
    let mut scanner = ScannerState::new(source, true);
    let token = scanner.scan();
    assert_eq!(token, SyntaxKind::NoSubstitutionTemplateLiteral);
    assert_eq!(scanner.get_token_value(), "hello${string}");
}

#[test]
fn template_no_substitution_handles_unicode_micro_sign() {
    let source = "`500µs`".to_string();
    let mut scanner = ScannerState::new(source, true);
    let token = scanner.scan();
    assert_eq!(token, SyntaxKind::NoSubstitutionTemplateLiteral);
    assert_eq!(scanner.get_token_value(), "500µs");
}

#[test]
fn re_scan_less_than_token_recognizes_closing_tag() {
    let source = "</tag>".to_string();
    let mut scanner = ScannerState::new(source, true);

    let token = scanner.scan();
    assert_eq!(token, SyntaxKind::LessThanToken);

    let token = scanner.re_scan_less_than_token();
    assert_eq!(token, SyntaxKind::LessThanSlashToken);
    assert_eq!(scanner.get_token_value(), "</");
}

#[test]
fn scan_hash_token_promotes_to_private_identifier_directly() {
    let source = "#myField".to_string();
    let mut scanner = ScannerState::new(source, true);

    let token = scanner.scan();
    assert_eq!(token, SyntaxKind::PrivateIdentifier);
    assert_eq!(scanner.get_token_value(), "#myField");
}

#[test]
fn scan_recognizes_regex_at_file_start_via_rescan() {
    let mut scanner = ScannerState::new("/foo/gim".to_string(), true);
    let token = scanner.scan();

    assert_eq!(token, SyntaxKind::SlashToken);
    let rescanned = scanner.re_scan_slash_token();
    assert_eq!(rescanned, SyntaxKind::RegularExpressionLiteral);
    assert_eq!(scanner.get_token_value(), "/foo/gim");
    assert_eq!(scanner.get_token_end(), 8);
}

#[test]
fn re_scan_hash_token_keeps_plain_hash_when_no_identifier_follows() {
    let source = "#".to_string();
    let mut scanner = ScannerState::new(source, true);

    let token = scanner.scan();
    assert_eq!(token, SyntaxKind::HashToken);

    let token = scanner.re_scan_hash_token();
    assert_eq!(token, SyntaxKind::HashToken);
}

#[test]
fn scan_distinguishes_division_and_regular_expression_tokenization() {
    let mut scanner = ScannerState::new("value = 42 / 2;".to_string(), true);

    let _ = scanner.scan(); // value
    let _ = scanner.scan(); // =
    assert_eq!(scanner.scan(), SyntaxKind::NumericLiteral);
    assert_eq!(scanner.scan(), SyntaxKind::SlashToken);
    assert_eq!(scanner.scan(), SyntaxKind::NumericLiteral);
}

#[test]
fn scanner_set_text_slice_scan_window() {
    let mut scanner = ScannerState::new("const ignored = 1;".to_string(), true);
    scanner.set_text("x = 99 + 1".to_string(), Some(4), Some(2));
    scanner.reset_token_state(4);

    let token = scanner.scan();
    assert_eq!(token, SyntaxKind::NumericLiteral);
    assert_eq!(scanner.get_token_value(), "99");
    assert_eq!(scanner.get_token_end(), 6);
}

#[test]
fn re_scan_jsx_token_includes_leading_whitespace() {
    // After parsing a JSX opening element's `>`, the regular scanner skips trivia
    // (newline + spaces) and scans the first identifier. re_scan_jsx_token must
    // reset to full_start_pos (before trivia) so the JSX text node includes
    // leading whitespace and newlines, matching tsc behavior.
    let source = ">\n        hi hi hi!\n    <".to_string();
    let mut scanner = ScannerState::new(source, true);

    // Regular scan: skips trivia, scans "hi" as identifier
    let token = scanner.scan();
    assert_eq!(token, SyntaxKind::GreaterThanToken);
    let token = scanner.scan();
    assert_eq!(token, SyntaxKind::Identifier);
    assert_eq!(scanner.get_token_value(), "hi");

    // Rescan as JSX text: must include leading whitespace from full_start_pos
    let token = scanner.re_scan_jsx_token(true);
    assert_eq!(token, SyntaxKind::JsxText);
    assert_eq!(
        scanner.get_token_value(),
        "\n        hi hi hi!\n    ",
        "JSX text should include leading whitespace/newlines from full_start_pos"
    );
}

#[test]
fn re_scan_jsx_token_clears_stale_identifier_atom() {
    // After scanning an identifier, token_atom is set. re_scan_jsx_token must
    // clear token_atom so get_token_value_ref() returns the JSX text, not
    // the stale identifier string.
    let source = ">\n    text\n<".to_string();
    let mut scanner = ScannerState::new(source, true);

    scanner.scan(); // >
    scanner.scan(); // "text" as identifier

    let token = scanner.re_scan_jsx_token(true);
    assert_eq!(token, SyntaxKind::JsxText);
    // get_token_value uses get_token_value_ref internally — both must return JSX text
    let value = scanner.get_token_value();
    assert!(
        value.contains("text"),
        "get_token_value should return JSX text, not stale identifier"
    );
    assert!(
        value.starts_with('\n'),
        "JSX text should include leading newline: {value:?}"
    );
}

#[test]
fn re_scan_jsx_token_single_line_text() {
    // JSX text without leading trivia should also work correctly
    let source = ">hello world<".to_string();
    let mut scanner = ScannerState::new(source, true);

    scanner.scan(); // >
    scanner.scan(); // "hello" as identifier

    let token = scanner.re_scan_jsx_token(true);
    assert_eq!(token, SyntaxKind::JsxText);
    assert_eq!(scanner.get_token_value(), "hello world");
}

#[test]
fn test_template_rescan_invalid_hex_escape() {
    use tsz_scanner::SyntaxKind;
    use tsz_scanner::scanner_impl::{ScannerState, TokenFlags};

    // Template tail with invalid hex escape
    let source = r#"}\xtraordinary`"#;
    let mut scanner = ScannerState::new(source.to_string(), true);

    // First scan the }
    scanner.scan();
    assert_eq!(scanner.get_token(), SyntaxKind::CloseBraceToken);

    // Now re-scan for template tail
    let token = scanner.re_scan_template_token(false);

    // Should be TemplateTail
    assert_eq!(token, SyntaxKind::TemplateTail);

    // Should have ContainsInvalidEscape flag
    let flags = scanner.get_token_flags();
    let has_invalid = (flags & TokenFlags::ContainsInvalidEscape as u32) != 0;

    assert!(
        has_invalid,
        "TemplateTail should have ContainsInvalidEscape flag. Flags={}, text={:?}",
        flags,
        scanner.get_token_text_ref()
    );
}

// --- Empty-exponent / identifier-after-numeric scanner diagnostics --------
//
// Mirrors tsc's `scanNumber` exponent branch: a numeric literal whose
// exponent has no digits (`1e+`, `1ee`, `3en`, ...) emits TS1124 right after
// the `e` (or sign), and tsc's `parseErrorAtPosition` same-start dedup
// suppresses any TS1351 that would fire at the same position. We verify the
// scanner emits those diagnostics in the same shape so the merged
// fingerprint matches tsc on `identifierStartAfterNumericLiteral.ts`.

fn scan_first_token(source: &str) -> Vec<(usize, u32)> {
    let mut scanner = ScannerState::new(source.to_string(), true);
    let _ = scanner.scan();
    scanner
        .get_scanner_diagnostics()
        .iter()
        .map(|d| (d.pos, d.code))
        .collect()
}

#[test]
fn scan_number_empty_exponent_emits_ts1124_after_e() {
    // `1e` — exponent has no digit. tsc emits TS1124 at pos=2 (right after
    // the `e`, before EOF). No TS1351 because there is no following
    // identifier.
    let diags = scan_first_token("1e");
    assert!(
        diags.iter().any(|(pos, code)| *pos == 2 && *code == 1124),
        "expected TS1124 at pos=2 for `1e`, got {diags:?}",
    );
    assert!(
        !diags.iter().any(|(_, code)| *code == 1351),
        "should not emit TS1351 for `1e`, got {diags:?}",
    );
}

#[test]
fn scan_number_exponent_with_sign_no_digit_emits_ts1124_after_sign() {
    // `1e+` — sign consumed, no digit. tsc fires TS1124 at the position
    // right after the sign (pos=3).
    let diags = scan_first_token("1e+");
    assert!(
        diags.iter().any(|(pos, code)| *pos == 3 && *code == 1124),
        "expected TS1124 at pos=3 for `1e+`, got {diags:?}",
    );
}

#[test]
fn scan_number_double_e_keeps_only_ts1124_dedup_ts1351() {
    // `1ee` — exponent has no digit, but the trailing char is an identifier
    // start. tsc's lastError-by-start dedup drops the TS1351 that would fire
    // at the same position as the TS1124 emitted by the empty exponent.
    let diags = scan_first_token("1ee");
    assert!(
        diags.iter().any(|(pos, code)| *pos == 2 && *code == 1124),
        "expected TS1124 at pos=2 for `1ee`, got {diags:?}",
    );
    assert!(
        !diags.iter().any(|(_, code)| *code == 1351),
        "TS1351 must be deduped at same position as TS1124 for `1ee`, got {diags:?}",
    );
}

#[test]
fn scan_number_exponent_followed_by_n_keeps_ts1124_and_ts1352() {
    // `3en` — exponent has no digit, then bigint suffix `n`. tsc emits both
    // TS1124 (after `e`) and TS1352 (bigint with exponential), at distinct
    // positions, so no dedup applies.
    let diags = scan_first_token("3en");
    assert!(
        diags.iter().any(|(pos, code)| *pos == 2 && *code == 1124),
        "expected TS1124 at pos=2 for `3en`, got {diags:?}",
    );
    assert!(
        diags.iter().any(|(_, code)| *code == 1352),
        "expected TS1352 (bigint exponential) for `3en`, got {diags:?}",
    );
    assert!(
        !diags.iter().any(|(_, code)| *code == 1351),
        "should not emit TS1351 for `3en` (bigint branch wins), got {diags:?}",
    );
}

#[test]
fn scan_number_well_formed_decimal_with_exponent_emits_no_diagnostic() {
    // Sanity: `1e9` is fully valid; no diagnostics expected from the
    // scanner's empty-exponent path.
    let diags = scan_first_token("1e9");
    assert!(
        !diags.iter().any(|(_, code)| *code == 1124),
        "well-formed `1e9` must not emit TS1124, got {diags:?}",
    );
    assert!(
        !diags.iter().any(|(_, code)| *code == 1351),
        "well-formed `1e9` must not emit TS1351, got {diags:?}",
    );
}
