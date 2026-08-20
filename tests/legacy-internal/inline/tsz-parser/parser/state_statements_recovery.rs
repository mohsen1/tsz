//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-parser/src/parser/state_statements_recovery.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 82b0f8cd2f2b88e0d90944ab340a3939965fa8dbda2e06dcb972b70b6082f71d 114 non_bracket_next_is_never_invalid
    #[test]
    fn non_bracket_next_is_never_invalid() {
        for next in [
            SyntaxKind::OpenBraceToken,
            SyntaxKind::Identifier,
            SyntaxKind::Unknown,
        ] {
            assert!(
                !is_invalid_let_array_start(next, SyntaxKind::NumericLiteral),
                "next={next:?} should not trigger the invalid-let-array predicate",
            );
        }
    }
// TSZ_INLINE_TEST_END 82b0f8cd2f2b88e0d90944ab340a3939965fa8dbda2e06dcb972b70b6082f71d

// TSZ_INLINE_TEST_BEGIN 09e5a487f81aea40aaa1028c42a5d01a617a08f741589bfe31144b86c41ad946 128 close_bracket_first_elem_is_recoverable
    #[test]
    fn close_bracket_first_elem_is_recoverable() {
        assert!(!is_invalid_let_array_start(
            SyntaxKind::OpenBracketToken,
            SyntaxKind::CloseBracketToken,
        ));
    }
// TSZ_INLINE_TEST_END 09e5a487f81aea40aaa1028c42a5d01a617a08f741589bfe31144b86c41ad946

// TSZ_INLINE_TEST_BEGIN 00ae3fb792b4567261d3bda785984a697f5dc382e1c72e52edb5a6ec7c86a1ca 136 comma_first_elem_is_recoverable
    #[test]
    fn comma_first_elem_is_recoverable() {
        assert!(!is_invalid_let_array_start(
            SyntaxKind::OpenBracketToken,
            SyntaxKind::CommaToken,
        ));
    }
// TSZ_INLINE_TEST_END 00ae3fb792b4567261d3bda785984a697f5dc382e1c72e52edb5a6ec7c86a1ca

// TSZ_INLINE_TEST_BEGIN 97de87d9e1f26c72b368493da0b94d499ad09b7dcc628e77f2216b57b33de09b 144 dot_dot_dot_first_elem_is_recoverable
    #[test]
    fn dot_dot_dot_first_elem_is_recoverable() {
        assert!(!is_invalid_let_array_start(
            SyntaxKind::OpenBracketToken,
            SyntaxKind::DotDotDotToken,
        ));
    }
// TSZ_INLINE_TEST_END 97de87d9e1f26c72b368493da0b94d499ad09b7dcc628e77f2216b57b33de09b

// TSZ_INLINE_TEST_BEGIN ea4fb646bdb7c7c8fcc1597891749a70707dc036b01088f2f67a5341bb0547d9 152 open_brace_first_elem_is_recoverable
    #[test]
    fn open_brace_first_elem_is_recoverable() {
        assert!(!is_invalid_let_array_start(
            SyntaxKind::OpenBracketToken,
            SyntaxKind::OpenBraceToken,
        ));
    }
// TSZ_INLINE_TEST_END ea4fb646bdb7c7c8fcc1597891749a70707dc036b01088f2f67a5341bb0547d9

// TSZ_INLINE_TEST_BEGIN 8a6ca977a60d1dad298d8125c82b73b942f86e5333b2624b50dc1acaa4b91b14 160 open_bracket_first_elem_is_recoverable
    #[test]
    fn open_bracket_first_elem_is_recoverable() {
        assert!(!is_invalid_let_array_start(
            SyntaxKind::OpenBracketToken,
            SyntaxKind::OpenBracketToken,
        ));
    }
// TSZ_INLINE_TEST_END 8a6ca977a60d1dad298d8125c82b73b942f86e5333b2624b50dc1acaa4b91b14

// TSZ_INLINE_TEST_BEGIN fde4f348f8c21a9cbcb91eef7e160ef6285936338f6e94b346eebbd7cb80061b 168 identifier_first_elem_is_recoverable
    #[test]
    fn identifier_first_elem_is_recoverable() {
        assert!(!is_invalid_let_array_start(
            SyntaxKind::OpenBracketToken,
            SyntaxKind::Identifier,
        ));
    }
// TSZ_INLINE_TEST_END fde4f348f8c21a9cbcb91eef7e160ef6285936338f6e94b346eebbd7cb80061b

// TSZ_INLINE_TEST_BEGIN cbdd7e8afcedc11ea835dffa657d3e377f4c317f909afcc5dbd9ab82774afb0d 176 reserved_word_first_elem_is_invalid
    #[test]
    fn reserved_word_first_elem_is_invalid() {
        assert!(is_invalid_let_array_start(
            SyntaxKind::OpenBracketToken,
            SyntaxKind::WhileKeyword,
        ));
    }
// TSZ_INLINE_TEST_END cbdd7e8afcedc11ea835dffa657d3e377f4c317f909afcc5dbd9ab82774afb0d

// TSZ_INLINE_TEST_BEGIN a22904e90c7418f717d626cc073c18ea816c224a50a39cdb7914f29aeb3627b9 184 for_keyword_first_elem_is_invalid
    #[test]
    fn for_keyword_first_elem_is_invalid() {
        assert!(is_invalid_let_array_start(
            SyntaxKind::OpenBracketToken,
            SyntaxKind::ForKeyword,
        ));
    }
// TSZ_INLINE_TEST_END a22904e90c7418f717d626cc073c18ea816c224a50a39cdb7914f29aeb3627b9

// TSZ_INLINE_TEST_BEGIN c58a300b3dfd1a13818ecf92dcc06bc54e59a6ceccd08211e187a7ff53ff8550 192 numeric_literal_first_elem_is_invalid
    #[test]
    fn numeric_literal_first_elem_is_invalid() {
        assert!(is_invalid_let_array_start(
            SyntaxKind::OpenBracketToken,
            SyntaxKind::NumericLiteral,
        ));
    }
// TSZ_INLINE_TEST_END c58a300b3dfd1a13818ecf92dcc06bc54e59a6ceccd08211e187a7ff53ff8550

// TSZ_INLINE_TEST_BEGIN 98ebcd8fdb6a21c31b8d499af941b274c1498d357625a93c8653cd275ce2ccfa 200 string_literal_first_elem_is_invalid
    #[test]
    fn string_literal_first_elem_is_invalid() {
        assert!(is_invalid_let_array_start(
            SyntaxKind::OpenBracketToken,
            SyntaxKind::StringLiteral,
        ));
    }
// TSZ_INLINE_TEST_END 98ebcd8fdb6a21c31b8d499af941b274c1498d357625a93c8653cd275ce2ccfa

// TSZ_INLINE_TEST_BEGIN 356ec8035f825c43021ef8eb35f7820bc14cd83c400f67ad4993ea0b6e84d6cc 208 plus_token_first_elem_is_invalid
    #[test]
    fn plus_token_first_elem_is_invalid() {
        assert!(is_invalid_let_array_start(
            SyntaxKind::OpenBracketToken,
            SyntaxKind::PlusToken,
        ));
    }
// TSZ_INLINE_TEST_END 356ec8035f825c43021ef8eb35f7820bc14cd83c400f67ad4993ea0b6e84d6cc
