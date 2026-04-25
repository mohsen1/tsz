use tsz_scanner::SyntaxKind;
use tsz_scanner::scanner_impl::{ScannerState, TokenFlags};

mod common;
use common::make_scanner;

// =============================================================================
// Helper: collect all tokens from source
// =============================================================================

fn scan_all_tokens(source: &str) -> Vec<SyntaxKind> {
    let mut scanner = make_scanner(source);
    let mut tokens = Vec::new();
    loop {
        let token = scanner.scan();
        tokens.push(token);
        if token == SyntaxKind::EndOfFileToken {
            break;
        }
    }
    tokens
}

/// Scan a single token from source and return (kind, value)
fn scan_single(source: &str) -> (SyntaxKind, String) {
    let mut scanner = make_scanner(source);
    let kind = scanner.scan();
    let value = scanner.get_token_value();
    (kind, value)
}

// =============================================================================
// 1. String Scanning
// =============================================================================
mod string_scanning {
    use super::*;

    #[test]
    fn empty_double_quoted_string() {
        let mut scanner = make_scanner(r#""""#);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::StringLiteral);
        assert_eq!(scanner.get_token_value(), "");
        assert!(!scanner.is_unterminated());
    }

    #[test]
    fn empty_single_quoted_string() {
        let mut scanner = make_scanner("''");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::StringLiteral);
        assert_eq!(scanner.get_token_value(), "");
        assert!(!scanner.is_unterminated());
    }

    #[test]
    fn simple_double_quoted_string() {
        let mut scanner = make_scanner(r#""hello""#);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::StringLiteral);
        assert_eq!(scanner.get_token_value(), "hello");
    }

    #[test]
    fn simple_single_quoted_string() {
        let mut scanner = make_scanner("'hello'");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::StringLiteral);
        assert_eq!(scanner.get_token_value(), "hello");
    }

