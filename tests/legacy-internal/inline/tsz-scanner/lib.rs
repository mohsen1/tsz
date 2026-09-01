//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-scanner/src/lib.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN a62d740d2c6c5c14791bacb490375719aa2a17331f5c382949efc9b149465ce2 965 try_from_u16_valid_range
    #[test]
    fn try_from_u16_valid_range() {
        assert_eq!(SyntaxKind::try_from_u16(0), Some(SyntaxKind::Unknown));
        assert_eq!(
            SyntaxKind::try_from_u16(1),
            Some(SyntaxKind::EndOfFileToken)
        );
        assert_eq!(
            SyntaxKind::try_from_u16(9),
            Some(SyntaxKind::NumericLiteral)
        );
        assert_eq!(SyntaxKind::try_from_u16(80), Some(SyntaxKind::Identifier));
        assert_eq!(
            SyntaxKind::try_from_u16(166),
            Some(SyntaxKind::DeferKeyword)
        );
    }
// TSZ_INLINE_TEST_END a62d740d2c6c5c14791bacb490375719aa2a17331f5c382949efc9b149465ce2

// TSZ_INLINE_TEST_BEGIN aa163fe24f5dfe797001748bf05995bada3e059757c118eaf118d63222da6608 983 try_from_u16_out_of_range
    #[test]
    fn try_from_u16_out_of_range() {
        assert_eq!(SyntaxKind::try_from_u16(167), None);
        assert_eq!(SyntaxKind::try_from_u16(200), None);
        assert_eq!(SyntaxKind::try_from_u16(u16::MAX), None);
    }
// TSZ_INLINE_TEST_END aa163fe24f5dfe797001748bf05995bada3e059757c118eaf118d63222da6608

// TSZ_INLINE_TEST_BEGIN c4dcf21a87fba9400dfa631fd914bf33e17402d692cf999ff53aa28d6b900f67 992 keyword_classification
    #[test]
    fn keyword_classification() {
        assert!(token_is_keyword(SyntaxKind::BreakKeyword));
        assert!(token_is_keyword(SyntaxKind::IfKeyword));
        assert!(token_is_keyword(SyntaxKind::ClassKeyword));
        assert!(token_is_keyword(SyntaxKind::DeferKeyword)); // last keyword
        assert!(token_is_keyword(SyntaxKind::AsyncKeyword));
        assert!(token_is_keyword(SyntaxKind::LetKeyword));
        assert!(token_is_keyword(SyntaxKind::YieldKeyword));

        assert!(!token_is_keyword(SyntaxKind::Identifier));
        assert!(!token_is_keyword(SyntaxKind::NumericLiteral));
        assert!(!token_is_keyword(SyntaxKind::OpenBraceToken));
        assert!(!token_is_keyword(SyntaxKind::EndOfFileToken));
    }
// TSZ_INLINE_TEST_END c4dcf21a87fba9400dfa631fd914bf33e17402d692cf999ff53aa28d6b900f67

// TSZ_INLINE_TEST_BEGIN 6301211101613b15c4a50206332bb91c405a79510854faf02445472d8ccce434 1010 reserved_word_classification
    #[test]
    fn reserved_word_classification() {
        // Reserved words: break..with
        assert!(token_is_reserved_word(SyntaxKind::BreakKeyword));
        assert!(token_is_reserved_word(SyntaxKind::WithKeyword));
        assert!(token_is_reserved_word(SyntaxKind::ReturnKeyword));
        assert!(token_is_reserved_word(SyntaxKind::ClassKeyword));

        // Strict mode reserved words are NOT reserved words
        assert!(!token_is_reserved_word(SyntaxKind::ImplementsKeyword));
        assert!(!token_is_reserved_word(SyntaxKind::YieldKeyword));
        // Contextual keywords are NOT reserved words
        assert!(!token_is_reserved_word(SyntaxKind::AsyncKeyword));
        assert!(!token_is_reserved_word(SyntaxKind::TypeKeyword));
        assert!(!token_is_reserved_word(SyntaxKind::Identifier));
    }
// TSZ_INLINE_TEST_END 6301211101613b15c4a50206332bb91c405a79510854faf02445472d8ccce434

