mod punctuation_scanning {
    use super::*;

    #[test]
    fn all_single_char_punctuation() {
        let cases = vec![
            ("{", SyntaxKind::OpenBraceToken),
            ("}", SyntaxKind::CloseBraceToken),
            ("(", SyntaxKind::OpenParenToken),
            (")", SyntaxKind::CloseParenToken),
            ("[", SyntaxKind::OpenBracketToken),
            ("]", SyntaxKind::CloseBracketToken),
            (";", SyntaxKind::SemicolonToken),
            (",", SyntaxKind::CommaToken),
            ("~", SyntaxKind::TildeToken),
            ("@", SyntaxKind::AtToken),
            (":", SyntaxKind::ColonToken),
        ];
        for (source, expected) in cases {
            let (kind, _) = scan_single(source);
            assert_eq!(kind, expected, "Failed for source: {source}");
        }
    }

    #[test]
    fn dot_variants() {
        assert_eq!(scan_all_tokens(".")[0], SyntaxKind::DotToken);
        assert_eq!(scan_all_tokens("...")[0], SyntaxKind::DotDotDotToken);
    }

    #[test]
    fn equals_variants() {
        assert_eq!(scan_all_tokens("=")[0], SyntaxKind::EqualsToken);
        assert_eq!(scan_all_tokens("==")[0], SyntaxKind::EqualsEqualsToken);
        assert_eq!(
            scan_all_tokens("===")[0],
            SyntaxKind::EqualsEqualsEqualsToken
        );
        assert_eq!(scan_all_tokens("=>")[0], SyntaxKind::EqualsGreaterThanToken);
    }

    #[test]
    fn exclamation_variants() {
        assert_eq!(scan_all_tokens("!")[0], SyntaxKind::ExclamationToken);
        assert_eq!(scan_all_tokens("!=")[0], SyntaxKind::ExclamationEqualsToken);
        assert_eq!(
            scan_all_tokens("!==")[0],
            SyntaxKind::ExclamationEqualsEqualsToken
        );
    }

    #[test]
    fn plus_variants() {
        assert_eq!(scan_all_tokens("+")[0], SyntaxKind::PlusToken);
        assert_eq!(scan_all_tokens("++")[0], SyntaxKind::PlusPlusToken);
        assert_eq!(scan_all_tokens("+=")[0], SyntaxKind::PlusEqualsToken);
    }

    #[test]
    fn minus_variants() {
        assert_eq!(scan_all_tokens("-")[0], SyntaxKind::MinusToken);
        assert_eq!(scan_all_tokens("--")[0], SyntaxKind::MinusMinusToken);
        assert_eq!(scan_all_tokens("-=")[0], SyntaxKind::MinusEqualsToken);
    }

    #[test]
    fn asterisk_variants() {
        assert_eq!(scan_all_tokens("*")[0], SyntaxKind::AsteriskToken);
        assert_eq!(scan_all_tokens("**")[0], SyntaxKind::AsteriskAsteriskToken);
        assert_eq!(scan_all_tokens("*=")[0], SyntaxKind::AsteriskEqualsToken);
        assert_eq!(
            scan_all_tokens("**=")[0],
            SyntaxKind::AsteriskAsteriskEqualsToken
        );
    }

    #[test]
    fn percent_variants() {
        assert_eq!(scan_all_tokens("%")[0], SyntaxKind::PercentToken);
        assert_eq!(scan_all_tokens("%=")[0], SyntaxKind::PercentEqualsToken);
    }

    #[test]
    fn ampersand_variants() {
        assert_eq!(scan_all_tokens("&")[0], SyntaxKind::AmpersandToken);
        assert_eq!(
            scan_all_tokens("&&")[0],
            SyntaxKind::AmpersandAmpersandToken
        );
        assert_eq!(scan_all_tokens("&=")[0], SyntaxKind::AmpersandEqualsToken);
        assert_eq!(
            scan_all_tokens("&&=")[0],
            SyntaxKind::AmpersandAmpersandEqualsToken
        );
    }

    #[test]
    fn bar_variants() {
        assert_eq!(scan_all_tokens("|")[0], SyntaxKind::BarToken);
        assert_eq!(scan_all_tokens("||")[0], SyntaxKind::BarBarToken);
        assert_eq!(scan_all_tokens("|=")[0], SyntaxKind::BarEqualsToken);
        assert_eq!(scan_all_tokens("||=")[0], SyntaxKind::BarBarEqualsToken);
    }