    #[test]
    fn escape_newline() {
        let mut scanner = make_scanner(r#""line\nbreak""#);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::StringLiteral);
        assert_eq!(scanner.get_token_value(), "line\nbreak");
    }

    #[test]
    fn escape_carriage_return() {
        let mut scanner = make_scanner(r#""before\rafter""#);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::StringLiteral);
        assert_eq!(scanner.get_token_value(), "before\rafter");
    }

    #[test]
    fn escape_tab() {
        let mut scanner = make_scanner(r#""tab\there""#);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::StringLiteral);
        assert_eq!(scanner.get_token_value(), "tab\there");
    }

    #[test]
    fn escape_backslash() {
        let mut scanner = make_scanner(r#""back\\slash""#);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::StringLiteral);
        assert_eq!(scanner.get_token_value(), "back\\slash");
    }

    #[test]
    fn escape_single_quote_in_single_quoted_string() {
        let mut scanner = make_scanner(r"'it\'s'");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::StringLiteral);
        assert_eq!(scanner.get_token_value(), "it's");
    }

    #[test]
    fn escape_double_quote_in_double_quoted_string() {
        let mut scanner = make_scanner(r#""say \"hi\"""#);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::StringLiteral);
        assert_eq!(scanner.get_token_value(), r#"say "hi""#);
    }

    #[test]
    fn escape_vertical_tab() {
        let mut scanner = make_scanner(r#""vt\vtab""#);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::StringLiteral);
        assert_eq!(scanner.get_token_value(), "vt\x0Btab");
    }

    #[test]
    fn escape_backspace() {
        let mut scanner = make_scanner(r#""bs\bhere""#);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::StringLiteral);
        assert_eq!(scanner.get_token_value(), "bs\x08here");
    }

    #[test]
    fn escape_form_feed() {
        let mut scanner = make_scanner(r#""ff\fhere""#);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::StringLiteral);
        assert_eq!(scanner.get_token_value(), "ff\x0Chere");
    }

    #[test]
    fn escape_hex_two_digit() {
        // \x41 = 'A'
        let mut scanner = make_scanner(r#""\x41""#);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::StringLiteral);
        assert_eq!(scanner.get_token_value(), "A");
    }

    #[test]
    fn escape_hex_lowercase() {
        // \x61 = 'a'
        let mut scanner = make_scanner(r#""\x61""#);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::StringLiteral);
        assert_eq!(scanner.get_token_value(), "a");
    }

    #[test]
    fn escape_hex_null() {
        // \x00 = null character
        let mut scanner = make_scanner(r#""\x00""#);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::StringLiteral);
        assert_eq!(scanner.get_token_value(), "\0");
    }

    #[test]
    fn escape_unicode_four_digit() {
        // \u0041 = 'A'
        let mut scanner = make_scanner(r#""\u0041""#);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::StringLiteral);
        assert_eq!(scanner.get_token_value(), "A");
    }

    #[test]
    fn escape_unicode_bmp() {
        // \u00E9 = 'e' with acute accent
        let mut scanner = make_scanner(r#""\u00E9""#);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::StringLiteral);
        assert_eq!(scanner.get_token_value(), "\u{00E9}");
    }

    #[test]
    fn escape_unicode_braced() {
        // \u{1F600} = grinning face emoji
        let mut scanner = make_scanner(r#""\u{1F600}""#);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::StringLiteral);
        assert_eq!(scanner.get_token_value(), "\u{1F600}");
    }

    #[test]
    fn escape_unicode_braced_small_code_point() {
        // \u{41} = 'A'
        let mut scanner = make_scanner(r#""\u{41}""#);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::StringLiteral);
        assert_eq!(scanner.get_token_value(), "A");
    }

    #[test]
    fn escape_zero_null() {
        // \0 not followed by a digit => null character
        let mut scanner = make_scanner("\"\\0\"");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::StringLiteral);
        assert_eq!(scanner.get_token_value(), "\0");
    }

    #[test]
    fn escape_octal_single_digit() {
        // \1 = octal 1 = char code 1
        let mut scanner = make_scanner("\"\\1\"");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::StringLiteral);
        assert_eq!(scanner.get_token_value(), "\x01");
    }

    #[test]
    fn escape_octal_multi_digit() {
        // \101 = octal 101 = 65 = 'A'
        let mut scanner = make_scanner("\"\\101\"");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::StringLiteral);
        assert_eq!(scanner.get_token_value(), "A");
    }

    #[test]
    fn escape_octal_max_three_digits() {
        // \377 = octal 377 = 255
        let mut scanner = make_scanner("\"\\377\"");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::StringLiteral);
        assert_eq!(scanner.get_token_value(), "\u{00FF}");
    }

    #[test]
    fn unterminated_string_newline() {
        let mut scanner = make_scanner("\"hello\nworld\"");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::StringLiteral);
        assert_eq!(scanner.get_token_value(), "hello");
        assert!(scanner.is_unterminated());
    }

    #[test]
    fn unterminated_string_eof() {
        let mut scanner = make_scanner("\"hello");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::StringLiteral);
        assert_eq!(scanner.get_token_value(), "hello");
        assert!(scanner.is_unterminated());
    }

    #[test]
    fn line_continuation_backslash_newline() {
        // Backslash followed by newline is a line continuation (skips the newline)
        let mut scanner = make_scanner("\"hello\\\nworld\"");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::StringLiteral);
        assert_eq!(scanner.get_token_value(), "helloworld");
    }

    #[test]
    fn string_with_multiple_escape_sequences() {
        let mut scanner = make_scanner(r#""a\tb\nc\\\u0041""#);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::StringLiteral);
        assert_eq!(scanner.get_token_value(), "a\tb\nc\\A");
    }

    #[test]
    fn escape_unknown_char_passes_through() {
        // \z is not a recognized escape - it should produce 'z'
        let mut scanner = make_scanner(r#""\z""#);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::StringLiteral);
        assert_eq!(scanner.get_token_value(), "z");
    }
}

// =============================================================================
// 2. Number Scanning
// =============================================================================
mod number_scanning {
    use super::*;

    #[test]
    fn simple_integer() {
        let mut scanner = make_scanner("42");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::NumericLiteral);
        assert_eq!(scanner.get_token_text(), "42");
    }

    #[test]
    fn zero() {
        let mut scanner = make_scanner("0");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::NumericLiteral);
        assert_eq!(scanner.get_token_text(), "0");
    }

    #[test]
    fn decimal_with_dot() {
        let mut scanner = make_scanner("3.14");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::NumericLiteral);
        assert_eq!(scanner.get_token_text(), "3.14");
    }

    #[test]
    fn leading_dot_number() {
        let mut scanner = make_scanner(".5");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::NumericLiteral);
        assert_eq!(scanner.get_token_text(), ".5");
    }

    #[test]
    fn trailing_dot_number() {
        // "5." should scan as "5." (NumericLiteral with trailing dot)
        let mut scanner = make_scanner("5.");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::NumericLiteral);
        assert_eq!(scanner.get_token_text(), "5.");
    }

    #[test]
    fn hex_lowercase() {
        let mut scanner = make_scanner("0xff");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::NumericLiteral);
        assert_eq!(scanner.get_token_text(), "0xff");
        assert_ne!(
            scanner.get_token_flags() & TokenFlags::HexSpecifier as u32,
            0
        );
    }

    #[test]
    fn hex_uppercase() {
        let mut scanner = make_scanner("0XFF");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::NumericLiteral);
        assert_eq!(scanner.get_token_text(), "0XFF");
        assert_ne!(
            scanner.get_token_flags() & TokenFlags::HexSpecifier as u32,
            0
        );
    }

    #[test]
    fn octal_prefix() {
        let mut scanner = make_scanner("0o77");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::NumericLiteral);
        assert_eq!(scanner.get_token_text(), "0o77");
        assert_ne!(
            scanner.get_token_flags() & TokenFlags::OctalSpecifier as u32,
            0
        );
    }

    #[test]
    fn octal_prefix_uppercase() {
        let mut scanner = make_scanner("0O77");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::NumericLiteral);
        assert_eq!(scanner.get_token_text(), "0O77");
        assert_ne!(
            scanner.get_token_flags() & TokenFlags::OctalSpecifier as u32,
            0
        );
    }

    #[test]
    fn binary_prefix() {
        let mut scanner = make_scanner("0b1010");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::NumericLiteral);
        assert_eq!(scanner.get_token_text(), "0b1010");
        assert_ne!(
            scanner.get_token_flags() & TokenFlags::BinarySpecifier as u32,
            0
        );
    }

