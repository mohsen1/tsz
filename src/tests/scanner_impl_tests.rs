use super::*;

fn has_flag(flags: u32, flag: TokenFlags) -> bool {
    (flags & flag as u32) != 0
}

#[test]
fn test_scan_empty() {
    let mut scanner = ScannerState::new(String::new(), true);
    assert_eq!(scanner.scan(), SyntaxKind::EndOfFileToken);
}

#[test]
fn test_scan_whitespace() {
    let mut scanner = ScannerState::new("   ".to_string(), false);
    assert_eq!(scanner.scan(), SyntaxKind::WhitespaceTrivia);
    assert_eq!(scanner.scan(), SyntaxKind::EndOfFileToken);
}

#[test]
fn test_scan_whitespace_skip() {
    let mut scanner = ScannerState::new("   foo".to_string(), true);
    assert_eq!(scanner.scan(), SyntaxKind::Identifier);
    assert_eq!(scanner.get_token_value(), "foo");
}

#[test]
fn test_scan_newline() {
    let mut scanner = ScannerState::new("\n".to_string(), false);
    assert_eq!(scanner.scan(), SyntaxKind::NewLineTrivia);
    assert!(scanner.has_preceding_line_break());
}

#[test]
fn test_scan_punctuation() {
    let mut scanner = ScannerState::new("{}()[];,".to_string(), true);
    assert_eq!(scanner.scan(), SyntaxKind::OpenBraceToken);
    assert_eq!(scanner.scan(), SyntaxKind::CloseBraceToken);
    assert_eq!(scanner.scan(), SyntaxKind::OpenParenToken);
    assert_eq!(scanner.scan(), SyntaxKind::CloseParenToken);
    assert_eq!(scanner.scan(), SyntaxKind::OpenBracketToken);
    assert_eq!(scanner.scan(), SyntaxKind::CloseBracketToken);
    assert_eq!(scanner.scan(), SyntaxKind::SemicolonToken);
    assert_eq!(scanner.scan(), SyntaxKind::CommaToken);
}

#[test]
fn test_scan_operators() {
    let mut scanner = ScannerState::new("+ - * / =".to_string(), true);
    assert_eq!(scanner.scan(), SyntaxKind::PlusToken);
    assert_eq!(scanner.scan(), SyntaxKind::MinusToken);
    assert_eq!(scanner.scan(), SyntaxKind::AsteriskToken);
    assert_eq!(scanner.scan(), SyntaxKind::SlashToken);
    assert_eq!(scanner.scan(), SyntaxKind::EqualsToken);
}

#[test]
fn test_scan_compound_operators() {
    let mut scanner = ScannerState::new("=== !== == != => && || ??".to_string(), true);
    assert_eq!(scanner.scan(), SyntaxKind::EqualsEqualsEqualsToken);
    assert_eq!(scanner.scan(), SyntaxKind::ExclamationEqualsEqualsToken);
    assert_eq!(scanner.scan(), SyntaxKind::EqualsEqualsToken);
    assert_eq!(scanner.scan(), SyntaxKind::ExclamationEqualsToken);
    assert_eq!(scanner.scan(), SyntaxKind::EqualsGreaterThanToken);
    assert_eq!(scanner.scan(), SyntaxKind::AmpersandAmpersandToken);
    assert_eq!(scanner.scan(), SyntaxKind::BarBarToken);
    assert_eq!(scanner.scan(), SyntaxKind::QuestionQuestionToken);
}

#[test]
fn test_scan_string_literal() {
    let mut scanner = ScannerState::new("\"hello\"".to_string(), true);
    assert_eq!(scanner.scan(), SyntaxKind::StringLiteral);
    assert_eq!(scanner.get_token_value(), "hello");
}

#[test]
fn test_scan_string_with_escapes() {
    let mut scanner = ScannerState::new("\"hello\\nworld\"".to_string(), true);
    assert_eq!(scanner.scan(), SyntaxKind::StringLiteral);
    assert_eq!(scanner.get_token_value(), "hello\nworld");
}

#[test]
fn test_scan_single_quote_string() {
    let mut scanner = ScannerState::new("'test'".to_string(), true);
    assert_eq!(scanner.scan(), SyntaxKind::StringLiteral);
    assert_eq!(scanner.get_token_value(), "test");
}

