mod state_management {
    use super::*;

    #[test]
    fn save_restore_basic() {
        let mut scanner = ScannerState::new("a b c".to_string(), true);
        scanner.scan(); // a
        let snapshot = scanner.save_state();
        scanner.scan(); // b
        assert_eq!(scanner.get_token_value(), "b");
        scanner.restore_state(snapshot);
        let token = scanner.scan(); // should get b again
        assert_eq!(token, SyntaxKind::Identifier);
        assert_eq!(scanner.get_token_value(), "b");
    }

    #[test]
    fn set_text_replaces_source() {
        let mut scanner = ScannerState::new("old text".to_string(), true);
        scanner.set_text("fresh text".to_string(), None, None);
        scanner.reset_token_state(0);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::Identifier);
        assert_eq!(scanner.get_token_value(), "fresh");
    }

    #[test]
    fn set_text_with_offset_and_length() {
        let mut scanner = ScannerState::new("".to_string(), true);
        scanner.set_text("xxFOOxx".to_string(), Some(2), Some(3));
        scanner.reset_token_state(2);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::Identifier);
        // FOO is scanned from offset 2 with length 3
        assert_eq!(scanner.get_token_value(), "FOO");
    }

    #[test]
    fn reset_token_state_clears_flags() {
        let mut scanner = ScannerState::new("\nfoo".to_string(), true);
        scanner.scan(); // foo - has PrecedingLineBreak
        assert!(scanner.has_preceding_line_break());
        scanner.reset_token_state(0);
        // After reset, flags are cleared
        assert!(!scanner.has_preceding_line_break());
    }
}
mod shebang_scanning {
    use super::*;

    #[test]
    fn shebang_at_start() {
        let mut scanner = ScannerState::new("#!/usr/bin/env node\nvar x".to_string(), true);
        let len = scanner.scan_shebang_trivia();
        assert!(len > 0);
        // After shebang, scanner position should be past the shebang + newline
        assert_eq!(scanner.get_pos(), 20); // "#!/usr/bin/env node\n" = 20 chars
    }

    #[test]
    fn no_shebang_returns_zero() {
        let mut scanner = ScannerState::new("var x".to_string(), true);
        let len = scanner.scan_shebang_trivia();
        assert_eq!(len, 0);
    }

    #[test]
    fn shebang_not_at_start_returns_zero() {
        let mut scanner = ScannerState::new("var x".to_string(), true);
        scanner.scan(); // advance past "var"
        // If we try to scan shebang now, it won't be at pos 0
        let len = scanner.scan_shebang_trivia();
        assert_eq!(len, 0);
    }

    #[test]
    fn shebang_with_crlf() {
        let mut scanner = ScannerState::new("#!/usr/bin/env node\r\nvar x".to_string(), true);
        let len = scanner.scan_shebang_trivia();
        assert!(len > 0);
        assert_eq!(scanner.get_pos(), 21); // includes \r\n
    }

    #[test]
    fn shebang_at_eof() {
        let mut scanner = ScannerState::new("#!/usr/bin/env node".to_string(), true);
        let len = scanner.scan_shebang_trivia();
        assert!(len > 0);
    }
}
mod full_token_stream {
    use super::*;

    #[test]
    fn variable_declaration() {
        let tokens = scan_all_tokens("const x = 42;");
        assert_eq!(tokens[0], SyntaxKind::ConstKeyword);
        assert_eq!(tokens[1], SyntaxKind::Identifier);
        assert_eq!(tokens[2], SyntaxKind::EqualsToken);
        assert_eq!(tokens[3], SyntaxKind::NumericLiteral);
        assert_eq!(tokens[4], SyntaxKind::SemicolonToken);
        assert_eq!(tokens[5], SyntaxKind::EndOfFileToken);
    }

    #[test]
    fn function_declaration() {
        let tokens = scan_all_tokens("function foo(a, b) { return a + b; }");
        assert_eq!(tokens[0], SyntaxKind::FunctionKeyword);
        assert_eq!(tokens[1], SyntaxKind::Identifier); // foo
        assert_eq!(tokens[2], SyntaxKind::OpenParenToken);
        assert_eq!(tokens[3], SyntaxKind::Identifier); // a
        assert_eq!(tokens[4], SyntaxKind::CommaToken);
        assert_eq!(tokens[5], SyntaxKind::Identifier); // b
        assert_eq!(tokens[6], SyntaxKind::CloseParenToken);
        assert_eq!(tokens[7], SyntaxKind::OpenBraceToken);
        assert_eq!(tokens[8], SyntaxKind::ReturnKeyword);
        assert_eq!(tokens[9], SyntaxKind::Identifier); // a
        assert_eq!(tokens[10], SyntaxKind::PlusToken);
        assert_eq!(tokens[11], SyntaxKind::Identifier); // b
        assert_eq!(tokens[12], SyntaxKind::SemicolonToken);
        assert_eq!(tokens[13], SyntaxKind::CloseBraceToken);
        assert_eq!(tokens[14], SyntaxKind::EndOfFileToken);
    }

