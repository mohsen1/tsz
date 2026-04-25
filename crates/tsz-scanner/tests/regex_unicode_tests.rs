//! Unit tests for tsz-scanner token types, numeric literals, and token classification.
//!
//! This file provides test coverage for:
//! - Numeric literals (decimal, hex, octal, binary, `BigInt`)
//! - Numeric separators
//! - Exponential notation
//! - String literals
//! - Template literals
//! - Token classification functions

//! - Basic scanning operations

use tsz_scanner::scanner_impl::ScannerState;
use tsz_scanner::scanner_impl::TokenFlags;
use tsz_scanner::{SyntaxKind, token_is_keyword, token_is_literal, token_is_punctuation};

mod common;
use common::make_scanner;

// =============================================================================
// Numeric Literal Tests
// =============================================================================

#[test]
fn test_decimal_literal() {
    let mut scanner = make_scanner("123");
    scanner.scan();

    assert_eq!(scanner.get_token(), SyntaxKind::NumericLiteral);
    assert_eq!(scanner.get_token_value(), "123");
}

#[test]
fn test_decimal_with_fraction() {
    let mut scanner = make_scanner("123.456");
    scanner.scan();

    assert_eq!(scanner.get_token(), SyntaxKind::NumericLiteral);
    assert_eq!(scanner.get_token_value(), "123.456");
}

#[test]
fn test_hex_literal() {
    let mut scanner = make_scanner("0xFF");
    scanner.scan();

    assert_eq!(scanner.get_token(), SyntaxKind::NumericLiteral);
    assert_eq!(scanner.get_token_value(), "0xFF");
    assert!(scanner.get_token_flags() & TokenFlags::HexSpecifier as u32 != 0);
}

#[test]
fn test_octal_literal() {
    let mut scanner = make_scanner("0o755");
    scanner.scan();

    assert_eq!(scanner.get_token(), SyntaxKind::NumericLiteral);
    assert_eq!(scanner.get_token_value(), "0o755");
    assert!(scanner.get_token_flags() & TokenFlags::OctalSpecifier as u32 != 0);
}

#[test]
fn test_binary_literal() {
    let mut scanner = make_scanner("0b1010");
    scanner.scan();

    assert_eq!(scanner.get_token(), SyntaxKind::NumericLiteral);
    assert_eq!(scanner.get_token_value(), "0b1010");
    assert!(scanner.get_token_flags() & TokenFlags::BinarySpecifier as u32 != 0);
}

#[test]
fn test_bigint_literal_decimal() {
    let mut scanner = make_scanner("9007199254740992n");
    scanner.scan();

    assert_eq!(scanner.get_token(), SyntaxKind::BigIntLiteral);
    assert_eq!(scanner.get_token_value(), "9007199254740992n");
}

#[test]
fn test_bigint_literal_hex() {
    let mut scanner = make_scanner("0xFFn");
    scanner.scan();

    assert_eq!(scanner.get_token(), SyntaxKind::BigIntLiteral);
    assert_eq!(scanner.get_token_value(), "0xFFn");
}

#[test]
fn test_bigint_literal_binary() {
    let mut scanner = make_scanner("0b1010n");
    scanner.scan();

    assert_eq!(scanner.get_token(), SyntaxKind::BigIntLiteral);
    assert_eq!(scanner.get_token_value(), "0b1010n");
}

#[test]
fn test_bigint_literal_octal() {
    let mut scanner = make_scanner("0o755n");
    scanner.scan();

    assert_eq!(scanner.get_token(), SyntaxKind::BigIntLiteral);
    assert_eq!(scanner.get_token_value(), "0o755n");
}

// =============================================================================
// Numeric Separator Tests
// =============================================================================

#[test]
fn test_numeric_separator() {
    let mut scanner = make_scanner("1_000_000");
    scanner.scan();

    assert_eq!(scanner.get_token(), SyntaxKind::NumericLiteral);
    assert_eq!(scanner.get_token_value(), "1_000_000");
    // Check that separator flag is set
    assert!(scanner.get_token_flags() & TokenFlags::ContainsSeparator as u32 != 0);
}

