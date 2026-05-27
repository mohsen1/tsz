mod string_scanning {
    use super::*;

    #[test]
    fn empty_double_quoted_string() {
        let mut scanner = ScannerState::new(r#""""#.to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::StringLiteral);
        assert_eq!(scanner.get_token_value(), "");
        assert!(!scanner.is_unterminated());
    }

    #[test]
    fn empty_single_quoted_string() {
        let mut scanner = ScannerState::new("''".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::StringLiteral);
        assert_eq!(scanner.get_token_value(), "");
        assert!(!scanner.is_unterminated());
    }

    #[test]
    fn simple_double_quoted_string() {
        let mut scanner = ScannerState::new(r#""hello""#.to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::StringLiteral);
        assert_eq!(scanner.get_token_value(), "hello");
    }

    #[test]
    fn simple_single_quoted_string() {
        let mut scanner = ScannerState::new("'hello'".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::StringLiteral);
        assert_eq!(scanner.get_token_value(), "hello");
    }

    #[test]
    fn escape_newline() {
        let mut scanner = ScannerState::new(r#""line\nbreak""#.to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::StringLiteral);
        assert_eq!(scanner.get_token_value(), "line\nbreak");
    }

    #[test]
    fn escape_carriage_return() {
        let mut scanner = ScannerState::new(r#""before\rafter""#.to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::StringLiteral);
        assert_eq!(scanner.get_token_value(), "before\rafter");
    }

    #[test]
    fn escape_tab() {
        let mut scanner = ScannerState::new(r#""tab\there""#.to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::StringLiteral);
        assert_eq!(scanner.get_token_value(), "tab\there");
    }

    #[test]
    fn escape_backslash() {
        let mut scanner = ScannerState::new(r#""back\\slash""#.to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::StringLiteral);
        assert_eq!(scanner.get_token_value(), "back\\slash");
    }

    #[test]
    fn escape_single_quote_in_single_quoted_string() {
        let mut scanner = ScannerState::new(r"'it\'s'".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::StringLiteral);
        assert_eq!(scanner.get_token_value(), "it's");
    }

    #[test]
    fn escape_double_quote_in_double_quoted_string() {
        let mut scanner = ScannerState::new(r#""say \"hi\"""#.to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::StringLiteral);
        assert_eq!(scanner.get_token_value(), r#"say "hi""#);
    }

    #[test]
    fn escape_vertical_tab() {
        let mut scanner = ScannerState::new(r#""vt\vtab""#.to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::StringLiteral);
        assert_eq!(scanner.get_token_value(), "vt\x0Btab");
    }

    #[test]
    fn escape_backspace() {
        let mut scanner = ScannerState::new(r#""bs\bhere""#.to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::StringLiteral);
        assert_eq!(scanner.get_token_value(), "bs\x08here");
    }

    #[test]
    fn escape_form_feed() {
        let mut scanner = ScannerState::new(r#""ff\fhere""#.to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::StringLiteral);
        assert_eq!(scanner.get_token_value(), "ff\x0Chere");
    }

    #[test]
    fn escape_hex_two_digit() {
        // \x41 = 'A'
        let mut scanner = ScannerState::new(r#""\x41""#.to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::StringLiteral);
        assert_eq!(scanner.get_token_value(), "A");
    }

    #[test]
    fn escape_hex_lowercase() {
        // \x61 = 'a'
        let mut scanner = ScannerState::new(r#""\x61""#.to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::StringLiteral);
        assert_eq!(scanner.get_token_value(), "a");
    }

    #[test]
    fn escape_hex_null() {
        // \x00 = null character
        let mut scanner = ScannerState::new(r#""\x00""#.to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::StringLiteral);
        assert_eq!(scanner.get_token_value(), "\0");
    }

    #[test]
    fn escape_unicode_four_digit() {
        // \u0041 = 'A'
        let mut scanner = ScannerState::new(r#""\u0041""#.to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::StringLiteral);
        assert_eq!(scanner.get_token_value(), "A");
    }

    #[test]
    fn escape_unicode_bmp() {
        // \u00E9 = 'e' with acute accent
        let mut scanner = ScannerState::new(r#""\u00E9""#.to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::StringLiteral);
        assert_eq!(scanner.get_token_value(), "\u{00E9}");
    }

    #[test]
    fn escape_unicode_braced() {
        // \u{1F600} = grinning face emoji
        let mut scanner = ScannerState::new(r#""\u{1F600}""#.to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::StringLiteral);
        assert_eq!(scanner.get_token_value(), "\u{1F600}");
    }

    #[test]
    fn escape_unicode_braced_small_code_point() {
        // \u{41} = 'A'
        let mut scanner = ScannerState::new(r#""\u{41}""#.to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::StringLiteral);
        assert_eq!(scanner.get_token_value(), "A");
    }

    #[test]
    fn escape_zero_null() {
        // \0 not followed by a digit => null character
        let mut scanner = ScannerState::new("\"\\0\"".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::StringLiteral);
        assert_eq!(scanner.get_token_value(), "\0");
    }

    #[test]
    fn escape_octal_single_digit() {
        // \1 = octal 1 = char code 1
        let mut scanner = ScannerState::new("\"\\1\"".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::StringLiteral);
        assert_eq!(scanner.get_token_value(), "\x01");
    }

    #[test]
    fn escape_octal_multi_digit() {
        // \101 = octal 101 = 65 = 'A'
        let mut scanner = ScannerState::new("\"\\101\"".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::StringLiteral);
        assert_eq!(scanner.get_token_value(), "A");
    }

    #[test]
    fn escape_octal_max_three_digits() {
        // \377 = octal 377 = 255
        let mut scanner = ScannerState::new("\"\\377\"".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::StringLiteral);
        assert_eq!(scanner.get_token_value(), "\u{00FF}");
    }

    #[test]
    fn unterminated_string_newline() {
        let mut scanner = ScannerState::new("\"hello\nworld\"".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::StringLiteral);
        assert_eq!(scanner.get_token_value(), "hello");
        assert!(scanner.is_unterminated());
    }

    #[test]
    fn unterminated_string_eof() {
        let mut scanner = ScannerState::new("\"hello".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::StringLiteral);
        assert_eq!(scanner.get_token_value(), "hello");
        assert!(scanner.is_unterminated());
    }

    #[test]
    fn line_continuation_backslash_newline() {
        // Backslash followed by newline is a line continuation (skips the newline)
        let mut scanner = ScannerState::new("\"hello\\\nworld\"".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::StringLiteral);
        assert_eq!(scanner.get_token_value(), "helloworld");
    }

    #[test]
    fn string_with_multiple_escape_sequences() {
        let mut scanner = ScannerState::new(r#""a\tb\nc\\\u0041""#.to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::StringLiteral);
        assert_eq!(scanner.get_token_value(), "a\tb\nc\\A");
    }

    #[test]
    fn escape_unknown_char_passes_through() {
        // \z is not a recognized escape - it should produce 'z'
        let mut scanner = ScannerState::new(r#""\z""#.to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::StringLiteral);
        assert_eq!(scanner.get_token_value(), "z");
    }
}
mod number_scanning {
    use super::*;

    #[test]
    fn simple_integer() {
        let mut scanner = ScannerState::new("42".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::NumericLiteral);
        assert_eq!(scanner.get_token_text(), "42");
    }

    #[test]
    fn zero() {
        let mut scanner = ScannerState::new("0".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::NumericLiteral);
        assert_eq!(scanner.get_token_text(), "0");
    }

    #[test]
    fn decimal_with_dot() {
        let mut scanner = ScannerState::new("3.14".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::NumericLiteral);
        assert_eq!(scanner.get_token_text(), "3.14");
    }

    #[test]
    fn leading_dot_number() {
        let mut scanner = ScannerState::new(".5".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::NumericLiteral);
        assert_eq!(scanner.get_token_text(), ".5");
    }

    #[test]
    fn trailing_dot_number() {
        // "5." should scan as "5." (NumericLiteral with trailing dot)
        let mut scanner = ScannerState::new("5.".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::NumericLiteral);
        assert_eq!(scanner.get_token_text(), "5.");
    }

    #[test]
    fn hex_lowercase() {
        let mut scanner = ScannerState::new("0xff".to_string(), true);
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
        let mut scanner = ScannerState::new("0XFF".to_string(), true);
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
        let mut scanner = ScannerState::new("0o77".to_string(), true);
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
        let mut scanner = ScannerState::new("0O77".to_string(), true);
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
        let mut scanner = ScannerState::new("0b1010".to_string(), true);
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
        let mut scanner = ScannerState::new("0B1010".to_string(), true);
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
        let mut scanner = ScannerState::new("1E5".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::NumericLiteral);
        assert_eq!(scanner.get_token_text(), "1E5");
        assert_ne!(scanner.get_token_flags() & TokenFlags::Scientific as u32, 0);
    }

    #[test]
    fn scientific_notation_lowercase_e() {
        let mut scanner = ScannerState::new("1e5".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::NumericLiteral);
        assert_eq!(scanner.get_token_text(), "1e5");
        assert_ne!(scanner.get_token_flags() & TokenFlags::Scientific as u32, 0);
    }

    #[test]
    fn scientific_notation_negative_exponent() {
        let mut scanner = ScannerState::new("1.5e-3".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::NumericLiteral);
        assert_eq!(scanner.get_token_text(), "1.5e-3");
        assert_ne!(scanner.get_token_flags() & TokenFlags::Scientific as u32, 0);
    }

    #[test]
    fn scientific_notation_positive_exponent() {
        let mut scanner = ScannerState::new("2.5e+10".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::NumericLiteral);
        assert_eq!(scanner.get_token_text(), "2.5e+10");
        assert_ne!(scanner.get_token_flags() & TokenFlags::Scientific as u32, 0);
    }

    #[test]
    fn numeric_separator() {
        let mut scanner = ScannerState::new("1_000_000".to_string(), true);
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
        let mut scanner = ScannerState::new("0xFF_FF".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::NumericLiteral);
        assert_ne!(
            scanner.get_token_flags() & TokenFlags::ContainsSeparator as u32,
            0
        );
    }

    #[test]
    fn numeric_separator_binary() {
        let mut scanner = ScannerState::new("0b1010_0101".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::NumericLiteral);
        assert_ne!(
            scanner.get_token_flags() & TokenFlags::ContainsSeparator as u32,
            0
        );
    }

    #[test]
    fn bigint_literal() {
        let mut scanner = ScannerState::new("123n".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::BigIntLiteral);
        assert_eq!(scanner.get_token_value(), "123n");
    }

    #[test]
    fn bigint_hex() {
        let mut scanner = ScannerState::new("0xFFn".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::BigIntLiteral);
        assert_eq!(scanner.get_token_value(), "0xFFn");
    }

    #[test]
    fn bigint_binary() {
        let mut scanner = ScannerState::new("0b1010n".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::BigIntLiteral);
        assert_eq!(scanner.get_token_value(), "0b1010n");
    }

    #[test]
    fn bigint_octal() {
        let mut scanner = ScannerState::new("0o77n".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::BigIntLiteral);
        assert_eq!(scanner.get_token_value(), "0o77n");
    }

    #[test]
    fn legacy_octal() {
        let mut scanner = ScannerState::new("0777".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::NumericLiteral);
        assert_ne!(scanner.get_token_flags() & TokenFlags::Octal as u32, 0);
    }

    #[test]
    fn legacy_octal_not_pure_octal() {
        // 089 has non-octal digits, so it should NOT be treated as legacy octal
        let mut scanner = ScannerState::new("089".to_string(), true);
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
        let mut scanner = ScannerState::new(".2n".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::NumericLiteral);
        // tsc consumes the `n` (advances pos) and reports TS1353; emit then
        // prints the literal verbatim as `.2n;`. The token value covers the
        // full consumed span so the emitter preserves the source spelling.
        assert_eq!(scanner.get_token_value(), ".2n");
        let diagnostics = scanner.get_scanner_diagnostics();
        assert!(
            diagnostics.iter().any(|d| d.code == 1353),
            "expected TS1353, got {diagnostics:?}"
        );
    }

    #[test]
    fn scientific_bigint_suffix_emits_ts1352_and_stays_numeric() {
        let mut scanner = ScannerState::new("1e2n".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::NumericLiteral);
        // Same recovery rule as `.2n`: the trailing `n` is consumed, TS1352
        // is reported, and the literal span includes the `n` so emit prints
        // `1e2n;` instead of dropping the suffix.
        assert_eq!(scanner.get_token_value(), "1e2n");
        let diagnostics = scanner.get_scanner_diagnostics();
        assert!(
            diagnostics.iter().any(|d| d.code == 1352),
            "expected TS1352, got {diagnostics:?}"
        );
    }

    #[test]
    fn invalid_separator_at_start() {
        let mut scanner = ScannerState::new("_123".to_string(), true);
        let token = scanner.scan();
        // _123 is an identifier, not a number with invalid separator
        assert_eq!(token, SyntaxKind::Identifier);
    }

    #[test]
    fn invalid_unicode_escape_identifier_start_scans_unknown() {
        let mut scanner =
            ScannerState::new("_\\uD4A5\\u7204\\uC316\\uE59F = local".to_string(), true);
        let mut saw_unknown_escape = false;
        loop {
            let token = scanner.scan();
            if token == SyntaxKind::Unknown && scanner.get_token_text().starts_with("\\u") {
                saw_unknown_escape = true;
            }
            if token == SyntaxKind::EndOfFileToken {
                break;
            }
        }

        assert!(
            saw_unknown_escape,
            "expected at least one invalid unicode escape to scan as Unknown"
        );
        let diagnostics = scanner.get_scanner_diagnostics();
        assert!(
            diagnostics.iter().any(|d| d.code == 1127),
            "expected TS1127 for invalid unicode escape identifier start, got {diagnostics:?}"
        );
    }

    #[test]
    fn decimal_bigint() {
        let mut scanner = ScannerState::new("0n".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::BigIntLiteral);
        assert_eq!(scanner.get_token_value(), "0n");
    }

    // tsc parity: empty hex / binary / octal literals (with or without bigint
    // suffix) emit `<base> digit expected` (TS1125 / TS1177 / TS1178) at the
    // position immediately after the base prefix. Mirrors `scanner.ts`'s
    // `if (!tokenValue) error(...)` ladder in `scanIntegerBaseLiteral`.
    // Reproduces baselines from `parseBigInt.errors.txt` (lines 8-10).

    #[test]
    fn empty_hex_bigint_emits_ts1125() {
        let mut scanner = ScannerState::new("0xn".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::BigIntLiteral);
        let diagnostics = scanner.get_scanner_diagnostics();
        let hit = diagnostics
            .iter()
            .find(|d| d.code == 1125)
            .unwrap_or_else(|| {
                panic!("expected TS1125 (Hexadecimal digit expected); got {diagnostics:?}")
            });
        // Position should land on the `n` (just past `0x`).
        assert_eq!(
            hit.pos, 2,
            "expected error at pos 2 (after '0x'), got {hit:?}"
        );
        assert_eq!(hit.length, 0, "expected zero-width diagnostic, got {hit:?}");
    }

    #[test]
    fn empty_binary_bigint_emits_ts1177() {
        let mut scanner = ScannerState::new("0bn".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::BigIntLiteral);
        let diagnostics = scanner.get_scanner_diagnostics();
        let hit = diagnostics
            .iter()
            .find(|d| d.code == 1177)
            .unwrap_or_else(|| {
                panic!("expected TS1177 (Binary digit expected); got {diagnostics:?}")
            });
        assert_eq!(
            hit.pos, 2,
            "expected error at pos 2 (after '0b'), got {hit:?}"
        );
        assert_eq!(hit.length, 0, "expected zero-width diagnostic, got {hit:?}");
    }

    #[test]
    fn empty_octal_bigint_emits_ts1178() {
        let mut scanner = ScannerState::new("0on".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::BigIntLiteral);
        let diagnostics = scanner.get_scanner_diagnostics();
        let hit = diagnostics
            .iter()
            .find(|d| d.code == 1178)
            .unwrap_or_else(|| {
                panic!("expected TS1178 (Octal digit expected); got {diagnostics:?}")
            });
        assert_eq!(
            hit.pos, 2,
            "expected error at pos 2 (after '0o'), got {hit:?}"
        );
        assert_eq!(hit.length, 0, "expected zero-width diagnostic, got {hit:?}");
    }

    #[test]
    fn empty_hex_numeric_at_eof_emits_ts1125() {
        // No `n` suffix: still a NumericLiteral, still TS1125.
        let mut scanner = ScannerState::new("0x".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::NumericLiteral);
        let diagnostics = scanner.get_scanner_diagnostics();
        assert!(
            diagnostics.iter().any(|d| d.code == 1125),
            "expected TS1125 for empty `0x`, got {diagnostics:?}"
        );
    }

    #[test]
    fn empty_binary_numeric_at_eof_emits_ts1177() {
        let mut scanner = ScannerState::new("0b".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::NumericLiteral);
        let diagnostics = scanner.get_scanner_diagnostics();
        assert!(
            diagnostics.iter().any(|d| d.code == 1177),
            "expected TS1177 for empty `0b`, got {diagnostics:?}"
        );
    }

    #[test]
    fn empty_octal_numeric_at_eof_emits_ts1178() {
        let mut scanner = ScannerState::new("0o".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::NumericLiteral);
        let diagnostics = scanner.get_scanner_diagnostics();
        assert!(
            diagnostics.iter().any(|d| d.code == 1178),
            "expected TS1178 for empty `0o`, got {diagnostics:?}"
        );
    }

    #[test]
    fn populated_hex_bigint_does_not_emit_ts1125() {
        let mut scanner = ScannerState::new("0xFFn".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::BigIntLiteral);
        let diagnostics = scanner.get_scanner_diagnostics();
        assert!(
            !diagnostics.iter().any(|d| d.code == 1125),
            "valid hex bigint should not emit TS1125; got {diagnostics:?}"
        );
    }

    #[test]
    fn populated_binary_bigint_does_not_emit_ts1177() {
        let mut scanner = ScannerState::new("0b101n".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::BigIntLiteral);
        let diagnostics = scanner.get_scanner_diagnostics();
        assert!(
            !diagnostics.iter().any(|d| d.code == 1177),
            "valid binary bigint should not emit TS1177; got {diagnostics:?}"
        );
    }

    #[test]
    fn populated_octal_bigint_does_not_emit_ts1178() {
        let mut scanner = ScannerState::new("0o77n".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::BigIntLiteral);
        let diagnostics = scanner.get_scanner_diagnostics();
        assert!(
            !diagnostics.iter().any(|d| d.code == 1178),
            "valid octal bigint should not emit TS1178; got {diagnostics:?}"
        );
    }
}
mod identifier_scanning {
    use super::*;

    #[test]
    fn simple_identifier() {
        let mut scanner = ScannerState::new("foo".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::Identifier);
        assert_eq!(scanner.get_token_value(), "foo");
    }

    #[test]
    fn identifier_with_digits() {
        let mut scanner = ScannerState::new("foo123".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::Identifier);
        assert_eq!(scanner.get_token_value(), "foo123");
    }

    #[test]
    fn identifier_starting_with_underscore() {
        let mut scanner = ScannerState::new("_private".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::Identifier);
        assert_eq!(scanner.get_token_value(), "_private");
    }

    #[test]
    fn identifier_starting_with_dollar() {
        let mut scanner = ScannerState::new("$scope".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::Identifier);
        assert_eq!(scanner.get_token_value(), "$scope");
    }

    #[test]
    fn single_char_identifier() {
        let mut scanner = ScannerState::new("x".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::Identifier);
        assert_eq!(scanner.get_token_value(), "x");
    }

    #[test]
    fn keyword_if() {
        let mut scanner = ScannerState::new("if".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::IfKeyword);
    }

    #[test]
    fn keyword_const() {
        let mut scanner = ScannerState::new("const".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::ConstKeyword);
    }

    #[test]
    fn keyword_function() {
        let mut scanner = ScannerState::new("function".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::FunctionKeyword);
    }

    #[test]
    fn keyword_let() {
        let mut scanner = ScannerState::new("let".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::LetKeyword);
    }

    #[test]
    fn keyword_class() {
        let mut scanner = ScannerState::new("class".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::ClassKeyword);
    }

    #[test]
    fn keyword_return() {
        let mut scanner = ScannerState::new("return".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::ReturnKeyword);
    }

    #[test]
    fn keyword_true() {
        let mut scanner = ScannerState::new("true".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::TrueKeyword);
    }

    #[test]
    fn keyword_false() {
        let mut scanner = ScannerState::new("false".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::FalseKeyword);
    }

    #[test]
    fn keyword_null() {
        let mut scanner = ScannerState::new("null".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::NullKeyword);
    }

    #[test]
    fn keyword_void() {
        let mut scanner = ScannerState::new("void".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::VoidKeyword);
    }

    #[test]
    fn contextual_keyword_async() {
        let mut scanner = ScannerState::new("async".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::AsyncKeyword);
    }

    #[test]
    fn contextual_keyword_type() {
        let mut scanner = ScannerState::new("type".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::TypeKeyword);
    }

    #[test]
    fn contextual_keyword_interface() {
        let mut scanner = ScannerState::new("interface".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::InterfaceKeyword);
    }

    #[test]
    fn keyword_prefix_is_identifier() {
        // "iff" is not a keyword
        let mut scanner = ScannerState::new("iff".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::Identifier);
    }

    #[test]
    fn keyword_suffix_is_identifier() {
        // "classes" is not a keyword
        let mut scanner = ScannerState::new("classes".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::Identifier);
    }

    #[test]
    fn unicode_escape_identifier_start() {
        // \u0041 = 'A'
        let mut scanner = ScannerState::new("\\u0041bc".to_string(), true);
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
        let mut scanner = ScannerState::new("\\u{42}ar".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::Identifier);
        assert_eq!(scanner.get_token_value(), "Bar");
    }

    #[test]
    fn unicode_escape_braced_astral_identifier_start_is_identifier_in_es2015() {
        let mut scanner = ScannerState::new("\\u{102A7}".to_string(), true);
        scanner.set_language_version(ScriptTarget::ES2015);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::Identifier);
        assert_eq!(scanner.get_token_value_ref(), "𐊧");
    }

    #[test]
    fn unicode_escape_braced_astral_identifier_start_recovers_as_debris_in_es5() {
        let mut scanner = ScannerState::new("\\u{102A7}".to_string(), true);
        scanner.set_language_version(ScriptTarget::ES5);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::Unknown);
        assert_eq!(scanner.get_token_text(), "\\");
    }

    #[test]
    fn unicode_escape_braced_astral_identifier_tail_recovers_as_debris_in_es5() {
        let mut scanner = ScannerState::new("_\\u{102A7}".to_string(), true);
        scanner.set_language_version(ScriptTarget::ES5);

        assert_eq!(scanner.scan(), SyntaxKind::Identifier);
        assert_eq!(scanner.get_token_text(), "_");
        assert_eq!(
            scanner
                .get_scanner_diagnostics()
                .iter()
                .map(|d| (d.code, d.pos))
                .collect::<Vec<_>>(),
            vec![(
                tsz_common::diagnostics::diagnostic_codes::INVALID_CHARACTER,
                1
            )]
        );

        assert_eq!(scanner.scan(), SyntaxKind::Unknown);
        assert_eq!(scanner.get_token_text(), "\\");
        assert_eq!(scanner.scan(), SyntaxKind::Identifier);
        assert_eq!(scanner.get_token_text(), "u");
        assert_eq!(scanner.scan(), SyntaxKind::OpenBraceToken);
    }

    #[test]
    fn unicode_escape_combining_mark_not_identifier_start() {
        let mut scanner = ScannerState::new("\\u0345 = 1;".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::Unknown);
    }

    #[test]
    fn unicode_escape_mid_identifier() {
        // foo\u0042ar = "fooBar"
        let mut scanner = ScannerState::new("foo\\u0042ar".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::Identifier);
        assert_eq!(scanner.get_token_value(), "fooBar");
    }

    #[test]
    fn unicode_escape_keyword_detection() {
        // \u0069\u0066 = "if" which is a keyword
        let mut scanner = ScannerState::new("\\u0069\\u0066".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::IfKeyword);
    }

    #[test]
    fn private_identifier() {
        let mut scanner = ScannerState::new("#myField".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::PrivateIdentifier);
        assert_eq!(scanner.get_token_value(), "#myField");
    }

    #[test]
    fn private_identifier_with_underscore() {
        let mut scanner = ScannerState::new("#_internal".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::PrivateIdentifier);
        assert_eq!(scanner.get_token_value(), "#_internal");
    }

    #[test]
    fn hash_alone_is_hash_token() {
        let mut scanner = ScannerState::new("#".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::HashToken);
    }

    #[test]
    fn hash_followed_by_digit_is_hash_token() {
        // #123 - digit cannot start an identifier
        let mut scanner = ScannerState::new("#123".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::HashToken);
    }

    #[test]
    fn at_hashbang_sequence_tokenization() {
        let mut scanner = ScannerState::new("@#!x".to_string(), true);
        assert_eq!(scanner.scan(), SyntaxKind::AtToken);
        assert_eq!(scanner.scan(), SyntaxKind::HashToken);
        assert_eq!(scanner.scan(), SyntaxKind::ExclamationToken);
        assert_eq!(scanner.scan(), SyntaxKind::Identifier);
    }

    #[test]
    fn reserved_word_check() {
        let mut scanner = ScannerState::new("break".to_string(), true);
        scanner.scan();
        assert!(scanner.is_reserved_word());

        let mut scanner = ScannerState::new("with".to_string(), true);
        scanner.scan();
        assert!(scanner.is_reserved_word());
    }

    #[test]
    fn contextual_keyword_not_reserved() {
        let mut scanner = ScannerState::new("async".to_string(), true);
        scanner.scan();
        assert!(!scanner.is_reserved_word());
    }

    #[test]
    fn is_identifier_check() {
        let mut scanner = ScannerState::new("myVar".to_string(), true);
        scanner.scan();
        assert!(scanner.is_identifier());
    }

    #[test]
    fn contextual_keyword_is_identifier() {
        // Contextual keywords (async, type, etc.) are past WithKeyword so is_identifier() returns true
        let mut scanner = ScannerState::new("async".to_string(), true);
        scanner.scan();
        assert!(scanner.is_identifier());
    }
}
mod template_literal_scanning {
    use super::*;

    #[test]
    fn simple_no_substitution_template() {
        let mut scanner = ScannerState::new("`hello`".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::NoSubstitutionTemplateLiteral);
        assert_eq!(scanner.get_token_value(), "hello");
    }

    #[test]
    fn empty_template() {
        let mut scanner = ScannerState::new("``".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::NoSubstitutionTemplateLiteral);
        assert_eq!(scanner.get_token_value(), "");
    }

    #[test]
    fn template_head_with_expression() {
        let mut scanner = ScannerState::new("`hello ${".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::TemplateHead);
        assert_eq!(scanner.get_token_value(), "hello ");
    }

    #[test]
    fn template_escape_newline() {
        let mut scanner = ScannerState::new("`hello\\nworld`".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::NoSubstitutionTemplateLiteral);
        assert_eq!(scanner.get_token_value(), "hello\nworld");
    }

    #[test]
    fn template_escape_tab() {
        let mut scanner = ScannerState::new("`hello\\tworld`".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::NoSubstitutionTemplateLiteral);
        assert_eq!(scanner.get_token_value(), "hello\tworld");
    }

    #[test]
    fn template_escape_backslash() {
        let mut scanner = ScannerState::new("`back\\\\slash`".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::NoSubstitutionTemplateLiteral);
        assert_eq!(scanner.get_token_value(), "back\\slash");
    }

    #[test]
    fn template_escape_backtick() {
        let mut scanner = ScannerState::new("`back\\`tick`".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::NoSubstitutionTemplateLiteral);
        assert_eq!(scanner.get_token_value(), "back`tick");
    }

    #[test]
    fn template_escape_dollar() {
        let mut scanner = ScannerState::new("`hello\\${string}`".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::NoSubstitutionTemplateLiteral);
        assert_eq!(scanner.get_token_value(), "hello${string}");
    }

    #[test]
    fn template_escape_hex() {
        let mut scanner = ScannerState::new("`\\x41`".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::NoSubstitutionTemplateLiteral);
        assert_eq!(scanner.get_token_value(), "A");
    }

    #[test]
    fn template_escape_unicode_four_digit() {
        let mut scanner = ScannerState::new("`\\u0041`".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::NoSubstitutionTemplateLiteral);
        assert_eq!(scanner.get_token_value(), "A");
    }

    #[test]
    fn template_escape_unicode_braced() {
        let mut scanner = ScannerState::new("`\\u{1F600}`".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::NoSubstitutionTemplateLiteral);
        assert_eq!(scanner.get_token_value(), "\u{1F600}");
    }

    #[test]
    fn template_multiline() {
        let mut scanner = ScannerState::new("`line1\nline2`".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::NoSubstitutionTemplateLiteral);
        assert_eq!(scanner.get_token_value(), "line1\nline2");
    }

    #[test]
    fn unterminated_template() {
        let mut scanner = ScannerState::new("`hello".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::NoSubstitutionTemplateLiteral);
        assert_eq!(scanner.get_token_value(), "hello");
        assert!(scanner.is_unterminated());
    }

    #[test]
    fn template_rescan_tail() {
        let mut scanner = ScannerState::new("}world`".to_string(), true);
        scanner.reset_token_state(0);
        let token = scanner.re_scan_template_token(false);
        assert_eq!(token, SyntaxKind::TemplateTail);
        assert_eq!(scanner.get_token_value(), "world");
    }

    #[test]
    fn template_rescan_middle() {
        let mut scanner = ScannerState::new("}mid${".to_string(), true);
        scanner.reset_token_state(0);
        let token = scanner.re_scan_template_token(false);
        assert_eq!(token, SyntaxKind::TemplateMiddle);
        assert_eq!(scanner.get_token_value(), "mid");
    }

    #[test]
    fn template_escape_zero() {
        let mut scanner = ScannerState::new("`\\0`".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::NoSubstitutionTemplateLiteral);
        assert_eq!(scanner.get_token_value(), "\0");
    }

    #[test]
    fn template_octal_escape_is_invalid() {
        // In template literals, octal escapes (other than \0) are invalid
        let mut scanner = ScannerState::new("`\\1`".to_string(), true);
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
        let mut scanner = ScannerState::new("`line1\rline2`".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::NoSubstitutionTemplateLiteral);
        // The rescan path normalizes CR to LF; the initial scan path preserves CR.
        // Let's test via the rescan path (scan_template_and_set_token_value).
    }

    #[test]
    fn template_rescan_cr_lf_normalization() {
        // Test via re_scan_template_head_or_no_substitution_template which uses
        // scan_template_and_set_token_value that normalizes CR/CRLF to LF
        let mut scanner = ScannerState::new("`line1\r\nline2`".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::NoSubstitutionTemplateLiteral);
        // Now rescan
        let token = scanner.re_scan_template_head_or_no_substitution_template();
        assert_eq!(token, SyntaxKind::NoSubstitutionTemplateLiteral);
        assert_eq!(scanner.get_token_value(), "line1\nline2");
    }

    #[test]
    fn template_with_unicode_characters() {
        let mut scanner = ScannerState::new("`500\u{00B5}s`".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::NoSubstitutionTemplateLiteral);
        assert_eq!(scanner.get_token_value(), "500\u{00B5}s");
    }
}
mod comment_scanning {
    use super::*;

    #[test]
    fn single_line_comment_skipped_in_skip_trivia_mode() {
        let mut scanner = ScannerState::new("// comment\nfoo".to_string(), true);
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
        let mut scanner = ScannerState::new("/* comment */foo".to_string(), true);
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
        let mut scanner = ScannerState::new("// first\n// second\nfoo".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::Identifier);
        assert_eq!(scanner.get_token_value(), "foo");
    }
}
mod regex_scanning {
    use super::*;
    use tsz_scanner::scanner_impl::RegexFlagErrorKind;

    #[test]
    fn simple_regex() {
        let mut scanner = ScannerState::new("/abc/".to_string(), true);
        scanner.scan();
        let token = scanner.re_scan_slash_token();
        assert_eq!(token, SyntaxKind::RegularExpressionLiteral);
        assert_eq!(scanner.get_token_value(), "/abc/");
    }

    #[test]
    fn regex_with_all_valid_flags() {
        let mut scanner = ScannerState::new("/abc/gimsyd".to_string(), true);
        scanner.scan();
        let token = scanner.re_scan_slash_token();
        assert_eq!(token, SyntaxKind::RegularExpressionLiteral);
        assert_eq!(scanner.get_token_value(), "/abc/gimsyd");
        assert!(scanner.get_regex_flag_errors().is_empty());
    }

    #[test]
    fn regex_with_v_flag() {
        let mut scanner = ScannerState::new("/abc/v".to_string(), true);
        scanner.scan();
        let token = scanner.re_scan_slash_token();
        assert_eq!(token, SyntaxKind::RegularExpressionLiteral);
        assert_eq!(scanner.get_token_value(), "/abc/v");
        assert!(scanner.get_regex_flag_errors().is_empty());
    }

    #[test]
    fn regex_with_character_class() {
        // The / inside [...] should not end the regex
        let mut scanner = ScannerState::new("/[a/b]/".to_string(), true);
        scanner.scan();
        let token = scanner.re_scan_slash_token();
        assert_eq!(token, SyntaxKind::RegularExpressionLiteral);
        assert_eq!(scanner.get_token_value(), "/[a/b]/");
    }

    #[test]
    fn regex_with_escaped_slash() {
        let mut scanner = ScannerState::new(r"/a\/b/".to_string(), true);
        scanner.scan();
        let token = scanner.re_scan_slash_token();
        assert_eq!(token, SyntaxKind::RegularExpressionLiteral);
        assert_eq!(scanner.get_token_value(), r"/a\/b/");
    }

    #[test]
    fn regex_unterminated_at_newline() {
        let mut scanner = ScannerState::new("/abc\ndef/".to_string(), true);
        scanner.scan();
        let token = scanner.re_scan_slash_token();
        assert_eq!(token, SyntaxKind::RegularExpressionLiteral);
        assert!(scanner.is_unterminated());
    }

    #[test]
    fn regex_unterminated_at_eof() {
        let mut scanner = ScannerState::new("/abc".to_string(), true);
        scanner.scan();
        let token = scanner.re_scan_slash_token();
        assert_eq!(token, SyntaxKind::RegularExpressionLiteral);
        assert!(scanner.is_unterminated());
    }

    #[test]
    fn regex_empty_body() {
        let mut scanner = ScannerState::new("//".to_string(), true);
        let token = scanner.scan();
        // "//" is a single-line comment, not a regex
        // In skip_trivia mode, it becomes EOF
        assert_eq!(token, SyntaxKind::EndOfFileToken);
    }

    #[test]
    fn regex_from_slash_equals() {
        // /=abc/ - starts as SlashEqualsToken, rescanned as regex
        let mut scanner = ScannerState::new("/=abc/".to_string(), true);
        let token = scanner.scan();
        assert_eq!(token, SyntaxKind::SlashEqualsToken);
        let token = scanner.re_scan_slash_token();
        assert_eq!(token, SyntaxKind::RegularExpressionLiteral);
        assert_eq!(scanner.get_token_value(), "/=abc/");
    }

    #[test]
    fn regex_triple_duplicate_flags() {
        let mut scanner = ScannerState::new("/foo/ggg".to_string(), true);
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
        let mut scanner = ScannerState::new("/foo/gxz".to_string(), true);
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
        let mut scanner = ScannerState::new(r"/[\]]/".to_string(), true);
        scanner.scan();
        let token = scanner.re_scan_slash_token();
        assert_eq!(token, SyntaxKind::RegularExpressionLiteral);
        assert!(!scanner.is_unterminated());
    }
}
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
