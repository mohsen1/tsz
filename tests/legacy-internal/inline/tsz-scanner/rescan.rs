//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-scanner/src/rescan.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 31290c4d058ca1de1d52bd39482daf5e98332bcf525f1fbe3edb4a5acddde4a0 246 greater_token_rescan_is_noop_on_non_greater_token
    #[test]
    fn greater_token_rescan_is_noop_on_non_greater_token() {
        let mut scanner = scan_one("foo");
        assert_eq!(scanner.get_token(), SyntaxKind::Identifier);
        assert_eq!(scanner.re_scan_greater_token(), SyntaxKind::Identifier);
    }
// TSZ_INLINE_TEST_END 31290c4d058ca1de1d52bd39482daf5e98332bcf525f1fbe3edb4a5acddde4a0

// TSZ_INLINE_TEST_BEGIN 69957429a4b0e7a48f5991e31a78139311a7fab35e80f552b45944f1a8d750d2 255 asterisk_equals_rescan_is_noop_on_other_tokens
    #[test]
    fn asterisk_equals_rescan_is_noop_on_other_tokens() {
        let mut scanner = scan_one("*");
        assert_eq!(scanner.get_token(), SyntaxKind::AsteriskToken);
        assert_eq!(
            scanner.re_scan_asterisk_equals_token(),
            SyntaxKind::AsteriskToken
        );
    }
// TSZ_INLINE_TEST_END 69957429a4b0e7a48f5991e31a78139311a7fab35e80f552b45944f1a8d750d2

// TSZ_INLINE_TEST_BEGIN 56bf935e8f4bcb39cae888c70a8335a0f992584f5a3a8147aecaba6ad448bcc1 267 less_than_unchanged_when_not_followed_by_slash
    #[test]
    fn less_than_unchanged_when_not_followed_by_slash() {
        let mut scanner = scan_one("<a");
        assert_eq!(scanner.get_token(), SyntaxKind::LessThanToken);
        assert_eq!(scanner.re_scan_less_than_token(), SyntaxKind::LessThanToken);
    }
// TSZ_INLINE_TEST_END 56bf935e8f4bcb39cae888c70a8335a0f992584f5a3a8147aecaba6ad448bcc1

// TSZ_INLINE_TEST_BEGIN 9e014a41d02da8c09d5842bd5cd9ba93cf4c529c6a5f81a12796a25be1c661ea 274 less_than_rescan_is_noop_on_other_tokens
    #[test]
    fn less_than_rescan_is_noop_on_other_tokens() {
        let mut scanner = scan_one(">");
        assert_eq!(scanner.get_token(), SyntaxKind::GreaterThanToken);
        assert_eq!(
            scanner.re_scan_less_than_token(),
            SyntaxKind::GreaterThanToken
        );
    }
// TSZ_INLINE_TEST_END 9e014a41d02da8c09d5842bd5cd9ba93cf4c529c6a5f81a12796a25be1c661ea

// TSZ_INLINE_TEST_BEGIN 902e7682b5d2591300e0fa444551c00c2b107f61894a0cc7ba426c053e3dec6c 290 question_widens_to_question_dot
    #[test]
    fn question_widens_to_question_dot() {
        let mut scanner = scanner_with_forced_token("?.foo", SyntaxKind::QuestionToken);
        assert_eq!(
            scanner.re_scan_question_token(),
            SyntaxKind::QuestionDotToken
        );
    }
// TSZ_INLINE_TEST_END 902e7682b5d2591300e0fa444551c00c2b107f61894a0cc7ba426c053e3dec6c

// TSZ_INLINE_TEST_BEGIN 0810195f9489239095689baeb5797c62a1a7fcb1fb40aa04ee98af0f87e7f38b 299 question_does_not_widen_when_followed_by_digit
    #[test]
    fn question_does_not_widen_when_followed_by_digit() {
        // `?.1` is the ternary `?` followed by the numeric `.1`, not `?.`.
        let mut scanner = scanner_with_forced_token("?.1", SyntaxKind::QuestionToken);
        assert_eq!(scanner.re_scan_question_token(), SyntaxKind::QuestionToken);
    }
// TSZ_INLINE_TEST_END 0810195f9489239095689baeb5797c62a1a7fcb1fb40aa04ee98af0f87e7f38b

// TSZ_INLINE_TEST_BEGIN ce805a6f8a37e0c4375527457c866d2ab7a087fe89c70371e527c85318bdabb2 306 question_widens_to_question_question
    #[test]
    fn question_widens_to_question_question() {
        let mut scanner = scanner_with_forced_token("??foo", SyntaxKind::QuestionToken);
        assert_eq!(
            scanner.re_scan_question_token(),
            SyntaxKind::QuestionQuestionToken
        );
    }
// TSZ_INLINE_TEST_END ce805a6f8a37e0c4375527457c866d2ab7a087fe89c70371e527c85318bdabb2

