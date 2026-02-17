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