#[test]
fn test_scan_number() {
    let mut scanner = ScannerState::new("42".to_string(), true);
    assert_eq!(scanner.scan(), SyntaxKind::NumericLiteral);
    assert_eq!(scanner.get_token_value(), "42");
}

#[test]
fn test_scan_decimal_number() {
    let mut scanner = ScannerState::new("3.14".to_string(), true);
    assert_eq!(scanner.scan(), SyntaxKind::NumericLiteral);
    assert_eq!(scanner.get_token_value(), "3.14");
}

#[test]
fn test_scan_hex_number() {
    let mut scanner = ScannerState::new("0xFF".to_string(), true);
    assert_eq!(scanner.scan(), SyntaxKind::NumericLiteral);
    assert_eq!(scanner.get_token_value(), "0xFF");
}

#[test]
fn test_scan_bigint() {
    let mut scanner = ScannerState::new("123n".to_string(), true);
    assert_eq!(scanner.scan(), SyntaxKind::BigIntLiteral);
    assert_eq!(scanner.get_token_value(), "123n");
}

#[test]
fn test_scan_numeric_separators_valid() {
    let cases = [
        "1_000",
        "0xFF_FF",
        "0b1010_0101",
        "0o12_34",
        "1_2.3_4",
        "1e2_3",
        "1_000n",
        "0xFF_FFn",
    ];

    for source in cases {
        let mut scanner = ScannerState::new(source.to_string(), true);
        let token = scanner.scan();
        assert!(matches!(
            token,
            SyntaxKind::NumericLiteral | SyntaxKind::BigIntLiteral
        ));
        let flags = scanner.get_token_flags();
        assert!(has_flag(flags, TokenFlags::ContainsSeparator));
        assert!(!has_flag(flags, TokenFlags::ContainsInvalidSeparator));
    }
}

#[test]
fn test_scan_numeric_separators_invalid() {
    let cases = [
        "1__0", "1_", "0x_FF", "1_.0", "1._0", "1e_2", "1e+_2", "0b_1",
    ];

    for source in cases {
        let mut scanner = ScannerState::new(source.to_string(), true);
        let token = scanner.scan();
        assert!(matches!(
            token,
            SyntaxKind::NumericLiteral | SyntaxKind::BigIntLiteral
        ));
        let flags = scanner.get_token_flags();
        assert!(has_flag(flags, TokenFlags::ContainsSeparator));
        assert!(has_flag(flags, TokenFlags::ContainsInvalidSeparator));
    }
}

#[test]
fn test_scan_identifier() {
    let mut scanner = ScannerState::new("myVar".to_string(), true);
    assert_eq!(scanner.scan(), SyntaxKind::Identifier);
    assert_eq!(scanner.get_token_value(), "myVar");
}

#[test]
fn test_scan_keyword() {
    let mut scanner = ScannerState::new("const".to_string(), true);
    assert_eq!(scanner.scan(), SyntaxKind::ConstKeyword);
}

#[test]
fn test_scan_let_keyword() {
    let mut scanner = ScannerState::new("let".to_string(), true);
    assert_eq!(scanner.scan(), SyntaxKind::LetKeyword);
}

#[test]
fn test_scan_comment() {
    let mut scanner = ScannerState::new("// comment\nfoo".to_string(), true);
    assert_eq!(scanner.scan(), SyntaxKind::Identifier);
    assert_eq!(scanner.get_token_value(), "foo");
}

#[test]
fn test_scan_multiline_comment() {
    let mut scanner = ScannerState::new("/* comment */foo".to_string(), true);
    assert_eq!(scanner.scan(), SyntaxKind::Identifier);
    assert_eq!(scanner.get_token_value(), "foo");
}

#[test]
fn test_scan_template_literal() {
    let mut scanner = ScannerState::new("`hello`".to_string(), true);
    assert_eq!(scanner.scan(), SyntaxKind::NoSubstitutionTemplateLiteral);
    assert_eq!(scanner.get_token_value(), "hello");
}

#[test]
fn test_scan_template_head() {
    let mut scanner = ScannerState::new("`hello ${".to_string(), true);
    assert_eq!(scanner.scan(), SyntaxKind::TemplateHead);
    assert_eq!(scanner.get_token_value(), "hello ");
}