    #[test]
    fn binary_prefix_uppercase() {
        let mut scanner = make_scanner("0B1010");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::NumericLiteral);
        assert_eq!(scanner.get_token_text(), "0B1010");
        assert_ne!(
            scanner.get_token_flags() & TokenFlags::BinarySpecifier as u32,
            0
        );
    }

    #[test]
    fn scientific_notation_uppercase_e() {
        let mut scanner = make_scanner("1E5");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::NumericLiteral);
        assert_eq!(scanner.get_token_text(), "1E5");
        assert_ne!(scanner.get_token_flags() & TokenFlags::Scientific as u32, 0);
    }

    #[test]
    fn scientific_notation_lowercase_e() {
        let mut scanner = make_scanner("1e5");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::NumericLiteral);
        assert_eq!(scanner.get_token_text(), "1e5");
        assert_ne!(scanner.get_token_flags() & TokenFlags::Scientific as u32, 0);
    }

    #[test]
    fn scientific_notation_negative_exponent() {
        let mut scanner = make_scanner("1.5e-3");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::NumericLiteral);
        assert_eq!(scanner.get_token_text(), "1.5e-3");
        assert_ne!(scanner.get_token_flags() & TokenFlags::Scientific as u32, 0);
    }

    #[test]
    fn scientific_notation_positive_exponent() {
        let mut scanner = make_scanner("2.5e+10");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::NumericLiteral);
        assert_eq!(scanner.get_token_text(), "2.5e+10");
        assert_ne!(scanner.get_token_flags() & TokenFlags::Scientific as u32, 0);
    }

    #[test]
    fn numeric_separator() {
        let mut scanner = make_scanner("1_000_000");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::NumericLiteral);
        assert_ne!(
            scanner.get_token_flags() & TokenFlags::ContainsSeparator as u32,
            0
        );
        assert_eq!(scanner.get_token_value(), "1_000_000");
    }

    #[test]
    fn numeric_separator_hex() {
        let mut scanner = make_scanner("0xFF_FF");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::NumericLiteral);
        assert_ne!(
            scanner.get_token_flags() & TokenFlags::ContainsSeparator as u32,
            0
        );
    }

    #[test]
    fn numeric_separator_binary() {
        let mut scanner = make_scanner("0b1010_0101");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::NumericLiteral);
        assert_ne!(
            scanner.get_token_flags() & TokenFlags::ContainsSeparator as u32,
            0
        );
    }

    #[test]
    fn bigint_literal() {
        let mut scanner = make_scanner("123n");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::BigIntLiteral);
        assert_eq!(scanner.get_token_value(), "123n");
    }

    #[test]
    fn bigint_hex() {
        let mut scanner = make_scanner("0xFFn");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::BigIntLiteral);
        assert_eq!(scanner.get_token_value(), "0xFFn");
    }

    #[test]
    fn bigint_binary() {
        let mut scanner = make_scanner("0b1010n");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::BigIntLiteral);
        assert_eq!(scanner.get_token_value(), "0b1010n");
    }

    #[test]
    fn bigint_octal() {
        let mut scanner = make_scanner("0o77n");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::BigIntLiteral);
        assert_eq!(scanner.get_token_value(), "0o77n");
    }

    #[test]
    fn legacy_octal() {
        let mut scanner = make_scanner("0777");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::NumericLiteral);
        assert_ne!(scanner.get_token_flags() & TokenFlags::Octal as u32, 0);
    }

    #[test]
    fn legacy_octal_not_pure_octal() {
        // 089 has non-octal digits, so it should NOT be treated as legacy octal
        let mut scanner = make_scanner("089");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::NumericLiteral);
        assert_ne!(
            scanner.get_token_flags() & TokenFlags::ContainsLeadingZero as u32,
            0
        );
        // Should not have the Octal flag
        assert_eq!(scanner.get_token_flags() & TokenFlags::Octal as u32, 0);
    }

    #[test]
    fn dot_then_digit_scans_as_number() {
        // .5 should produce a NumericLiteral, not DotToken + NumericLiteral
        let tokens = scan_all_tokens(".5");
        assert_eq!(tokens[0], SyntaxKind::NumericLiteral);
    }

    #[test]
    fn dot_without_digit_scans_as_dot() {
        let tokens = scan_all_tokens(".");
        assert_eq!(tokens[0], SyntaxKind::DotToken);
    }

    #[test]
    fn number_followed_by_identifier() {
        let tokens = scan_all_tokens("42px");
        assert_eq!(tokens[0], SyntaxKind::NumericLiteral);
        assert_eq!(tokens[1], SyntaxKind::Identifier);
    }

    #[test]
    fn decimal_bigint_suffix_emits_ts1353_and_stays_numeric() {
        let mut scanner = make_scanner(".2n");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::NumericLiteral);
        assert_eq!(scanner.get_token_value(), ".2");
        let diagnostics = scanner.get_scanner_diagnostics();
        assert!(
            diagnostics.iter().any(|d| d.code == 1353),
            "expected TS1353, got {diagnostics:?}"
        );
    }

    #[test]
    fn scientific_bigint_suffix_emits_ts1352_and_stays_numeric() {
        let mut scanner = make_scanner("1e2n");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::NumericLiteral);
        assert_eq!(scanner.get_token_value(), "1e2");
        let diagnostics = scanner.get_scanner_diagnostics();
        assert!(
            diagnostics.iter().any(|d| d.code == 1352),
            "expected TS1352, got {diagnostics:?}"
        );
    }

    #[test]
    fn invalid_separator_at_start() {
        let mut scanner = make_scanner("_123");
        let token = scanner.scan();
        // _123 is an identifier, not a number with invalid separator
        assert_eq!(token, SyntaxKind::Identifier);
    }

    #[test]
    fn decimal_bigint() {
        let mut scanner = make_scanner("0n");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::BigIntLiteral);
        assert_eq!(scanner.get_token_value(), "0n");
    }
}

// =============================================================================
// 3. Identifier Scanning
// =============================================================================
mod identifier_scanning {
    use super::*;