// TSZ_INLINE_TEST_BEGIN 4659b82fde6afc8ec259a0886f89b0ea314bf86e949c03547d4f6dcc2270d2a3 1029 strict_mode_reserved_word_classification
    #[test]
    fn strict_mode_reserved_word_classification() {
        assert!(token_is_strict_mode_reserved_word(
            SyntaxKind::ImplementsKeyword
        ));
        assert!(token_is_strict_mode_reserved_word(
            SyntaxKind::InterfaceKeyword
        ));
        assert!(token_is_strict_mode_reserved_word(SyntaxKind::LetKeyword));
        assert!(token_is_strict_mode_reserved_word(SyntaxKind::YieldKeyword));

        assert!(!token_is_strict_mode_reserved_word(
            SyntaxKind::BreakKeyword
        ));
        assert!(!token_is_strict_mode_reserved_word(
            SyntaxKind::AsyncKeyword
        ));
        assert!(!token_is_strict_mode_reserved_word(SyntaxKind::Identifier));
    }
// TSZ_INLINE_TEST_END 4659b82fde6afc8ec259a0886f89b0ea314bf86e949c03547d4f6dcc2270d2a3

// TSZ_INLINE_TEST_BEGIN 1926863f6ad80334022e3508c5d4dfeddcf637f6be53d8d91490305e80989e5b 1051 literal_classification
    #[test]
    fn literal_classification() {
        assert!(token_is_literal(SyntaxKind::NumericLiteral));
        assert!(token_is_literal(SyntaxKind::BigIntLiteral));
        assert!(token_is_literal(SyntaxKind::StringLiteral));
        assert!(token_is_literal(SyntaxKind::RegularExpressionLiteral));
        assert!(token_is_literal(SyntaxKind::NoSubstitutionTemplateLiteral));

        assert!(!token_is_literal(SyntaxKind::TemplateHead));
        assert!(!token_is_literal(SyntaxKind::Identifier));
        assert!(!token_is_literal(SyntaxKind::BreakKeyword));
    }
// TSZ_INLINE_TEST_END 1926863f6ad80334022e3508c5d4dfeddcf637f6be53d8d91490305e80989e5b

// TSZ_INLINE_TEST_BEGIN 58a0fbeffe13328c6e3960eb8f99cbcfa518b07f75d33db0eab8e52344fd4e30 1066 template_literal_classification
    #[test]
    fn template_literal_classification() {
        assert!(token_is_template_literal(
            SyntaxKind::NoSubstitutionTemplateLiteral
        ));
        assert!(token_is_template_literal(SyntaxKind::TemplateHead));
        assert!(token_is_template_literal(SyntaxKind::TemplateMiddle));
        assert!(token_is_template_literal(SyntaxKind::TemplateTail));

        assert!(!token_is_template_literal(SyntaxKind::StringLiteral));
        assert!(!token_is_template_literal(SyntaxKind::NumericLiteral));
    }
// TSZ_INLINE_TEST_END 58a0fbeffe13328c6e3960eb8f99cbcfa518b07f75d33db0eab8e52344fd4e30

// TSZ_INLINE_TEST_BEGIN a9ce7e43890080801b9cb4df8c71576e52142f903bf706f892a53ad404d52b3a 1081 punctuation_classification
    #[test]
    fn punctuation_classification() {
        assert!(token_is_punctuation(SyntaxKind::OpenBraceToken));
        assert!(token_is_punctuation(SyntaxKind::SemicolonToken));
        assert!(token_is_punctuation(SyntaxKind::PlusToken));
        assert!(token_is_punctuation(SyntaxKind::EqualsToken));
        assert!(token_is_punctuation(SyntaxKind::CaretEqualsToken)); // last punctuation

        assert!(!token_is_punctuation(SyntaxKind::Identifier));
        assert!(!token_is_punctuation(SyntaxKind::NumericLiteral));
        assert!(!token_is_punctuation(SyntaxKind::BreakKeyword));
    }
// TSZ_INLINE_TEST_END a9ce7e43890080801b9cb4df8c71576e52142f903bf706f892a53ad404d52b3a

// TSZ_INLINE_TEST_BEGIN dac2a4538831275a29a2d536304694f5566b1dc593b6c1ce5a040027ae18e0b6 1096 assignment_operator_classification
    #[test]
    fn assignment_operator_classification() {
        assert!(token_is_assignment_operator(SyntaxKind::EqualsToken));
        assert!(token_is_assignment_operator(SyntaxKind::PlusEqualsToken));
        assert!(token_is_assignment_operator(
            SyntaxKind::AsteriskAsteriskEqualsToken
        ));
        assert!(token_is_assignment_operator(SyntaxKind::BarBarEqualsToken));
        assert!(token_is_assignment_operator(
            SyntaxKind::QuestionQuestionEqualsToken
        ));
        assert!(token_is_assignment_operator(SyntaxKind::CaretEqualsToken));

        assert!(!token_is_assignment_operator(SyntaxKind::PlusToken));
        assert!(!token_is_assignment_operator(SyntaxKind::EqualsEqualsToken));
        assert!(!token_is_assignment_operator(SyntaxKind::Identifier));
    }