#[test]
fn test_scan_expression() {
    let mut scanner = ScannerState::new("const x = 42;".to_string(), true);
    assert_eq!(scanner.scan(), SyntaxKind::ConstKeyword);
    assert_eq!(scanner.scan(), SyntaxKind::Identifier);
    assert_eq!(scanner.get_token_value(), "x");
    assert_eq!(scanner.scan(), SyntaxKind::EqualsToken);
    assert_eq!(scanner.scan(), SyntaxKind::NumericLiteral);
    assert_eq!(scanner.get_token_value(), "42");
    assert_eq!(scanner.scan(), SyntaxKind::SemicolonToken);
    assert_eq!(scanner.scan(), SyntaxKind::EndOfFileToken);
}

#[test]
fn test_identifier_interning() {
    use crate::interner::Atom;

    let mut scanner = ScannerState::new("foo bar foo baz foo".to_string(), true);

    // Scan first "foo"
    assert_eq!(scanner.scan(), SyntaxKind::Identifier);
    assert_eq!(scanner.get_token_value(), "foo");
    let foo_atom1 = scanner.get_token_atom();
    assert_ne!(foo_atom1, Atom::NONE);

    // Scan "bar"
    assert_eq!(scanner.scan(), SyntaxKind::Identifier);
    assert_eq!(scanner.get_token_value(), "bar");
    let bar_atom = scanner.get_token_atom();
    assert_ne!(bar_atom, Atom::NONE);
    assert_ne!(bar_atom, foo_atom1); // Different identifier = different atom

    // Scan second "foo" - should get same atom
    assert_eq!(scanner.scan(), SyntaxKind::Identifier);
    assert_eq!(scanner.get_token_value(), "foo");
    let foo_atom2 = scanner.get_token_atom();
    assert_eq!(foo_atom1, foo_atom2); // Same identifier = same atom (O(1) comparison!)

    // Scan "baz"
    assert_eq!(scanner.scan(), SyntaxKind::Identifier);
    let baz_atom = scanner.get_token_atom();
    assert_ne!(baz_atom, foo_atom1);
    assert_ne!(baz_atom, bar_atom);

    // Scan third "foo" - still same atom
    assert_eq!(scanner.scan(), SyntaxKind::Identifier);
    let foo_atom3 = scanner.get_token_atom();
    assert_eq!(foo_atom1, foo_atom3);

    // Verify we can resolve atoms back to strings
    assert_eq!(scanner.resolve_atom(foo_atom1), "foo");
    assert_eq!(scanner.resolve_atom(bar_atom), "bar");
    assert_eq!(scanner.resolve_atom(baz_atom), "baz");
}

#[test]
fn test_non_identifier_atom_is_none() {
    use crate::interner::Atom;

    let mut scanner = ScannerState::new("42 + 'hello'".to_string(), true);

    // Numeric literal - atom should be NONE
    assert_eq!(scanner.scan(), SyntaxKind::NumericLiteral);
    assert_eq!(scanner.get_token_atom(), Atom::NONE);

    // Operator - atom should be NONE
    assert_eq!(scanner.scan(), SyntaxKind::PlusToken);
    assert_eq!(scanner.get_token_atom(), Atom::NONE);

    // String literal - atom should be NONE
    assert_eq!(scanner.scan(), SyntaxKind::StringLiteral);
    assert_eq!(scanner.get_token_atom(), Atom::NONE);
}

#[test]
fn test_keyword_interning() {
    use crate::interner::Atom;

    let mut scanner = ScannerState::new("const let var const".to_string(), true);

    // Keywords are also interned (but they have their own SyntaxKind)
    assert_eq!(scanner.scan(), SyntaxKind::ConstKeyword);
    let const_atom1 = scanner.get_token_atom();
    assert_ne!(const_atom1, Atom::NONE);

    assert_eq!(scanner.scan(), SyntaxKind::LetKeyword);
    let let_atom = scanner.get_token_atom();
    assert_ne!(let_atom, const_atom1);

    assert_eq!(scanner.scan(), SyntaxKind::VarKeyword);
    let var_atom = scanner.get_token_atom();

    // Second "const" should get same atom
    assert_eq!(scanner.scan(), SyntaxKind::ConstKeyword);
    let const_atom2 = scanner.get_token_atom();
    assert_eq!(const_atom1, const_atom2);

    // All atoms are distinct
    assert_ne!(const_atom1, let_atom);
    assert_ne!(const_atom1, var_atom);
    assert_ne!(let_atom, var_atom);
}