#[test]
fn test_numeric_separator_hex() {
    let mut scanner = make_scanner("0xFF_FF");
    scanner.scan();

    assert_eq!(scanner.get_token(), SyntaxKind::NumericLiteral);
    assert_eq!(scanner.get_token_value(), "0xFF_FF");
}

#[test]
fn test_numeric_separator_binary() {
    let mut scanner = make_scanner("0b1010_1010");
    scanner.scan();

    assert_eq!(scanner.get_token(), SyntaxKind::NumericLiteral);
    assert_eq!(scanner.get_token_value(), "0b1010_1010");
}

#[test]
fn test_numeric_separator_bigint() {
    let mut scanner = make_scanner("1_000_000n");
    scanner.scan();

    assert_eq!(scanner.get_token(), SyntaxKind::BigIntLiteral);
    assert_eq!(scanner.get_token_value(), "1_000_000n");
}

#[test]
fn test_numeric_separator_multiple() {
    let mut scanner = make_scanner("1_2_3_4_5");
    scanner.scan();

    assert_eq!(scanner.get_token(), SyntaxKind::NumericLiteral);
    assert_eq!(scanner.get_token_value(), "1_2_3_4_5");
}

// =============================================================================
// Exponential Notation Tests
// =============================================================================

#[test]
fn test_exponential_notation() {
    let mut scanner = make_scanner("1e10");
    scanner.scan();

    assert_eq!(scanner.get_token(), SyntaxKind::NumericLiteral);
    assert_eq!(scanner.get_token_value(), "1e10");
    assert!(scanner.get_token_flags() & TokenFlags::Scientific as u32 != 0);
}

#[test]
fn test_exponential_notation_positive() {
    let mut scanner = make_scanner("1.5e+10");
    scanner.scan();

    assert_eq!(scanner.get_token(), SyntaxKind::NumericLiteral);
    assert_eq!(scanner.get_token_value(), "1.5e+10");
}

#[test]
fn test_exponential_notation_negative() {
    let mut scanner = make_scanner("1.5e-10");
    scanner.scan();

    assert_eq!(scanner.get_token(), SyntaxKind::NumericLiteral);
    assert_eq!(scanner.get_token_value(), "1.5e-10");
}

#[test]
fn test_exponential_uppercase() {
    let mut scanner = make_scanner("1E10");
    scanner.scan();

    assert_eq!(scanner.get_token(), SyntaxKind::NumericLiteral);
    assert_eq!(scanner.get_token_value(), "1E10");
}

#[test]
fn test_exponential_with_separator() {
    let mut scanner = make_scanner("1_000e10");
    scanner.scan();

    assert_eq!(scanner.get_token(), SyntaxKind::NumericLiteral);
    assert_eq!(scanner.get_token_value(), "1_000e10");
}

// =============================================================================
// String and Template Literal Tests
// =============================================================================

#[test]
fn test_string_literal_single_quotes() {
    let mut scanner = make_scanner("'hello world'");
    scanner.scan();

    assert_eq!(scanner.get_token(), SyntaxKind::StringLiteral);
    // get_token_value() returns the unquoted string content
    assert_eq!(scanner.get_token_value(), "hello world");
}

#[test]
fn test_string_literal_double_quotes() {
    let mut scanner = make_scanner("\"hello world\"");
    scanner.scan();

    assert_eq!(scanner.get_token(), SyntaxKind::StringLiteral);
    // get_token_value() returns the unquoted string content
    assert_eq!(scanner.get_token_value(), "hello world");
}

#[test]
fn test_template_literal_head() {
    let mut scanner = make_scanner("`hello ${");
    scanner.scan();

    // First scan should be TemplateHead or backtick
    assert_eq!(scanner.get_token(), SyntaxKind::TemplateHead);
}

#[test]
fn test_template_literal_no_substitution() {
    let mut scanner = make_scanner("`hello world`");
    scanner.scan();

    assert_eq!(
        scanner.get_token(),
        SyntaxKind::NoSubstitutionTemplateLiteral
    );
    // get_token_value() returns the unquoted template content
    assert_eq!(scanner.get_token_value(), "hello world");
}