    #[test]
    fn arrow_function() {
        let tokens = scan_all_tokens("(x) => x * 2");
        assert_eq!(tokens[0], SyntaxKind::OpenParenToken);
        assert_eq!(tokens[1], SyntaxKind::Identifier); // x
        assert_eq!(tokens[2], SyntaxKind::CloseParenToken);
        assert_eq!(tokens[3], SyntaxKind::EqualsGreaterThanToken);
        assert_eq!(tokens[4], SyntaxKind::Identifier); // x
        assert_eq!(tokens[5], SyntaxKind::AsteriskToken);
        assert_eq!(tokens[6], SyntaxKind::NumericLiteral);
        assert_eq!(tokens[7], SyntaxKind::EndOfFileToken);
    }

    #[test]
    fn string_and_template_mix() {
        let tokens = scan_all_tokens(r#"let s = "hello" + `world`"#);
        assert_eq!(tokens[0], SyntaxKind::LetKeyword);
        assert_eq!(tokens[1], SyntaxKind::Identifier); // s
        assert_eq!(tokens[2], SyntaxKind::EqualsToken);
        assert_eq!(tokens[3], SyntaxKind::StringLiteral);
        assert_eq!(tokens[4], SyntaxKind::PlusToken);
        assert_eq!(tokens[5], SyntaxKind::NoSubstitutionTemplateLiteral);
    }

    #[test]
    fn typescript_type_annotation() {
        let tokens = scan_all_tokens("let x: number = 5;");
        assert_eq!(tokens[0], SyntaxKind::LetKeyword);
        assert_eq!(tokens[1], SyntaxKind::Identifier); // x
        assert_eq!(tokens[2], SyntaxKind::ColonToken);
        assert_eq!(tokens[3], SyntaxKind::NumberKeyword);
        assert_eq!(tokens[4], SyntaxKind::EqualsToken);
        assert_eq!(tokens[5], SyntaxKind::NumericLiteral);
        assert_eq!(tokens[6], SyntaxKind::SemicolonToken);
    }

    #[test]
    fn optional_chaining() {
        let tokens = scan_all_tokens("a?.b?.c");
        assert_eq!(tokens[0], SyntaxKind::Identifier); // a
        assert_eq!(tokens[1], SyntaxKind::QuestionDotToken);
        assert_eq!(tokens[2], SyntaxKind::Identifier); // b
        assert_eq!(tokens[3], SyntaxKind::QuestionDotToken);
        assert_eq!(tokens[4], SyntaxKind::Identifier); // c
    }

    #[test]
    fn nullish_coalescing() {
        let tokens = scan_all_tokens("a ?? b");
        assert_eq!(tokens[0], SyntaxKind::Identifier);
        assert_eq!(tokens[1], SyntaxKind::QuestionQuestionToken);
        assert_eq!(tokens[2], SyntaxKind::Identifier);
    }

    #[test]
    fn spread_operator() {
        let tokens = scan_all_tokens("...args");
        assert_eq!(tokens[0], SyntaxKind::DotDotDotToken);
        assert_eq!(tokens[1], SyntaxKind::Identifier);
    }

    #[test]
    fn class_with_private_field() {
        let tokens = scan_all_tokens("class Foo { #bar = 1; }");
        assert_eq!(tokens[0], SyntaxKind::ClassKeyword);
        assert_eq!(tokens[1], SyntaxKind::Identifier); // Foo
        assert_eq!(tokens[2], SyntaxKind::OpenBraceToken);
        assert_eq!(tokens[3], SyntaxKind::PrivateIdentifier); // #bar
        assert_eq!(tokens[4], SyntaxKind::EqualsToken);
        assert_eq!(tokens[5], SyntaxKind::NumericLiteral);
        assert_eq!(tokens[6], SyntaxKind::SemicolonToken);
        assert_eq!(tokens[7], SyntaxKind::CloseBraceToken);
    }

    #[test]
    fn all_contextual_keywords() {
        let keywords = vec![
            ("abstract", SyntaxKind::AbstractKeyword),
            ("accessor", SyntaxKind::AccessorKeyword),
            ("as", SyntaxKind::AsKeyword),
            ("asserts", SyntaxKind::AssertsKeyword),
            ("assert", SyntaxKind::AssertKeyword),
            ("any", SyntaxKind::AnyKeyword),
            ("async", SyntaxKind::AsyncKeyword),
            ("await", SyntaxKind::AwaitKeyword),
            ("boolean", SyntaxKind::BooleanKeyword),
            ("constructor", SyntaxKind::ConstructorKeyword),
            ("declare", SyntaxKind::DeclareKeyword),
            ("get", SyntaxKind::GetKeyword),
            ("infer", SyntaxKind::InferKeyword),
            ("intrinsic", SyntaxKind::IntrinsicKeyword),
            ("is", SyntaxKind::IsKeyword),
            ("keyof", SyntaxKind::KeyOfKeyword),
            ("module", SyntaxKind::ModuleKeyword),
            ("namespace", SyntaxKind::NamespaceKeyword),
            ("never", SyntaxKind::NeverKeyword),
            ("out", SyntaxKind::OutKeyword),
            ("readonly", SyntaxKind::ReadonlyKeyword),
            ("require", SyntaxKind::RequireKeyword),
            ("number", SyntaxKind::NumberKeyword),
            ("object", SyntaxKind::ObjectKeyword),
            ("satisfies", SyntaxKind::SatisfiesKeyword),
            ("set", SyntaxKind::SetKeyword),
            ("string", SyntaxKind::StringKeyword),
            ("symbol", SyntaxKind::SymbolKeyword),
            ("type", SyntaxKind::TypeKeyword),
            ("undefined", SyntaxKind::UndefinedKeyword),
            ("unique", SyntaxKind::UniqueKeyword),
            ("unknown", SyntaxKind::UnknownKeyword),
            ("using", SyntaxKind::UsingKeyword),
            ("from", SyntaxKind::FromKeyword),
            ("global", SyntaxKind::GlobalKeyword),
            ("bigint", SyntaxKind::BigIntKeyword),
            ("override", SyntaxKind::OverrideKeyword),
            ("of", SyntaxKind::OfKeyword),
            ("defer", SyntaxKind::DeferKeyword),
        ];
        for (text, expected_kind) in keywords {
            let (kind, _) = scan_single(text);
            assert_eq!(kind, expected_kind, "Failed for keyword: {text}");
        }
    }
}
mod jsx_scanning {
    use super::*;