// TSZ_INLINE_TEST_END dac2a4538831275a29a2d536304694f5566b1dc593b6c1ce5a040027ae18e0b6

// TSZ_INLINE_TEST_BEGIN 018da470349d1a2563eb3ef66e6d58f49e05cf1d46d8e6b3c447ee1ada11b76f 1116 trivia_classification
    #[test]
    fn trivia_classification() {
        assert!(token_is_trivia(SyntaxKind::SingleLineCommentTrivia));
        assert!(token_is_trivia(SyntaxKind::MultiLineCommentTrivia));
        assert!(token_is_trivia(SyntaxKind::NewLineTrivia));
        assert!(token_is_trivia(SyntaxKind::WhitespaceTrivia));
        assert!(token_is_trivia(SyntaxKind::ShebangTrivia));
        assert!(token_is_trivia(SyntaxKind::ConflictMarkerTrivia));
        assert!(token_is_trivia(SyntaxKind::NonTextFileMarkerTrivia));

        assert!(!token_is_trivia(SyntaxKind::Unknown));
        assert!(!token_is_trivia(SyntaxKind::EndOfFileToken));
        assert!(!token_is_trivia(SyntaxKind::NumericLiteral));
    }
// TSZ_INLINE_TEST_END 018da470349d1a2563eb3ef66e6d58f49e05cf1d46d8e6b3c447ee1ada11b76f

// TSZ_INLINE_TEST_BEGIN 1fc6273313ab3a48c95b36fee0d2a8a7683326b0bd19cf78beb2daf2fcc90c26 1133 identifier_or_keyword_classification
    #[test]
    fn identifier_or_keyword_classification() {
        assert!(token_is_identifier_or_keyword(SyntaxKind::Identifier));
        assert!(token_is_identifier_or_keyword(SyntaxKind::BreakKeyword));
        assert!(token_is_identifier_or_keyword(SyntaxKind::AsyncKeyword));
        assert!(token_is_identifier_or_keyword(SyntaxKind::DeferKeyword));
        // PrivateIdentifier is between Identifier and keywords
        assert!(token_is_identifier_or_keyword(
            SyntaxKind::PrivateIdentifier
        ));

        assert!(!token_is_identifier_or_keyword(SyntaxKind::NumericLiteral));
        assert!(!token_is_identifier_or_keyword(SyntaxKind::OpenBraceToken));
        assert!(!token_is_identifier_or_keyword(SyntaxKind::EndOfFileToken));
    }
// TSZ_INLINE_TEST_END 1fc6273313ab3a48c95b36fee0d2a8a7683326b0bd19cf78beb2daf2fcc90c26

// TSZ_INLINE_TEST_BEGIN 01a5d2295c210a9cc217a27d081544fa8adde579ca9988173ecf5eba07137f79 1151 text_to_keyword_all_reserved_words
    #[test]
    fn text_to_keyword_all_reserved_words() {
        let cases = [
            ("break", SyntaxKind::BreakKeyword),
            ("case", SyntaxKind::CaseKeyword),
            ("class", SyntaxKind::ClassKeyword),
            ("const", SyntaxKind::ConstKeyword),
            ("function", SyntaxKind::FunctionKeyword),
            ("if", SyntaxKind::IfKeyword),
            ("return", SyntaxKind::ReturnKeyword),
            ("this", SyntaxKind::ThisKeyword),
            ("typeof", SyntaxKind::TypeOfKeyword),
            ("var", SyntaxKind::VarKeyword),
            ("void", SyntaxKind::VoidKeyword),
            ("while", SyntaxKind::WhileKeyword),
            ("with", SyntaxKind::WithKeyword),
        ];
        for (text, expected) in cases {
            assert_eq!(
                text_to_keyword(text),
                Some(expected),
                "text_to_keyword({text:?})"
            );
        }
    }
// TSZ_INLINE_TEST_END 01a5d2295c210a9cc217a27d081544fa8adde579ca9988173ecf5eba07137f79