// TSZ_INLINE_TEST_BEGIN 4ab38467eea9db7740f86e380a98ff6e6f1c1de13153aa952982d054b6a08759 315 question_widens_to_question_question_equals
    #[test]
    fn question_widens_to_question_question_equals() {
        let mut scanner = scanner_with_forced_token("??= foo", SyntaxKind::QuestionToken);
        assert_eq!(
            scanner.re_scan_question_token(),
            SyntaxKind::QuestionQuestionEqualsToken
        );
    }
// TSZ_INLINE_TEST_END 4ab38467eea9db7740f86e380a98ff6e6f1c1de13153aa952982d054b6a08759

// TSZ_INLINE_TEST_BEGIN cc39b10799177b585ff39bd48a9982866982dfd5994a09b3a5392c53658f5d02 324 question_remains_when_alone
    #[test]
    fn question_remains_when_alone() {
        let mut scanner = scan_one("? ");
        assert_eq!(scanner.get_token(), SyntaxKind::QuestionToken);
        assert_eq!(scanner.re_scan_question_token(), SyntaxKind::QuestionToken);
    }
// TSZ_INLINE_TEST_END cc39b10799177b585ff39bd48a9982866982dfd5994a09b3a5392c53658f5d02

// TSZ_INLINE_TEST_BEGIN f5f1ad0ccfbe40165bf1ea313dbabcde38720f8324aa2b2cac0da413e59f11d5 331 question_rescan_is_noop_on_non_question
    #[test]
    fn question_rescan_is_noop_on_non_question() {
        let mut scanner = scan_one("foo");
        assert_eq!(scanner.re_scan_question_token(), SyntaxKind::Identifier);
    }
// TSZ_INLINE_TEST_END f5f1ad0ccfbe40165bf1ea313dbabcde38720f8324aa2b2cac0da413e59f11d5

// TSZ_INLINE_TEST_BEGIN ad3eab1cec47cea844bbe8a0e0a663f3874d793a502c6bae54fba4c0f947002e 343 hash_widens_to_private_identifier_when_forced
    #[test]
    fn hash_widens_to_private_identifier_when_forced() {
        let mut scanner = scanner_with_forced_token("#name", SyntaxKind::HashToken);
        assert_eq!(scanner.re_scan_hash_token(), SyntaxKind::PrivateIdentifier);
        assert_eq!(scanner.get_token_value(), "#name");
    }
// TSZ_INLINE_TEST_END ad3eab1cec47cea844bbe8a0e0a663f3874d793a502c6bae54fba4c0f947002e

// TSZ_INLINE_TEST_BEGIN b748e6b7183f45e1c54dfac6288b0def0ef9fe0a79a8d61a9110e46737ac8916 350 hash_widens_with_underscore_prefix_when_forced
    #[test]
    fn hash_widens_with_underscore_prefix_when_forced() {
        let mut scanner = scanner_with_forced_token("#_private", SyntaxKind::HashToken);
        assert_eq!(scanner.re_scan_hash_token(), SyntaxKind::PrivateIdentifier);
        assert_eq!(scanner.get_token_value(), "#_private");
    }
// TSZ_INLINE_TEST_END b748e6b7183f45e1c54dfac6288b0def0ef9fe0a79a8d61a9110e46737ac8916

// TSZ_INLINE_TEST_BEGIN 6fb3473fbcb89b9ce06847c99fbb21472cea8168b3adc5be3e905b52d80b9dbb 357 hash_remains_when_not_followed_by_identifier_start
    #[test]
    fn hash_remains_when_not_followed_by_identifier_start() {
        let mut scanner = scan_one("#1");
        assert_eq!(scanner.get_token(), SyntaxKind::HashToken);
        assert_eq!(scanner.re_scan_hash_token(), SyntaxKind::HashToken);
    }
// TSZ_INLINE_TEST_END 6fb3473fbcb89b9ce06847c99fbb21472cea8168b3adc5be3e905b52d80b9dbb

// TSZ_INLINE_TEST_BEGIN 06774dc99c9a531f80c269e4b4faf8f0eae605ae3b90edc0f6055cbc20204fa9 364 hash_rescan_is_noop_on_other_tokens
    #[test]
    fn hash_rescan_is_noop_on_other_tokens() {
        let mut scanner = scan_one("foo");
        assert_eq!(scanner.re_scan_hash_token(), SyntaxKind::Identifier);
    }
// TSZ_INLINE_TEST_END 06774dc99c9a531f80c269e4b4faf8f0eae605ae3b90edc0f6055cbc20204fa9

// TSZ_INLINE_TEST_BEGIN 49b804b3ceca17c018cec8acc4d7252f71ec39dcc7b7dd17115f9c4c64ece7cd 379 invalid_identifier_rescue_recovers_identifier_when_chars_are_valid
    #[test]
    fn invalid_identifier_rescue_recovers_identifier_when_chars_are_valid() {
        let mut scanner = forced_unknown("foo");
        assert_eq!(scanner.re_scan_invalid_identifier(), SyntaxKind::Identifier);
    }
// TSZ_INLINE_TEST_END 49b804b3ceca17c018cec8acc4d7252f71ec39dcc7b7dd17115f9c4c64ece7cd