    #[test]
    fn caret_variants() {
        assert_eq!(scan_all_tokens("^")[0], SyntaxKind::CaretToken);
        assert_eq!(scan_all_tokens("^=")[0], SyntaxKind::CaretEqualsToken);
    }

    #[test]
    fn question_variants() {
        assert_eq!(scan_all_tokens("?")[0], SyntaxKind::QuestionToken);
        assert_eq!(scan_all_tokens("??")[0], SyntaxKind::QuestionQuestionToken);
        assert_eq!(scan_all_tokens("?.")[0], SyntaxKind::QuestionDotToken);
        assert_eq!(
            scan_all_tokens("??=")[0],
            SyntaxKind::QuestionQuestionEqualsToken
        );
    }

    #[test]
    fn question_dot_not_before_digit() {
        // ?.5 should be ? and .5 (number), not QuestionDotToken
        let tokens = scan_all_tokens("?.5");
        assert_eq!(tokens[0], SyntaxKind::QuestionToken);
        assert_eq!(tokens[1], SyntaxKind::NumericLiteral);
    }

    #[test]
    fn less_than_variants() {
        assert_eq!(scan_all_tokens("<")[0], SyntaxKind::LessThanToken);
        assert_eq!(scan_all_tokens("<=")[0], SyntaxKind::LessThanEqualsToken);
        assert_eq!(scan_all_tokens("<<")[0], SyntaxKind::LessThanLessThanToken);
        assert_eq!(
            scan_all_tokens("<<=")[0],
            SyntaxKind::LessThanLessThanEqualsToken
        );
    }

    #[test]
    fn greater_than_only_single_on_scan() {
        // The scanner always returns GreaterThanToken for >
        // The parser calls reScanGreaterToken() to get compound tokens
        assert_eq!(scan_all_tokens(">")[0], SyntaxKind::GreaterThanToken);
        // >> also scans as > then >
        let tokens = scan_all_tokens(">>");
        assert_eq!(tokens[0], SyntaxKind::GreaterThanToken);
    }

    #[test]
    fn slash_variants() {
        assert_eq!(scan_all_tokens("/")[0], SyntaxKind::SlashToken);
        assert_eq!(scan_all_tokens("/=")[0], SyntaxKind::SlashEqualsToken);
    }
}

mod whitespace_scanning {
    use super::*;

    #[test]
    fn whitespace_skipped_in_skip_trivia_mode() {
        let mut scanner = ScannerState::new("   foo".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::Identifier);
        assert_eq!(scanner.get_token_value(), "foo");
    }

    #[test]
    fn whitespace_returned_in_non_skip_mode() {
        let mut scanner = ScannerState::new("   foo".to_string(), false);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::WhitespaceTrivia);
    }

    #[test]
    fn newline_returned_in_non_skip_mode() {
        let mut scanner = ScannerState::new("\nfoo".to_string(), false);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::NewLineTrivia);
    }

    #[test]
    fn crlf_treated_as_single_newline() {
        let mut scanner = ScannerState::new("\r\nfoo".to_string(), false);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::NewLineTrivia);
        // After the newline trivia, position should be past both \r and \n
        assert_eq!(scanner.get_token_end(), 2);
    }

    #[test]
    fn preceding_line_break_flag() {
        let mut scanner = ScannerState::new("\nfoo".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::Identifier);
        assert!(scanner.has_preceding_line_break());
    }

    #[test]
    fn no_preceding_line_break_on_same_line() {
        let mut scanner = ScannerState::new("foo bar".to_string(), true);
        scanner.scan(); // foo
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::Identifier);
        assert!(!scanner.has_preceding_line_break());
    }

    #[test]
    fn tab_is_whitespace() {
        let mut scanner = ScannerState::new("\tfoo".to_string(), false);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::WhitespaceTrivia);
    }

    #[test]
    fn eof_on_empty_input() {
        let mut scanner = ScannerState::new("".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::EndOfFileToken);
    }

    #[test]
    fn eof_after_all_tokens() {
        let mut scanner = ScannerState::new("x".to_string(), true);
        scanner.scan(); // x
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::EndOfFileToken);
    }
}