    #[test]
    fn simple_identifier() {
        let mut scanner = make_scanner("foo");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::Identifier);
        assert_eq!(scanner.get_token_value(), "foo");
    }

    #[test]
    fn identifier_with_digits() {
        let mut scanner = make_scanner("foo123");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::Identifier);
        assert_eq!(scanner.get_token_value(), "foo123");
    }

    #[test]
    fn identifier_starting_with_underscore() {
        let mut scanner = make_scanner("_private");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::Identifier);
        assert_eq!(scanner.get_token_value(), "_private");
    }

    #[test]
    fn identifier_starting_with_dollar() {
        let mut scanner = make_scanner("$scope");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::Identifier);
        assert_eq!(scanner.get_token_value(), "$scope");
    }

    #[test]
    fn single_char_identifier() {
        let mut scanner = make_scanner("x");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::Identifier);
        assert_eq!(scanner.get_token_value(), "x");
    }

    #[test]
    fn keyword_if() {
        let mut scanner = make_scanner("if");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::IfKeyword);
    }

    #[test]
    fn keyword_const() {
        let mut scanner = make_scanner("const");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::ConstKeyword);
    }

    #[test]
    fn keyword_function() {
        let mut scanner = make_scanner("function");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::FunctionKeyword);
    }

    #[test]
    fn keyword_let() {
        let mut scanner = make_scanner("let");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::LetKeyword);
    }

    #[test]
    fn keyword_class() {
        let mut scanner = make_scanner("class");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::ClassKeyword);
    }

    #[test]
    fn keyword_return() {
        let mut scanner = make_scanner("return");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::ReturnKeyword);
    }

    #[test]
    fn keyword_true() {
        let mut scanner = make_scanner("true");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::TrueKeyword);
    }

    #[test]
    fn keyword_false() {
        let mut scanner = make_scanner("false");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::FalseKeyword);
    }

    #[test]
    fn keyword_null() {
        let mut scanner = make_scanner("null");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::NullKeyword);
    }

    #[test]
    fn keyword_void() {
        let mut scanner = make_scanner("void");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::VoidKeyword);
    }

    #[test]
    fn contextual_keyword_async() {
        let mut scanner = make_scanner("async");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::AsyncKeyword);
    }

    #[test]
    fn contextual_keyword_type() {
        let mut scanner = make_scanner("type");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::TypeKeyword);
    }

    #[test]
    fn contextual_keyword_interface() {
        let mut scanner = make_scanner("interface");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::InterfaceKeyword);
    }

    #[test]
    fn keyword_prefix_is_identifier() {
        // "iff" is not a keyword
        let mut scanner = make_scanner("iff");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::Identifier);
    }

    #[test]
    fn keyword_suffix_is_identifier() {
        // "classes" is not a keyword
        let mut scanner = make_scanner("classes");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::Identifier);
    }

    #[test]
    fn unicode_escape_identifier_start() {
        // \u0041 = 'A'
        let mut scanner = make_scanner("\\u0041bc");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::Identifier);
        assert_eq!(scanner.get_token_value(), "Abc");
        assert_ne!(
            scanner.get_token_flags() & TokenFlags::UnicodeEscape as u32,
            0
        );
    }

    #[test]
    fn unicode_escape_braced_identifier_start() {
        // \u{42} = 'B'
        let mut scanner = make_scanner("\\u{42}ar");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::Identifier);
        assert_eq!(scanner.get_token_value(), "Bar");
    }

    #[test]
    fn unicode_escape_mid_identifier() {
        // foo\u0042ar = "fooBar"
        let mut scanner = make_scanner("foo\\u0042ar");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::Identifier);
        assert_eq!(scanner.get_token_value(), "fooBar");
    }

    #[test]
    fn unicode_escape_keyword_detection() {
        // \u0069\u0066 = "if" which is a keyword
        let mut scanner = make_scanner("\\u0069\\u0066");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::IfKeyword);
    }

    #[test]
    fn private_identifier() {
        let mut scanner = make_scanner("#myField");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::PrivateIdentifier);
        assert_eq!(scanner.get_token_value(), "#myField");
    }

    #[test]
    fn private_identifier_with_underscore() {
        let mut scanner = make_scanner("#_internal");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::PrivateIdentifier);
        assert_eq!(scanner.get_token_value(), "#_internal");
    }

    #[test]
    fn hash_alone_is_hash_token() {
        let mut scanner = make_scanner("#");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::HashToken);
    }

    #[test]
    fn hash_followed_by_digit_is_hash_token() {
        // #123 - digit cannot start an identifier
        let mut scanner = make_scanner("#123");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::HashToken);
    }

    #[test]
    fn at_hashbang_sequence_tokenization() {
        let mut scanner = make_scanner("@#!x");
        assert_eq!(scanner.scan(), SyntaxKind::AtToken);
        assert_eq!(scanner.scan(), SyntaxKind::HashToken);
        assert_eq!(scanner.scan(), SyntaxKind::ExclamationToken);
        assert_eq!(scanner.scan(), SyntaxKind::Identifier);
    }

    #[test]
    fn reserved_word_check() {
        let mut scanner = make_scanner("break");
        scanner.scan();
        assert!(scanner.is_reserved_word());

        let mut scanner = make_scanner("with");
        scanner.scan();
        assert!(scanner.is_reserved_word());
    }

    #[test]
    fn contextual_keyword_not_reserved() {
        let mut scanner = make_scanner("async");
        scanner.scan();
        assert!(!scanner.is_reserved_word());
    }

    #[test]
    fn is_identifier_check() {
        let mut scanner = make_scanner("myVar");
        scanner.scan();
        assert!(scanner.is_identifier());
    }

    #[test]
    fn contextual_keyword_is_identifier() {
        // Contextual keywords (async, type, etc.) are past WithKeyword so is_identifier() returns true
        let mut scanner = make_scanner("async");
        scanner.scan();
        assert!(scanner.is_identifier());
    }
}

// =============================================================================
// 4. Template Literal Scanning
// =============================================================================
mod template_literal_scanning {
    use super::*;