#[test]
fn test_zero_copy_accessors() {
    let mut scanner = ScannerState::new("foo bar baz".to_string(), true);

    // Scan first identifier
    assert_eq!(scanner.scan(), SyntaxKind::Identifier);

    // Zero-copy access should return same as regular access
    assert_eq!(scanner.get_token_value_ref(), "foo");
    assert_eq!(scanner.get_token_text_ref(), "foo");

    // Scan second identifier
    assert_eq!(scanner.scan(), SyntaxKind::Identifier);
    assert_eq!(scanner.get_token_value_ref(), "bar");

    // Source slice access
    assert_eq!(scanner.source_slice(0, 3), "foo");
    assert_eq!(scanner.source_slice(4, 7), "bar");
    assert_eq!(scanner.source_text(), "foo bar baz");
}

#[test]
fn test_zero_copy_vs_allocating() {
    let mut scanner = ScannerState::new("identifier".to_string(), true);
    assert_eq!(scanner.scan(), SyntaxKind::Identifier);

    // Both methods should return the same value
    let allocated = scanner.get_token_value();
    let zero_copy = scanner.get_token_value_ref();

    assert_eq!(allocated, zero_copy);
    assert_eq!(zero_copy, "identifier");
}

#[test]
fn test_unicode_escape_mid_identifier() {
    // Test unicode escape mid-identifier: C\u0032 should scan as "C2"
    let mut scanner = ScannerState::new("C\\u0032".to_string(), true);
    assert_eq!(scanner.scan(), SyntaxKind::Identifier);
    assert_eq!(scanner.get_token_value_ref(), "C2");
    assert!(has_flag(
        scanner.get_token_flags(),
        TokenFlags::UnicodeEscape
    ));
}

#[test]
fn test_unicode_escape_mid_identifier_multiple() {
    // Multiple unicode escapes: ab\u0063\u0064ef should scan as "abcdef"
    let mut scanner = ScannerState::new("ab\\u0063\\u0064ef".to_string(), true);
    assert_eq!(scanner.scan(), SyntaxKind::Identifier);
    assert_eq!(scanner.get_token_value_ref(), "abcdef");
    assert!(has_flag(
        scanner.get_token_flags(),
        TokenFlags::UnicodeEscape
    ));
}

#[test]
fn test_unicode_escape_mid_identifier_extended() {
    // Extended unicode escape: x\u{61} should scan as "xa"
    let mut scanner = ScannerState::new("x\\u{61}".to_string(), true);
    assert_eq!(scanner.scan(), SyntaxKind::Identifier);
    assert_eq!(scanner.get_token_value_ref(), "xa");
    assert!(has_flag(
        scanner.get_token_flags(),
        TokenFlags::UnicodeEscape
    ));
}

#[test]
fn test_unicode_escape_mid_identifier_digit() {
    // Unicode escape producing digit: foo\u0030 should scan as "foo0" (\u0030 is '0')
    let mut scanner = ScannerState::new("foo\\u0030".to_string(), true);
    assert_eq!(scanner.scan(), SyntaxKind::Identifier);
    assert_eq!(scanner.get_token_value_ref(), "foo0");
    assert!(has_flag(
        scanner.get_token_flags(),
        TokenFlags::UnicodeEscape
    ));
}

#[test]
fn test_unicode_escape_not_identifier_part() {
    // Unicode escape that is NOT an identifier part stops the identifier
    // \u002B is '+' which is not valid in identifiers
    // So "foo\u002B" should scan "foo" then stop
    let mut scanner = ScannerState::new("foo\\u002B".to_string(), true);
    assert_eq!(scanner.scan(), SyntaxKind::Identifier);
    assert_eq!(scanner.get_token_value_ref(), "foo");
    // Next should be an invalid character (the backslash)
    // The remaining "\u002B" is not consumed as part of the identifier
}