    #[test]
    fn jsx_identifier_with_hyphen() {
        let mut scanner = ScannerState::new("data-testid".to_string(), true);
        scanner.scan(); // scans "data" initially
        // Now call scan_jsx_identifier to extend with hyphenated parts
        scanner.scan_jsx_identifier();
        assert_eq!(scanner.get_token(), SyntaxKind::Identifier);
        assert_eq!(scanner.get_token_value(), "data-testid");
    }

    #[test]
    fn jsx_identifier_with_hyphen_digit_part() {
        let mut scanner = ScannerState::new("data-123".to_string(), true);
        scanner.scan(); // scans "data" initially
        scanner.scan_jsx_identifier();
        assert_eq!(scanner.get_token(), SyntaxKind::Identifier);
        assert_eq!(scanner.get_token_value(), "data-123");
    }

    #[test]
    fn jsx_text_scanning() {
        let mut scanner = ScannerState::new(">hello world<".to_string(), true);
        scanner.scan(); // >
        scanner.scan(); // scans "hello" as identifier
        let token = scanner.re_scan_jsx_token(true);
        assert_eq!(token, SyntaxKind::JsxText);
        assert_eq!(scanner.get_token_value(), "hello world");
    }

    #[test]
    fn jsx_attribute_double_quoted() {
        let mut scanner = ScannerState::new(r#""value""#.to_string(), true);
        let token = scanner.scan_jsx_attribute_value();
        assert_eq!(token, SyntaxKind::StringLiteral);
        assert_eq!(scanner.get_token_value(), "value");
    }

    #[test]
    fn jsx_attribute_single_quoted() {
        let mut scanner = ScannerState::new("'value'".to_string(), true);
        let token = scanner.scan_jsx_attribute_value();
        assert_eq!(token, SyntaxKind::StringLiteral);
        assert_eq!(scanner.get_token_value(), "value");
    }

    #[test]
    fn jsx_attribute_double_quoted_two_byte_utf8() {
        let mut scanner = ScannerState::new(r#""é""#.to_string(), true);
        let token = scanner.scan_jsx_attribute_value();
        assert_eq!(token, SyntaxKind::StringLiteral);
        assert_eq!(scanner.get_token_value(), "é");
    }

    #[test]
    fn jsx_attribute_single_quoted_three_byte_utf8() {
        let mut scanner = ScannerState::new("'日本'".to_string(), true);
        let token = scanner.scan_jsx_attribute_value();
        assert_eq!(token, SyntaxKind::StringLiteral);
        assert_eq!(scanner.get_token_value(), "日本");
    }

    #[test]
    fn jsx_attribute_four_byte_utf8_supplementary_plane() {
        let mut scanner = ScannerState::new("\"\u{1F600}\"".to_string(), true);
        let token = scanner.scan_jsx_attribute_value();
        assert_eq!(token, SyntaxKind::StringLiteral);
        assert_eq!(scanner.get_token_value(), "\u{1F600}");
    }

    #[test]
    fn jsx_attribute_mixed_ascii_and_unicode() {
        let mut scanner = ScannerState::new(r#""café""#.to_string(), true);
        let token = scanner.scan_jsx_attribute_value();
        assert_eq!(token, SyntaxKind::StringLiteral);
        assert_eq!(scanner.get_token_value(), "café");
    }
}
mod edge_cases {
    use super::*;

    #[test]
    fn unicode_identifier() {
        let mut scanner = ScannerState::new("variab\u{0142}e".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::Identifier);
        assert_eq!(scanner.get_token_value(), "variab\u{0142}e");
    }

    #[test]
    fn multiple_strings_in_sequence() {
        let mut scanner = ScannerState::new(r#""a" + "b""#.to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::StringLiteral);
        assert_eq!(scanner.get_token_value(), "a");

        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::PlusToken);

        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::StringLiteral);
        assert_eq!(scanner.get_token_value(), "b");
    }

    #[test]
    fn backslash_not_unicode_escape_is_unknown() {
        // A lone backslash not followed by 'u' is Unknown
        let mut scanner = ScannerState::new("\\z".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::Unknown);
    }

    #[test]
    fn interner_produces_consistent_atoms() {
        let mut scanner = ScannerState::new("foo bar foo".to_string(), true);
        scanner.scan(); // foo
        let atom1 = scanner.get_token_atom();
        scanner.scan(); // bar
        scanner.scan(); // foo again
        let atom2 = scanner.get_token_atom();
        assert_eq!(atom1, atom2);
    }

    #[test]
    fn get_text_returns_source() {
        let scanner = ScannerState::new("hello world".to_string(), true);
        assert_eq!(scanner.get_text(), "hello world");
    }

    #[test]
    fn multiple_line_breaks() {
        let mut scanner = ScannerState::new("\n\n\nfoo".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::Identifier);
        assert!(scanner.has_preceding_line_break());
    }

    #[test]
    fn token_value_ref_for_identifiers() {
        let mut scanner = ScannerState::new("myIdentifier".to_string(), true);
        scanner.scan();
        let value_ref = scanner.get_token_value_ref();
        assert_eq!(value_ref, "myIdentifier");
    }

    #[test]
    fn token_value_ref_for_strings() {
        let mut scanner = ScannerState::new(r#""hello""#.to_string(), true);
        scanner.scan();
        let value_ref = scanner.get_token_value_ref();
        assert_eq!(value_ref, "hello");
    }

    #[test]
    fn token_text_ref() {
        let mut scanner = ScannerState::new(r#""hello""#.to_string(), true);
        scanner.scan();
        let text_ref = scanner.get_token_text_ref();
        // token_text includes the quotes
        assert_eq!(text_ref, r#""hello""#);
    }

    #[test]
    fn source_text_ref() {
        let scanner = ScannerState::new("some source".to_string(), true);
        assert_eq!(scanner.source_text(), "some source");
    }

    #[test]
    fn source_slice() {
        let scanner = ScannerState::new("hello world".to_string(), true);
        assert_eq!(scanner.source_slice(6, 11), "world");
    }

    #[test]
    fn scan_multiple_numbers_separated_by_operators() {
        let tokens = scan_all_tokens("1 + 2 * 3");
        assert_eq!(tokens[0], SyntaxKind::NumericLiteral);
        assert_eq!(tokens[1], SyntaxKind::PlusToken);
        assert_eq!(tokens[2], SyntaxKind::NumericLiteral);
        assert_eq!(tokens[3], SyntaxKind::AsteriskToken);
        assert_eq!(tokens[4], SyntaxKind::NumericLiteral);
    }

    #[test]
    fn numeric_separator_trailing_is_invalid() {
        let mut scanner = ScannerState::new("100_".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::NumericLiteral);
        assert!(scanner.get_invalid_separator_pos().is_some());
    }

    #[test]
    fn numeric_separator_consecutive_is_invalid() {
        let mut scanner = ScannerState::new("1__0".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::NumericLiteral);
        assert!(scanner.get_invalid_separator_pos().is_some());
        assert!(scanner.invalid_separator_is_consecutive());
    }
}