    #[test]
    fn simple_no_substitution_template() {
        let mut scanner = make_scanner("`hello`");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::NoSubstitutionTemplateLiteral);
        assert_eq!(scanner.get_token_value(), "hello");
    }

    #[test]
    fn empty_template() {
        let mut scanner = make_scanner("``");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::NoSubstitutionTemplateLiteral);
        assert_eq!(scanner.get_token_value(), "");
    }

    #[test]
    fn template_head_with_expression() {
        let mut scanner = make_scanner("`hello ${");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::TemplateHead);
        assert_eq!(scanner.get_token_value(), "hello ");
    }

    #[test]
    fn template_escape_newline() {
        let mut scanner = make_scanner("`hello\\nworld`");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::NoSubstitutionTemplateLiteral);
        assert_eq!(scanner.get_token_value(), "hello\nworld");
    }

    #[test]
    fn template_escape_tab() {
        let mut scanner = make_scanner("`hello\\tworld`");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::NoSubstitutionTemplateLiteral);
        assert_eq!(scanner.get_token_value(), "hello\tworld");
    }

    #[test]
    fn template_escape_backslash() {
        let mut scanner = make_scanner("`back\\\\slash`");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::NoSubstitutionTemplateLiteral);
        assert_eq!(scanner.get_token_value(), "back\\slash");
    }

    #[test]
    fn template_escape_backtick() {
        let mut scanner = make_scanner("`back\\`tick`");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::NoSubstitutionTemplateLiteral);
        assert_eq!(scanner.get_token_value(), "back`tick");
    }

    #[test]
    fn template_escape_dollar() {
        let mut scanner = make_scanner("`hello\\${string}`");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::NoSubstitutionTemplateLiteral);
        assert_eq!(scanner.get_token_value(), "hello${string}");
    }

    #[test]
    fn template_escape_hex() {
        let mut scanner = make_scanner("`\\x41`");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::NoSubstitutionTemplateLiteral);
        assert_eq!(scanner.get_token_value(), "A");
    }

    #[test]
    fn template_escape_unicode_four_digit() {
        let mut scanner = make_scanner("`\\u0041`");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::NoSubstitutionTemplateLiteral);
        assert_eq!(scanner.get_token_value(), "A");
    }

    #[test]
    fn template_escape_unicode_braced() {
        let mut scanner = make_scanner("`\\u{1F600}`");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::NoSubstitutionTemplateLiteral);
        assert_eq!(scanner.get_token_value(), "\u{1F600}");
    }

    #[test]
    fn template_multiline() {
        let mut scanner = make_scanner("`line1\nline2`");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::NoSubstitutionTemplateLiteral);
        assert_eq!(scanner.get_token_value(), "line1\nline2");
    }

    #[test]
    fn unterminated_template() {
        let mut scanner = make_scanner("`hello");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::NoSubstitutionTemplateLiteral);
        assert_eq!(scanner.get_token_value(), "hello");
        assert!(scanner.is_unterminated());
    }

    #[test]
    fn template_rescan_tail() {
        let mut scanner = make_scanner("}world`");
        scanner.reset_token_state(0);
        let token = scanner.re_scan_template_token(false);
        assert_eq!(token, SyntaxKind::TemplateTail);
        assert_eq!(scanner.get_token_value(), "world");
    }

    #[test]
    fn template_rescan_middle() {
        let mut scanner = make_scanner("}mid${");
        scanner.reset_token_state(0);
        let token = scanner.re_scan_template_token(false);
        assert_eq!(token, SyntaxKind::TemplateMiddle);
        assert_eq!(scanner.get_token_value(), "mid");
    }

    #[test]
    fn template_escape_zero() {
        let mut scanner = make_scanner("`\\0`");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::NoSubstitutionTemplateLiteral);
        assert_eq!(scanner.get_token_value(), "\0");
    }

    #[test]
    fn template_octal_escape_is_invalid() {
        // In template literals, octal escapes (other than \0) are invalid
        let mut scanner = make_scanner("`\\1`");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::NoSubstitutionTemplateLiteral);
        assert_ne!(
            scanner.get_token_flags() & TokenFlags::ContainsInvalidEscape as u32,
            0
        );
    }

    #[test]
    fn template_cr_normalization() {
        // \r should be normalized to \n in templates
        let mut scanner = make_scanner("`line1\rline2`");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::NoSubstitutionTemplateLiteral);
        // The rescan path normalizes CR to LF; the initial scan path preserves CR.
        // Let's test via the rescan path (scan_template_and_set_token_value).
    }

    #[test]
    fn template_rescan_cr_lf_normalization() {
        // Test via re_scan_template_head_or_no_substitution_template which uses
        // scan_template_and_set_token_value that normalizes CR/CRLF to LF
        let mut scanner = make_scanner("`line1\r\nline2`");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::NoSubstitutionTemplateLiteral);
        // Now rescan
        let token = scanner.re_scan_template_head_or_no_substitution_template();
        assert_eq!(token, SyntaxKind::NoSubstitutionTemplateLiteral);
        assert_eq!(scanner.get_token_value(), "line1\nline2");
    }

    #[test]
    fn template_with_unicode_characters() {
        let mut scanner = make_scanner("`500\u{00B5}s`");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::NoSubstitutionTemplateLiteral);
        assert_eq!(scanner.get_token_value(), "500\u{00B5}s");
    }
}

// =============================================================================
// 5. Comment Scanning
// =============================================================================
mod comment_scanning {
    use super::*;

    #[test]
    fn single_line_comment_skipped_in_skip_trivia_mode() {
        let mut scanner = make_scanner("// comment\nfoo");
        let token = scanner.scan();
        // With skip_trivia=true, the comment is skipped
        assert_eq!(token, SyntaxKind::Identifier);
        assert_eq!(scanner.get_token_value(), "foo");
    }