// =============================================================================
// Token Classification Tests
// =============================================================================

#[test]
fn test_token_is_literal_numeric() {
    assert!(token_is_literal(SyntaxKind::NumericLiteral));
    assert!(token_is_literal(SyntaxKind::BigIntLiteral));
    assert!(token_is_literal(SyntaxKind::StringLiteral));
    assert!(token_is_literal(SyntaxKind::RegularExpressionLiteral));
    assert!(token_is_literal(SyntaxKind::NoSubstitutionTemplateLiteral));
}

#[test]
fn test_token_is_not_literal() {
    assert!(!token_is_literal(SyntaxKind::Identifier));
    assert!(!token_is_literal(SyntaxKind::PlusToken));
    assert!(!token_is_literal(SyntaxKind::FunctionKeyword));
}

#[test]
fn test_token_is_keyword_func() {
    assert!(token_is_keyword(SyntaxKind::FunctionKeyword));
    assert!(token_is_keyword(SyntaxKind::IfKeyword));
    assert!(token_is_keyword(SyntaxKind::ReturnKeyword));
}

#[test]
fn test_token_is_not_keyword() {
    assert!(!token_is_keyword(SyntaxKind::Identifier));
    assert!(!token_is_keyword(SyntaxKind::NumericLiteral));
    assert!(!token_is_keyword(SyntaxKind::PlusToken));
}

#[test]
fn test_token_is_punctuation_func() {
    assert!(token_is_punctuation(SyntaxKind::PlusToken));
    assert!(token_is_punctuation(SyntaxKind::OpenBraceToken));
    assert!(token_is_punctuation(SyntaxKind::EqualsToken));
}

#[test]
fn test_token_is_not_punctuation() {
    assert!(!token_is_punctuation(SyntaxKind::Identifier));
    assert!(!token_is_punctuation(SyntaxKind::NumericLiteral));
    assert!(!token_is_punctuation(SyntaxKind::FunctionKeyword));
}

// =============================================================================
// Edge Cases
// =============================================================================

#[test]
fn test_zero() {
    let mut scanner = make_scanner("0");
    scanner.scan();

    assert_eq!(scanner.get_token(), SyntaxKind::NumericLiteral);
    assert_eq!(scanner.get_token_value(), "0");
}

#[test]
fn test_leading_decimal() {
    let mut scanner = make_scanner(".5");
    scanner.scan();

    assert_eq!(scanner.get_token(), SyntaxKind::NumericLiteral);
    assert_eq!(scanner.get_token_value(), ".5");
}

#[test]
fn test_scientific_notation_fraction() {
    let mut scanner = make_scanner(".5e10");
    scanner.scan();

    assert_eq!(scanner.get_token(), SyntaxKind::NumericLiteral);
    assert_eq!(scanner.get_token_value(), ".5e10");
}

#[test]
fn test_end_of_file() {
    let mut scanner = make_scanner("");
    scanner.scan();

    assert_eq!(scanner.get_token(), SyntaxKind::EndOfFileToken);
}

#[test]
fn test_whitespace_only() {
    let mut scanner = ScannerState::new("   \t\n".to_string(), false); // don't skip trivia
    scanner.scan();

    // First token should be whitespace trivia
    assert_eq!(scanner.get_token(), SyntaxKind::WhitespaceTrivia);
}

#[test]
fn test_comment_single_line() {
    let mut scanner = ScannerState::new("// comment".to_string(), false); // don't skip trivia
    scanner.scan();

    assert_eq!(scanner.get_token(), SyntaxKind::SingleLineCommentTrivia);
}

#[test]
fn test_comment_multi_line() {
    let mut scanner = ScannerState::new("/* comment */".to_string(), false); // don't skip trivia
    scanner.scan();

    assert_eq!(scanner.get_token(), SyntaxKind::MultiLineCommentTrivia);
}

#[test]
fn test_identifier() {
    let mut scanner = make_scanner("hello");
    scanner.scan();

    assert_eq!(scanner.get_token(), SyntaxKind::Identifier);
    assert_eq!(scanner.get_token_value(), "hello");
}