// TSZ_INLINE_TEST_BEGIN 3e191076103f67d8d811acca021b54f9c3b04dd47d62ca4ae742f910207720c4 1177 text_to_keyword_contextual_keywords
    #[test]
    fn text_to_keyword_contextual_keywords() {
        let cases = [
            ("async", SyntaxKind::AsyncKeyword),
            ("await", SyntaxKind::AwaitKeyword),
            ("type", SyntaxKind::TypeKeyword),
            ("declare", SyntaxKind::DeclareKeyword),
            ("abstract", SyntaxKind::AbstractKeyword),
            ("as", SyntaxKind::AsKeyword),
            ("satisfies", SyntaxKind::SatisfiesKeyword),
            ("keyof", SyntaxKind::KeyOfKeyword),
            ("infer", SyntaxKind::InferKeyword),
            ("readonly", SyntaxKind::ReadonlyKeyword),
            ("override", SyntaxKind::OverrideKeyword),
            ("defer", SyntaxKind::DeferKeyword),
        ];
        for (text, expected) in cases {
            assert_eq!(
                text_to_keyword(text),
                Some(expected),
                "text_to_keyword({text:?})"
            );
        }
    }
// TSZ_INLINE_TEST_END 3e191076103f67d8d811acca021b54f9c3b04dd47d62ca4ae742f910207720c4

// TSZ_INLINE_TEST_BEGIN 9aec57f1d8f1682e33ef98f99215e1d7c74b03e6f17f6a3e59f560f4f15d63cb 1202 text_to_keyword_non_keywords
    #[test]
    fn text_to_keyword_non_keywords() {
        assert_eq!(text_to_keyword("foo"), None);
        assert_eq!(text_to_keyword("bar"), None);
        assert_eq!(text_to_keyword(""), None);
        assert_eq!(text_to_keyword("IF"), None); // case sensitive
        assert_eq!(text_to_keyword("Class"), None); // case sensitive
    }
// TSZ_INLINE_TEST_END 9aec57f1d8f1682e33ef98f99215e1d7c74b03e6f17f6a3e59f560f4f15d63cb

// TSZ_INLINE_TEST_BEGIN 57a76b4357ead556d2cc2511d7c48bfec87805516d25ee92116e41f5105b35ee 1211 keyword_to_text_roundtrip
    #[test]
    fn keyword_to_text_roundtrip() {
        // Every keyword should roundtrip: text_to_keyword(keyword_to_text(k)) == Some(k)
        let keywords = [
            SyntaxKind::BreakKeyword,
            SyntaxKind::CaseKeyword,
            SyntaxKind::CatchKeyword,
            SyntaxKind::ClassKeyword,
            SyntaxKind::ConstKeyword,
            SyntaxKind::IfKeyword,
            SyntaxKind::ReturnKeyword,
            SyntaxKind::AsyncKeyword,
            SyntaxKind::AwaitKeyword,
            SyntaxKind::TypeKeyword,
            SyntaxKind::LetKeyword,
            SyntaxKind::YieldKeyword,
            SyntaxKind::DeferKeyword,
            SyntaxKind::SatisfiesKeyword,
        ];
        for kw in keywords {
            let text = keyword_to_text_static(kw).expect("keyword should have text");
            let roundtripped = text_to_keyword(text);
            assert_eq!(
                roundtripped,
                Some(kw),
                "roundtrip failed for {kw:?} -> {text:?}"
            );
        }
    }
// TSZ_INLINE_TEST_END 57a76b4357ead556d2cc2511d7c48bfec87805516d25ee92116e41f5105b35ee

// TSZ_INLINE_TEST_BEGIN d908b6c5dbb47b0d7403e6db07c0b3ca96fbda7eed6c630db364c8970f508130 1241 keyword_to_text_non_keywords
    #[test]
    fn keyword_to_text_non_keywords() {
        assert_eq!(keyword_to_text_static(SyntaxKind::Identifier), None);
        assert_eq!(keyword_to_text_static(SyntaxKind::NumericLiteral), None);
        assert_eq!(keyword_to_text_static(SyntaxKind::OpenBraceToken), None);
    }
// TSZ_INLINE_TEST_END d908b6c5dbb47b0d7403e6db07c0b3ca96fbda7eed6c630db364c8970f508130

// TSZ_INLINE_TEST_BEGIN 37712da9cd81b500bb03be78d3c46baedcb409925fe2b72890047504d1fcf2f9 1250 punctuation_to_text_basics
    #[test]
    fn punctuation_to_text_basics() {
        assert_eq!(
            punctuation_to_text_static(SyntaxKind::OpenBraceToken),
            Some("{")
        );
        assert_eq!(
            punctuation_to_text_static(SyntaxKind::CloseBraceToken),
            Some("}")
        );
        assert_eq!(
            punctuation_to_text_static(SyntaxKind::SemicolonToken),
            Some(";")
        );
        assert_eq!(
            punctuation_to_text_static(SyntaxKind::DotDotDotToken),
            Some("...")
        );
        assert_eq!(
            punctuation_to_text_static(SyntaxKind::EqualsGreaterThanToken),
            Some("=>")
        );
        assert_eq!(
            punctuation_to_text_static(SyntaxKind::QuestionQuestionToken),
            Some("??")
        );
        assert_eq!(
            punctuation_to_text_static(SyntaxKind::AsteriskAsteriskToken),
            Some("**")
        );
        assert_eq!(
            punctuation_to_text_static(SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken),
            Some(">>>=")
        );
    }