    #[test]
    fn single_line_comment_returned_in_non_skip_mode() {
        let mut scanner = ScannerState::new("// comment\nfoo".to_string(), false);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::SingleLineCommentTrivia);
    }

    #[test]
    fn multi_line_comment_skipped_in_skip_trivia_mode() {
        let mut scanner = make_scanner("/* comment */foo");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::Identifier);
        assert_eq!(scanner.get_token_value(), "foo");
    }

    #[test]
    fn multi_line_comment_returned_in_non_skip_mode() {
        let mut scanner = ScannerState::new("/* comment */foo".to_string(), false);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::MultiLineCommentTrivia);
    }

    #[test]
    fn multi_line_comment_spanning_lines() {
        let mut scanner = ScannerState::new("/* line1\nline2 */foo".to_string(), false);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::MultiLineCommentTrivia);
        assert!(scanner.has_preceding_line_break());
    }

    #[test]
    fn unterminated_multi_line_comment() {
        let mut scanner = ScannerState::new("/* unterminated".to_string(), false);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::MultiLineCommentTrivia);
        assert!(scanner.is_unterminated());
    }

    #[test]
    fn jsdoc_comment_is_multi_line() {
        let mut scanner = ScannerState::new("/** @param x */foo".to_string(), false);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::MultiLineCommentTrivia);
    }

    #[test]
    fn empty_comment() {
        let mut scanner = ScannerState::new("/**/foo".to_string(), false);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::MultiLineCommentTrivia);
        assert!(!scanner.is_unterminated());
    }

    #[test]
    fn comment_followed_by_tokens() {
        let tokens = scan_all_tokens("// comment\nvar x = 1;");
        assert_eq!(tokens[0], SyntaxKind::VarKeyword);
        assert_eq!(tokens[1], SyntaxKind::Identifier); // x
        assert_eq!(tokens[2], SyntaxKind::EqualsToken);
        assert_eq!(tokens[3], SyntaxKind::NumericLiteral);
        assert_eq!(tokens[4], SyntaxKind::SemicolonToken);
    }

    #[test]
    fn adjacent_comments() {
        let mut scanner = make_scanner("// first\n// second\nfoo");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::Identifier);
        assert_eq!(scanner.get_token_value(), "foo");
    }
}

// =============================================================================
// 6. Regex Rescanning (edge cases)
// =============================================================================
mod regex_scanning {
    use super::*;
    use tsz_scanner::scanner_impl::RegexFlagErrorKind;

    #[test]
    fn simple_regex() {
        let mut scanner = make_scanner("/abc/");
        scanner.scan();
        let token = scanner.re_scan_slash_token();
        assert_eq!(token, SyntaxKind::RegularExpressionLiteral);
        assert_eq!(scanner.get_token_value(), "/abc/");
    }

    #[test]
    fn regex_with_all_valid_flags() {
        let mut scanner = make_scanner("/abc/gimsyd");
        scanner.scan();
        let token = scanner.re_scan_slash_token();
        assert_eq!(token, SyntaxKind::RegularExpressionLiteral);
        assert_eq!(scanner.get_token_value(), "/abc/gimsyd");
        assert!(scanner.get_regex_flag_errors().is_empty());
    }

    #[test]
    fn regex_with_v_flag() {
        let mut scanner = make_scanner("/abc/v");
        scanner.scan();
        let token = scanner.re_scan_slash_token();
        assert_eq!(token, SyntaxKind::RegularExpressionLiteral);
        assert_eq!(scanner.get_token_value(), "/abc/v");
        assert!(scanner.get_regex_flag_errors().is_empty());
    }

    #[test]
    fn regex_with_character_class() {
        // The / inside [...] should not end the regex
        let mut scanner = make_scanner("/[a/b]/");
        scanner.scan();
        let token = scanner.re_scan_slash_token();
        assert_eq!(token, SyntaxKind::RegularExpressionLiteral);
        assert_eq!(scanner.get_token_value(), "/[a/b]/");
    }

    #[test]
    fn regex_with_escaped_slash() {
        let mut scanner = make_scanner(r"/a\/b/");
        scanner.scan();
        let token = scanner.re_scan_slash_token();
        assert_eq!(token, SyntaxKind::RegularExpressionLiteral);
        assert_eq!(scanner.get_token_value(), r"/a\/b/");
    }

    #[test]
    fn regex_unterminated_at_newline() {
        let mut scanner = make_scanner("/abc\ndef/");
        scanner.scan();
        let token = scanner.re_scan_slash_token();
        assert_eq!(token, SyntaxKind::RegularExpressionLiteral);
        assert!(scanner.is_unterminated());
    }

    #[test]
    fn regex_unterminated_at_eof() {
        let mut scanner = make_scanner("/abc");
        scanner.scan();
        let token = scanner.re_scan_slash_token();
        assert_eq!(token, SyntaxKind::RegularExpressionLiteral);
        assert!(scanner.is_unterminated());
    }

    #[test]
    fn regex_empty_body() {
        let mut scanner = make_scanner("//");
        let token = scanner.scan();
        // "//" is a single-line comment, not a regex
        // In skip_trivia mode, it becomes EOF
        assert_eq!(token, SyntaxKind::EndOfFileToken);
    }

    #[test]
    fn regex_from_slash_equals() {
        // /=abc/ - starts as SlashEqualsToken, rescanned as regex
        let mut scanner = make_scanner("/=abc/");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::SlashEqualsToken);
        let token = scanner.re_scan_slash_token();
        assert_eq!(token, SyntaxKind::RegularExpressionLiteral);
        assert_eq!(scanner.get_token_value(), "/=abc/");
    }

    #[test]
    fn regex_triple_duplicate_flags() {
        let mut scanner = make_scanner("/foo/ggg");
        scanner.scan();
        scanner.re_scan_slash_token();
        let duplicates: Vec<_> = scanner
            .get_regex_flag_errors()
            .iter()
            .filter(|e| matches!(e.kind, RegexFlagErrorKind::Duplicate))
            .collect();
        // 'g' appears 3 times, so 2 are duplicates
        assert_eq!(duplicates.len(), 2);
    }

    #[test]
    fn regex_multiple_invalid_flags() {
        let mut scanner = make_scanner("/foo/gxz");
        scanner.scan();
        scanner.re_scan_slash_token();
        let invalid: Vec<_> = scanner
            .get_regex_flag_errors()
            .iter()
            .filter(|e| matches!(e.kind, RegexFlagErrorKind::InvalidFlag))
            .collect();
        assert_eq!(invalid.len(), 2);
    }

    #[test]
    fn regex_with_backslash_in_class() {
        let mut scanner = make_scanner(r"/[\]]/");
        scanner.scan();
        let token = scanner.re_scan_slash_token();
        assert_eq!(token, SyntaxKind::RegularExpressionLiteral);
        assert!(!scanner.is_unterminated());
    }
}