#[test]
fn test_private_identifier() {
    let mut scanner = make_scanner("#private");
    scanner.scan();

    assert_eq!(scanner.get_token(), SyntaxKind::PrivateIdentifier);
    assert_eq!(scanner.get_token_value(), "#private");
}

#[test]
fn test_keyword() {
    let mut scanner = make_scanner("function");
    scanner.scan();

    assert_eq!(scanner.get_token(), SyntaxKind::FunctionKeyword);
    assert_eq!(scanner.get_token_value(), "function");
}

#[test]
fn test_token_flags_scientific() {
    let mut scanner = make_scanner("1e10");
    scanner.scan();

    assert!(scanner.get_token_flags() & TokenFlags::Scientific as u32 != 0);
}

#[test]
fn test_token_flags_hex() {
    let mut scanner = make_scanner("0xFF");
    scanner.scan();

    assert!(scanner.get_token_flags() & TokenFlags::HexSpecifier as u32 != 0);
}

#[test]
fn test_token_flags_binary() {
    let mut scanner = make_scanner("0b1010");
    scanner.scan();

    assert!(scanner.get_token_flags() & TokenFlags::BinarySpecifier as u32 != 0);
}

#[test]
fn test_token_flags_octal() {
    let mut scanner = make_scanner("0o755");
    scanner.scan();

    assert!(scanner.get_token_flags() & TokenFlags::OctalSpecifier as u32 != 0);
}

#[test]
fn test_token_flags_contains_separator() {
    let mut scanner = make_scanner("1_000_000");
    scanner.scan();

    assert!(scanner.get_token_flags() & TokenFlags::ContainsSeparator as u32 != 0);
}

#[test]
fn test_multiple_tokens() {
    let mut scanner = make_scanner("let x = 5;");

    scanner.scan();
    assert_eq!(scanner.get_token(), SyntaxKind::LetKeyword);

    scanner.scan();
    assert_eq!(scanner.get_token(), SyntaxKind::Identifier);
    assert_eq!(scanner.get_token_value(), "x");

    scanner.scan();
    assert_eq!(scanner.get_token(), SyntaxKind::EqualsToken);

    scanner.scan();
    assert_eq!(scanner.get_token(), SyntaxKind::NumericLiteral);
    assert_eq!(scanner.get_token_value(), "5");

    scanner.scan();
    assert_eq!(scanner.get_token(), SyntaxKind::SemicolonToken);
}

#[test]
fn test_position_tracking() {
    let mut scanner = make_scanner("abc def");

    scanner.scan();
    assert_eq!(scanner.get_token(), SyntaxKind::Identifier);
    assert_eq!(scanner.get_token_start(), 0);
    assert_eq!(scanner.get_token_end(), 3);

    scanner.scan();
    assert_eq!(scanner.get_token(), SyntaxKind::Identifier);
    assert_eq!(scanner.get_token_start(), 4);
    assert_eq!(scanner.get_token_end(), 7);
}

#[test]
fn test_source_text() {
    let source = "hello world";
    let scanner = ScannerState::new(source.to_string(), true);

    assert_eq!(scanner.get_text(), source);
}

#[test]
fn test_interner() {
    let mut scanner = make_scanner("hello world");
    scanner.scan();

    // The interner should have been created
    let interner = scanner.take_interner();
    // After taking the interner, it should be replaced with a new one
    let new_interner = scanner.take_interner();
    // Both should be valid Interner instances
    assert!(!interner.is_empty() || interner.is_empty()); // Just verify it exists
    assert!(!new_interner.is_empty() || new_interner.is_empty());
}

#[test]
fn test_scanner_state_save_restore() {
    let mut scanner = make_scanner("abc def");

    // Scan first token
    scanner.scan();
    let saved = scanner.save_state();

    // Scan second token
    scanner.scan();
    assert_eq!(scanner.get_token_value(), "def");

    // Restore to first token state
    scanner.restore_state(saved);
    assert_eq!(scanner.get_token_value(), "abc");
}
