//! Template-literal line-terminator cooked-value parity tests.
//!
//! ECMAScript TV semantics (ES6 11.8.6.1) and tsc's
//! `scanTemplateAndSetTokenValue` normalize `<CR>` and `<CR><LF>` line
//! terminators inside template literals to `<LF>` in the cooked value, and
//! never set `PrecedingLineBreak` for line breaks *inside* a template (that
//! flag is trivia-only). The live first-scan path and the re-scan path share
//! one implementation, so all four template token positions must agree.

use tsz_scanner::SyntaxKind;
use tsz_scanner::scanner_impl::ScannerState;

#[test]
fn no_substitution_template_normalizes_crlf_and_lone_cr() {
    let source = "`x\r\ny\rz`".to_string();
    let mut scanner = ScannerState::new(source, true);
    let token = scanner.scan();
    assert_eq!(token, SyntaxKind::NoSubstitutionTemplateLiteral);
    assert_eq!(scanner.get_token_value(), "x\ny\nz");
    assert!(!scanner.is_unterminated());
}

#[test]
fn template_head_middle_tail_normalize_line_terminators() {
    // Real parser protocol: head from scan(), middle/tail via
    // re_scan_template_token at each closing `}`.
    let source = "`h\r\n${a}m\r${b}t\r\n`".to_string();
    let mut scanner = ScannerState::new(source, true);

    let token = scanner.scan();
    assert_eq!(token, SyntaxKind::TemplateHead);
    assert_eq!(scanner.get_token_value(), "h\n");

    let token = scanner.scan();
    assert_eq!(token, SyntaxKind::Identifier);
    let token = scanner.scan();
    assert_eq!(token, SyntaxKind::CloseBraceToken);

    let token = scanner.re_scan_template_token(false);
    assert_eq!(token, SyntaxKind::TemplateMiddle);
    assert_eq!(scanner.get_token_value(), "m\n");

    let token = scanner.scan();
    assert_eq!(token, SyntaxKind::Identifier);
    let token = scanner.scan();
    assert_eq!(token, SyntaxKind::CloseBraceToken);

    let token = scanner.re_scan_template_token(false);
    assert_eq!(token, SyntaxKind::TemplateTail);
    assert_eq!(scanner.get_token_value(), "t\n");
    assert!(!scanner.is_unterminated());
}

#[test]
fn template_head_crlf_only_part_cooks_to_single_lf() {
    let source = "`\r\n${x}`".to_string();
    let mut scanner = ScannerState::new(source, true);
    let token = scanner.scan();
    assert_eq!(token, SyntaxKind::TemplateHead);
    assert_eq!(scanner.get_token_value(), "\n");
}

#[test]
fn escaped_carriage_return_line_continuations_cook_to_empty() {
    // `\` + <CR><LF> and `\` + <CR> are LineContinuations: cooked value "".
    let source = "`a\\\r\nb`".to_string();
    let mut scanner = ScannerState::new(source, true);
    let token = scanner.scan();
    assert_eq!(token, SyntaxKind::NoSubstitutionTemplateLiteral);
    assert_eq!(scanner.get_token_value(), "ab");

    let source = "`a\\\rb`".to_string();
    let mut scanner = ScannerState::new(source, true);
    let token = scanner.scan();
    assert_eq!(token, SyntaxKind::NoSubstitutionTemplateLiteral);
    assert_eq!(scanner.get_token_value(), "ab");
}

#[test]
fn line_breaks_inside_template_do_not_set_preceding_line_break() {
    // tsc sets PrecedingLineBreak only for trivia before a token, never for
    // line terminators inside a template literal.
    for source in ["`a\r\nb`", "`a\rb`", "`a\nb`"] {
        let mut scanner = ScannerState::new(source.to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::NoSubstitutionTemplateLiteral);
        assert!(
            !scanner.has_preceding_line_break(),
            "line break inside template must not set PrecedingLineBreak: {source:?}"
        );
    }
}

#[test]
fn trivia_line_break_before_template_still_sets_preceding_line_break() {
    let source = ";\n`a`".to_string();
    let mut scanner = ScannerState::new(source, true);
    let token = scanner.scan();
    assert_eq!(token, SyntaxKind::SemicolonToken);
    let token = scanner.scan();
    assert_eq!(token, SyntaxKind::NoSubstitutionTemplateLiteral);
    assert!(scanner.has_preceding_line_break());
}

#[test]
fn unterminated_template_recovery_normalizes_and_flags() {
    let source = "`abc\r\ndef".to_string();
    let mut scanner = ScannerState::new(source, true);
    let token = scanner.scan();
    assert_eq!(token, SyntaxKind::NoSubstitutionTemplateLiteral);
    assert!(scanner.is_unterminated());
    assert!(scanner.has_preceding_line_break());
    assert_eq!(scanner.get_token_value(), "abc\ndef");
}

#[test]
fn template_token_positions_unchanged_by_shared_scan_path() {
    // `h\r\n${a}` — head token spans the backtick through `${`.
    let source = "`h\r\n${a}t`".to_string();
    let mut scanner = ScannerState::new(source, true);
    let token = scanner.scan();
    assert_eq!(token, SyntaxKind::TemplateHead);
    assert_eq!(scanner.get_token_start(), 0);
    assert_eq!(scanner.get_token_end(), 6); // after `${`
}