// =============================================================================
// 7. Punctuation and Operators
// =============================================================================
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

// =============================================================================
// 8. Whitespace and Trivia
// =============================================================================
mod whitespace_scanning {
    use super::*;

    #[test]
    fn whitespace_skipped_in_skip_trivia_mode() {
        let mut scanner = make_scanner("   foo");
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
        let mut scanner = make_scanner("\nfoo");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::Identifier);
        assert!(scanner.has_preceding_line_break());
    }

    #[test]
    fn no_preceding_line_break_on_same_line() {
        let mut scanner = make_scanner("foo bar");
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
        let mut scanner = make_scanner("");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::EndOfFileToken);
    }

    #[test]
    fn eof_after_all_tokens() {
        let mut scanner = make_scanner("x");
        scanner.scan(); // x
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::EndOfFileToken);
    }
}

// =============================================================================
// 9. Position Tracking
// =============================================================================
mod position_tracking {
    use super::*;

    #[test]
    fn token_positions_simple() {
        let mut scanner = make_scanner("foo bar");
        scanner.scan();
        assert_eq!(scanner.get_token_start(), 0);
        assert_eq!(scanner.get_token_end(), 3);

        scanner.scan();
        assert_eq!(scanner.get_token_start(), 4);
        assert_eq!(scanner.get_token_end(), 7);
    }

    #[test]
    fn full_start_includes_trivia() {
        let mut scanner = make_scanner("  foo");
        scanner.scan();
        assert_eq!(scanner.get_token_full_start(), 0);
        assert_eq!(scanner.get_token_start(), 2);
        assert_eq!(scanner.get_token_end(), 5);
    }

    #[test]
    fn token_text_matches_source() {
        let mut scanner = make_scanner("foo + bar");
        scanner.scan(); // foo
        assert_eq!(scanner.get_token_text(), "foo");
        scanner.scan(); // +
        assert_eq!(scanner.get_token_text(), "+");
        scanner.scan(); // bar
        assert_eq!(scanner.get_token_text(), "bar");
    }
}

// =============================================================================
// 10. Rescan Methods
// =============================================================================
mod rescan_methods {
    use super::*;

    #[test]
    fn rescan_greater_single() {
        let mut scanner = make_scanner("x > y");
        scanner.scan(); // x
        scanner.scan(); // >
        let token = scanner.re_scan_greater_token();
        // No chars follow >, so it stays as GreaterThanToken
        assert_eq!(token, SyntaxKind::GreaterThanToken);
    }

    #[test]
    fn rescan_greater_equals() {
        let mut scanner = make_scanner("x >= y");
        scanner.scan(); // x
        scanner.scan(); // >
        let token = scanner.re_scan_greater_token();
        assert_eq!(token, SyntaxKind::GreaterThanEqualsToken);
    }

    #[test]
    fn rescan_greater_shift_right() {
        let mut scanner = make_scanner("x >> y");
        scanner.scan(); // x
        scanner.scan(); // >
        let token = scanner.re_scan_greater_token();
        assert_eq!(token, SyntaxKind::GreaterThanGreaterThanToken);
    }

    #[test]
    fn rescan_greater_unsigned_shift_right() {
        let mut scanner = make_scanner("x >>> y");
        scanner.scan(); // x
        scanner.scan(); // >
        let token = scanner.re_scan_greater_token();
        assert_eq!(token, SyntaxKind::GreaterThanGreaterThanGreaterThanToken);
    }

    #[test]
    fn rescan_greater_shift_right_assign() {
        let mut scanner = make_scanner("x >>= y");
        scanner.scan(); // x
        scanner.scan(); // >
        let token = scanner.re_scan_greater_token();
        assert_eq!(token, SyntaxKind::GreaterThanGreaterThanEqualsToken);
    }

    #[test]
    fn rescan_asterisk_equals() {
        let mut scanner = make_scanner("*=");
        scanner.scan(); // *=
        assert_eq!(scanner.get_token(), SyntaxKind::AsteriskEqualsToken);
        let token = scanner.re_scan_asterisk_equals_token();
        assert_eq!(token, SyntaxKind::EqualsToken);
    }

    #[test]
    fn rescan_less_than_slash() {
        let mut scanner = make_scanner("</tag>");
        scanner.scan(); // <
        let token = scanner.re_scan_less_than_token();
        assert_eq!(token, SyntaxKind::LessThanSlashToken);
    }

    #[test]
    fn rescan_question_dot() {
        let mut scanner = make_scanner("?.foo");
        scanner.scan(); // gets QuestionDotToken directly
        // But let's test re_scan_question_token from QuestionToken
        let mut scanner = make_scanner("?");
        scanner.scan();
        let token = scanner.re_scan_question_token();
        assert_eq!(token, SyntaxKind::QuestionToken); // nothing follows, stays ?
    }
}

// =============================================================================
// 11. Save/Restore State
// =============================================================================
mod state_management {
    use super::*;

    #[test]
    fn save_restore_basic() {
        let mut scanner = make_scanner("a b c");
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
        let mut scanner = make_scanner("old text");
        scanner.set_text("fresh text".to_string(), None, None);
        scanner.reset_token_state(0);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::Identifier);
        assert_eq!(scanner.get_token_value(), "fresh");
    }

    #[test]
    fn set_text_with_offset_and_length() {
        let mut scanner = make_scanner("");
        scanner.set_text("xxFOOxx".to_string(), Some(2), Some(3));
        scanner.reset_token_state(2);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::Identifier);
        // FOO is scanned from offset 2 with length 3
        assert_eq!(scanner.get_token_value(), "FOO");
    }

    #[test]
    fn reset_token_state_clears_flags() {
        let mut scanner = make_scanner("\nfoo");
        scanner.scan(); // foo - has PrecedingLineBreak
        assert!(scanner.has_preceding_line_break());
        scanner.reset_token_state(0);
        // After reset, flags are cleared
        assert!(!scanner.has_preceding_line_break());
    }
}