// TSZ_INLINE_TEST_END 37712da9cd81b500bb03be78d3c46baedcb409925fe2b72890047504d1fcf2f9

// TSZ_INLINE_TEST_BEGIN c2bd3ef6b57ec3bb3aaa92ed0b480e70386eff6861f3dff9085f53ca4033ca94 1286 punctuation_to_text_non_punctuation
    #[test]
    fn punctuation_to_text_non_punctuation() {
        assert_eq!(punctuation_to_text_static(SyntaxKind::Identifier), None);
        assert_eq!(punctuation_to_text_static(SyntaxKind::BreakKeyword), None);
    }
// TSZ_INLINE_TEST_END c2bd3ef6b57ec3bb3aaa92ed0b480e70386eff6861f3dff9085f53ca4033ca94

// TSZ_INLINE_TEST_BEGIN c93851023100be59fa77637aa37c874f452fb0c7b67bb2d8f37be148e8ef4cd7 1294 string_to_token_keywords_and_identifiers
    #[test]
    fn string_to_token_keywords_and_identifiers() {
        assert_eq!(string_to_token("if"), SyntaxKind::IfKeyword);
        assert_eq!(string_to_token("class"), SyntaxKind::ClassKeyword);
        assert_eq!(string_to_token("async"), SyntaxKind::AsyncKeyword);
        assert_eq!(string_to_token("myVariable"), SyntaxKind::Identifier);
        assert_eq!(string_to_token("_foo"), SyntaxKind::Identifier);
        assert_eq!(string_to_token("$bar"), SyntaxKind::Identifier);
    }
// TSZ_INLINE_TEST_END c93851023100be59fa77637aa37c874f452fb0c7b67bb2d8f37be148e8ef4cd7

// TSZ_INLINE_TEST_BEGIN ecb774d0668e57e19bb13db4a11122bded1850ddcefdef6067c3336c3d2fc71d 1306 syntax_kind_constants_are_consistent
    #[test]
    fn syntax_kind_constants_are_consistent() {
        assert!(SyntaxKind::FIRST_KEYWORD as u16 <= SyntaxKind::LAST_KEYWORD as u16);
        assert!(SyntaxKind::FIRST_PUNCTUATION as u16 <= SyntaxKind::LAST_PUNCTUATION as u16);
        assert!(SyntaxKind::FIRST_LITERAL_TOKEN as u16 <= SyntaxKind::LAST_LITERAL_TOKEN as u16);
        assert!(SyntaxKind::FIRST_TEMPLATE_TOKEN as u16 <= SyntaxKind::LAST_TEMPLATE_TOKEN as u16);
        assert!(SyntaxKind::FIRST_RESERVED_WORD as u16 <= SyntaxKind::LAST_RESERVED_WORD as u16);

        // Verify boundary relationships
        assert_eq!(SyntaxKind::FIRST_TOKEN, SyntaxKind::Unknown);
        assert_eq!(SyntaxKind::LAST_TOKEN, SyntaxKind::DeferKeyword);
        assert_eq!(SyntaxKind::FIRST_KEYWORD, SyntaxKind::BreakKeyword);
        assert_eq!(SyntaxKind::LAST_KEYWORD, SyntaxKind::DeferKeyword);
    }
// TSZ_INLINE_TEST_END ecb774d0668e57e19bb13db4a11122bded1850ddcefdef6067c3336c3d2fc71d

// TSZ_INLINE_TEST_BEGIN 3e4a963daf87c60b77ec45b287da3d9ceed70a55a15aa525294a8b1edf06327a 1323 kind_by_value_table_is_identity
    #[test]
    fn kind_by_value_table_is_identity() {
        // Every entry in KIND_BY_VALUE should match its index
        for (i, &kind) in KIND_BY_VALUE.iter().enumerate() {
            assert_eq!(
                kind as u16, i as u16,
                "KIND_BY_VALUE[{i}] = {:?} (value {}), expected value {i}",
                kind, kind as u16
            );
        }
    }
// TSZ_INLINE_TEST_END 3e4a963daf87c60b77ec45b287da3d9ceed70a55a15aa525294a8b1edf06327a