mod position_tracking {
    use super::*;

    #[test]
    fn token_positions_simple() {
        let mut scanner = ScannerState::new("foo bar".to_string(), true);
        scanner.scan();
        assert_eq!(scanner.get_token_start(), 0);
        assert_eq!(scanner.get_token_end(), 3);

        scanner.scan();
        assert_eq!(scanner.get_token_start(), 4);
        assert_eq!(scanner.get_token_end(), 7);
    }

    #[test]
    fn full_start_includes_trivia() {
        let mut scanner = ScannerState::new("  foo".to_string(), true);
        scanner.scan();
        assert_eq!(scanner.get_token_full_start(), 0);
        assert_eq!(scanner.get_token_start(), 2);
        assert_eq!(scanner.get_token_end(), 5);
    }

    #[test]
    fn token_text_matches_source() {
        let mut scanner = ScannerState::new("foo + bar".to_string(), true);
        scanner.scan(); // foo
        assert_eq!(scanner.get_token_text(), "foo");
        scanner.scan(); // +
        assert_eq!(scanner.get_token_text(), "+");
        scanner.scan(); // bar
        assert_eq!(scanner.get_token_text(), "bar");
    }
}

mod rescan_methods {
    use super::*;

    #[test]
    fn rescan_greater_single() {
        let mut scanner = ScannerState::new("x > y".to_string(), true);
        scanner.scan(); // x
        scanner.scan(); // >
        let token = scanner.re_scan_greater_token();
        // No chars follow >, so it stays as GreaterThanToken
        assert_eq!(token, SyntaxKind::GreaterThanToken);
    }

    #[test]
    fn rescan_greater_equals() {
        let mut scanner = ScannerState::new("x >= y".to_string(), true);
        scanner.scan(); // x
        scanner.scan(); // >
        let token = scanner.re_scan_greater_token();
        assert_eq!(token, SyntaxKind::GreaterThanEqualsToken);
    }

    #[test]
    fn rescan_greater_shift_right() {
        let mut scanner = ScannerState::new("x >> y".to_string(), true);
        scanner.scan(); // x
        scanner.scan(); // >
        let token = scanner.re_scan_greater_token();
        assert_eq!(token, SyntaxKind::GreaterThanGreaterThanToken);
    }

    #[test]
    fn rescan_greater_unsigned_shift_right() {
        let mut scanner = ScannerState::new("x >>> y".to_string(), true);
        scanner.scan(); // x
        scanner.scan(); // >
        let token = scanner.re_scan_greater_token();
        assert_eq!(token, SyntaxKind::GreaterThanGreaterThanGreaterThanToken);
    }

    #[test]
    fn rescan_greater_shift_right_assign() {
        let mut scanner = ScannerState::new("x >>= y".to_string(), true);
        scanner.scan(); // x
        scanner.scan(); // >
        let token = scanner.re_scan_greater_token();
        assert_eq!(token, SyntaxKind::GreaterThanGreaterThanEqualsToken);
    }

    #[test]
    fn rescan_asterisk_equals() {
        let mut scanner = ScannerState::new("*=".to_string(), true);
        scanner.scan(); // *=
        assert_eq!(scanner.get_token(), SyntaxKind::AsteriskEqualsToken);
        let token = scanner.re_scan_asterisk_equals_token();
        assert_eq!(token, SyntaxKind::EqualsToken);
    }

    #[test]
    fn rescan_less_than_slash() {
        let mut scanner = ScannerState::new("</tag>".to_string(), true);
        scanner.scan(); // <
        let token = scanner.re_scan_less_than_token();
        assert_eq!(token, SyntaxKind::LessThanSlashToken);
    }

    #[test]
    fn rescan_question_dot() {
        let mut scanner = ScannerState::new("?.foo".to_string(), true);
        scanner.scan(); // gets QuestionDotToken directly
        // But let's test re_scan_question_token from QuestionToken
        let mut scanner = ScannerState::new("?".to_string(), true);
        scanner.scan();
        let token = scanner.re_scan_question_token();
        assert_eq!(token, SyntaxKind::QuestionToken); // nothing follows, stays ?
    }
}