// =============================================================================
// 12. Shebang Handling
// =============================================================================
mod shebang_scanning {
    use super::*;

    #[test]
    fn shebang_at_start() {
        let mut scanner = make_scanner("#!/usr/bin/env node\nvar x");
        let len = scanner.scan_shebang_trivia();
        assert!(len > 0);
        // After shebang, scanner position should be past the shebang + newline
        assert_eq!(scanner.get_pos(), 20); // "#!/usr/bin/env node\n" = 20 chars
    }

    #[test]
    fn no_shebang_returns_zero() {
        let mut scanner = make_scanner("var x");
        let len = scanner.scan_shebang_trivia();
        assert_eq!(len, 0);
    }

    #[test]
    fn shebang_not_at_start_returns_zero() {
        let mut scanner = make_scanner("var x");
        scanner.scan(); // advance past "var"
        // If we try to scan shebang now, it won't be at pos 0
        let len = scanner.scan_shebang_trivia();
        assert_eq!(len, 0);
    }

    #[test]
    fn shebang_with_crlf() {
        let mut scanner = make_scanner("#!/usr/bin/env node\r\nvar x");
        let len = scanner.scan_shebang_trivia();
        assert!(len > 0);
        assert_eq!(scanner.get_pos(), 21); // includes \r\n
    }

    #[test]
    fn shebang_at_eof() {
        let mut scanner = make_scanner("#!/usr/bin/env node");
        let len = scanner.scan_shebang_trivia();
        assert!(len > 0);
    }
}

// =============================================================================
// 13. Full Token Stream Tests
// =============================================================================
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

// =============================================================================
// 14. JSX Scanning
// =============================================================================
mod jsx_scanning {
    use super::*;

    #[test]
    fn jsx_identifier_with_hyphen() {
        let mut scanner = make_scanner("data-testid");
        scanner.scan(); // scans "data" initially
        // Now call scan_jsx_identifier to extend with hyphenated parts
        scanner.scan_jsx_identifier();
        assert_eq!(scanner.get_token(), SyntaxKind::Identifier);
        assert_eq!(scanner.get_token_value(), "data-testid");
    }

    #[test]
    fn jsx_text_scanning() {
        let mut scanner = make_scanner(">hello world<");
        scanner.scan(); // >
        scanner.scan(); // scans "hello" as identifier
        let token = scanner.re_scan_jsx_token(true);
        assert_eq!(token, SyntaxKind::JsxText);
        assert_eq!(scanner.get_token_value(), "hello world");
    }

    #[test]
    fn jsx_attribute_double_quoted() {
        let mut scanner = make_scanner(r#""value""#);
        let token = scanner.scan_jsx_attribute_value();
        assert_eq!(token, SyntaxKind::StringLiteral);
        assert_eq!(scanner.get_token_value(), "value");
    }

    #[test]
    fn jsx_attribute_single_quoted() {
        let mut scanner = make_scanner("'value'");
        let token = scanner.scan_jsx_attribute_value();
        assert_eq!(token, SyntaxKind::StringLiteral);
        assert_eq!(scanner.get_token_value(), "value");
    }
}

// =============================================================================
// 15. Edge Cases
// =============================================================================
mod edge_cases {
    use super::*;

    #[test]
    fn unicode_identifier() {
        let mut scanner = make_scanner("variab\u{0142}e");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::Identifier);
        assert_eq!(scanner.get_token_value(), "variab\u{0142}e");
    }

    #[test]
    fn multiple_strings_in_sequence() {
        let mut scanner = make_scanner(r#""a" + "b""#);
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
        let mut scanner = make_scanner("\\z");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::Unknown);
    }

    #[test]
    fn interner_produces_consistent_atoms() {
        let mut scanner = make_scanner("foo bar foo");
        scanner.scan(); // foo
        let atom1 = scanner.get_token_atom();
        scanner.scan(); // bar
        scanner.scan(); // foo again
        let atom2 = scanner.get_token_atom();
        assert_eq!(atom1, atom2);
    }

    #[test]
    fn get_text_returns_source() {
        let scanner = make_scanner("hello world");
        assert_eq!(scanner.get_text(), "hello world");
    }

    #[test]
    fn multiple_line_breaks() {
        let mut scanner = make_scanner("\n\n\nfoo");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::Identifier);
        assert!(scanner.has_preceding_line_break());
    }

    #[test]
    fn token_value_ref_for_identifiers() {
        let mut scanner = make_scanner("myIdentifier");
        scanner.scan();
        let value_ref = scanner.get_token_value_ref();
        assert_eq!(value_ref, "myIdentifier");
    }

    #[test]
    fn token_value_ref_for_strings() {
        let mut scanner = make_scanner(r#""hello""#);
        scanner.scan();
        let value_ref = scanner.get_token_value_ref();
        assert_eq!(value_ref, "hello");
    }

    #[test]
    fn token_text_ref() {
        let mut scanner = make_scanner(r#""hello""#);
        scanner.scan();
        let text_ref = scanner.get_token_text_ref();
        // token_text includes the quotes
        assert_eq!(text_ref, r#""hello""#);
    }

    #[test]
    fn source_text_ref() {
        let scanner = make_scanner("some source");
        assert_eq!(scanner.source_text(), "some source");
    }

    #[test]
    fn source_slice() {
        let scanner = make_scanner("hello world");
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
        let mut scanner = make_scanner("100_");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::NumericLiteral);
        assert!(scanner.get_invalid_separator_pos().is_some());
    }

    #[test]
    fn numeric_separator_consecutive_is_invalid() {
        let mut scanner = make_scanner("1__0");
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::NumericLiteral);
        assert!(scanner.get_invalid_separator_pos().is_some());
        assert!(scanner.invalid_separator_is_consecutive());
    }
}