// TSZ_INLINE_TEST_BEGIN 675ca649b466439f3e404479f8ae236bb11dd05d4c791e38ff83d953061039f0 385 invalid_identifier_rescue_recognizes_keyword
    #[test]
    fn invalid_identifier_rescue_recognizes_keyword() {
        let mut scanner = forced_unknown("class");
        assert_eq!(
            scanner.re_scan_invalid_identifier(),
            SyntaxKind::ClassKeyword
        );
    }
// TSZ_INLINE_TEST_END 675ca649b466439f3e404479f8ae236bb11dd05d4c791e38ff83d953061039f0

// TSZ_INLINE_TEST_BEGIN bb2625bdee0b37c5dc7352967e2531deea52c5914a17f02f41dbc5acefe9a743 394 invalid_identifier_rescue_rejects_non_identifier_start
    #[test]
    fn invalid_identifier_rescue_rejects_non_identifier_start() {
        // Leading digit is not a valid identifier start.
        let mut scanner = forced_unknown("1abc");
        assert_eq!(scanner.re_scan_invalid_identifier(), SyntaxKind::Unknown);
    }
// TSZ_INLINE_TEST_END bb2625bdee0b37c5dc7352967e2531deea52c5914a17f02f41dbc5acefe9a743

// TSZ_INLINE_TEST_BEGIN 180bf6b9bc9490e4f28f0daf093031635fa8fc18970dd18837dfdad0986a5b06 401 invalid_identifier_rescue_rejects_invalid_continuation
    #[test]
    fn invalid_identifier_rescue_rejects_invalid_continuation() {
        let mut scanner = forced_unknown("foo!bar");
        assert_eq!(scanner.re_scan_invalid_identifier(), SyntaxKind::Unknown);
    }
// TSZ_INLINE_TEST_END 180bf6b9bc9490e4f28f0daf093031635fa8fc18970dd18837dfdad0986a5b06

// TSZ_INLINE_TEST_BEGIN 98d5fbbd2b2115d90a76544f958417ab822310e2294fd12d49a64b7776c648b8 407 invalid_identifier_rescue_is_noop_on_empty_value
    #[test]
    fn invalid_identifier_rescue_is_noop_on_empty_value() {
        let mut scanner = ScannerState::new(String::new(), true);
        scanner.token = SyntaxKind::Unknown;
        assert_eq!(scanner.re_scan_invalid_identifier(), SyntaxKind::Unknown);
    }
// TSZ_INLINE_TEST_END 98d5fbbd2b2115d90a76544f958417ab822310e2294fd12d49a64b7776c648b8

// TSZ_INLINE_TEST_BEGIN 3584c813bf21dce287c63801b327a981427bdbe8d02e2264ef59cbcebbb60dc5 414 invalid_identifier_rescue_is_noop_when_token_is_not_unknown
    #[test]
    fn invalid_identifier_rescue_is_noop_when_token_is_not_unknown() {
        let mut scanner = scan_one("foo");
        scanner.token_value = String::from("class");
        assert_eq!(scanner.re_scan_invalid_identifier(), SyntaxKind::Identifier);
    }
// TSZ_INLINE_TEST_END 3584c813bf21dce287c63801b327a981427bdbe8d02e2264ef59cbcebbb60dc5

// TSZ_INLINE_TEST_BEGIN d18ce02fdd90f38b9a63c4760e738e1c67c692b0f14c2a03fe778659707f1986 421 unknown_identifier_name_rescue_recovers_raw_astral_token
    #[test]
    fn unknown_identifier_name_rescue_recovers_raw_astral_token() {
        let mut scanner = ScannerState::new("𐊧".to_string(), true);
        scanner.set_language_version(tsz_common::ScriptTarget::ES5);
        scanner.scan();

        assert_eq!(scanner.get_token(), SyntaxKind::Unknown);
        assert_eq!(
            scanner.re_scan_unknown_token_as_identifier_name(),
            SyntaxKind::Identifier
        );
        assert_eq!(scanner.get_token_value_ref(), "𐊧");
    }
// TSZ_INLINE_TEST_END d18ce02fdd90f38b9a63c4760e738e1c67c692b0f14c2a03fe778659707f1986

// TSZ_INLINE_TEST_BEGIN b88500ab14beb83f2a4be7fb7d6765703224360780bb5ef60ca30629893ee1db 435 unknown_identifier_name_rescue_rejects_braced_unicode_escape
    #[test]
    fn unknown_identifier_name_rescue_rejects_braced_unicode_escape() {
        let mut scanner = ScannerState::new(r"\u{102A7}".to_string(), true);
        scanner.set_language_version(tsz_common::ScriptTarget::ES5);
        scanner.scan();

        assert_eq!(scanner.get_token(), SyntaxKind::Unknown);
        assert_eq!(
            scanner.re_scan_unknown_token_as_identifier_name(),
            SyntaxKind::Unknown
        );
    }
// TSZ_INLINE_TEST_END b88500ab14beb83f2a4be7fb7d6765703224360780bb5ef60ca30629893ee1db
