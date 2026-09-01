//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-common/src/text_scan.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN bbde6c15767b5527603e07feead06598802c296fa63a239f6b9a69d4aedbe2f0 285 identifier_start_admits_underscore_dollar_and_letters_only
    #[test]
    fn identifier_start_admits_underscore_dollar_and_letters_only() {
        for b in *b"_$aZ" {
            assert!(is_ascii_identifier_start(b));
        }
        for b in *b"09 .-" {
            assert!(!is_ascii_identifier_start(b));
        }
        // Non-ASCII bytes are never identifier-start under the ASCII fast path.
        assert!(!is_ascii_identifier_start(0xC3));
    }
// TSZ_INLINE_TEST_END bbde6c15767b5527603e07feead06598802c296fa63a239f6b9a69d4aedbe2f0

// TSZ_INLINE_TEST_BEGIN 949e83943ebd7a02b18f7e41c5a3825af71eed196131b215f45cfe5debde4009 297 identifier_continue_additionally_admits_digits
    #[test]
    fn identifier_continue_additionally_admits_digits() {
        for b in *b"_$aZ09" {
            assert!(is_ascii_identifier_continue(b));
        }
        for b in *b" .-(" {
            assert!(!is_ascii_identifier_continue(b));
        }
    }
// TSZ_INLINE_TEST_END 949e83943ebd7a02b18f7e41c5a3825af71eed196131b215f45cfe5debde4009

// TSZ_INLINE_TEST_BEGIN cb23d03711703fd1e358c8e535f04184978b15454f0ee4b121d199586dee53c2 307 char_variants_match_byte_variants_for_ascii
    #[test]
    fn char_variants_match_byte_variants_for_ascii() {
        for c in ['_', '$', 'a', 'Z', '0', '9', ' ', '.', '-'] {
            assert_eq!(
                is_ascii_identifier_start_char(c),
                is_ascii_identifier_start(c as u8)
            );
            assert_eq!(
                is_ascii_identifier_continue_char(c),
                is_ascii_identifier_continue(c as u8)
            );
        }
        // Non-ASCII chars are rejected by the ASCII fast path.
        assert!(!is_ascii_identifier_start_char('é'));
        assert!(!is_ascii_identifier_continue_char('é'));
    }
// TSZ_INLINE_TEST_END cb23d03711703fd1e358c8e535f04184978b15454f0ee4b121d199586dee53c2

// TSZ_INLINE_TEST_BEGIN bed6d0355943adbf4da4f2ed13a14945426e74fb90d350c5796297fe1b2983c3 324 skip_quoted_basic_string_returns_past_close
    #[test]
    fn skip_quoted_basic_string_returns_past_close() {
        let s = b"'abc' rest";
        // Opens at 0, closes at index 4; returns 5 (the space).
        assert_eq!(skip_quoted_literal(s, 0, b'\''), 5);
    }
// TSZ_INLINE_TEST_END bed6d0355943adbf4da4f2ed13a14945426e74fb90d350c5796297fe1b2983c3

// TSZ_INLINE_TEST_BEGIN 55aaeb1f62a12eb69decfff8342f3a5ff5b972bdc9c5b18e34091f6baeb0c4ff 331 skip_quoted_honors_backslash_escape
    #[test]
    fn skip_quoted_honors_backslash_escape() {
        let s = br#""a\"b" rest"#;
        // The escaped quote does not close the literal; real close is index 5.
        assert_eq!(skip_quoted_literal(s, 0, b'"'), 6);
    }
// TSZ_INLINE_TEST_END 55aaeb1f62a12eb69decfff8342f3a5ff5b972bdc9c5b18e34091f6baeb0c4ff

// TSZ_INLINE_TEST_BEGIN 9295966e30866067e3aedba8f3c6da26d66185899d21bd53d9c46b378141e01f 338 skip_quoted_trailing_backslash_at_eof_clamps
    #[test]
    fn skip_quoted_trailing_backslash_at_eof_clamps() {
        let s = b"'ab\\";
        assert_eq!(skip_quoted_literal(s, 0, b'\''), s.len());
    }
// TSZ_INLINE_TEST_END 9295966e30866067e3aedba8f3c6da26d66185899d21bd53d9c46b378141e01f

// TSZ_INLINE_TEST_BEGIN 17a7cc5b54cf4fceeba933ffc257113fc09e6bfebc73c89389a4ef919fd73d0c 344 skip_quoted_single_line_terminates_on_raw_newline
    #[test]
    fn skip_quoted_single_line_terminates_on_raw_newline() {
        let s = b"'unterminated\nnext";
        // Returns the index *at* the newline (13), not EOF.
        assert_eq!(skip_quoted_literal(s, 0, b'\''), 13);
        assert_eq!(s[13], b'\n');
    }
// TSZ_INLINE_TEST_END 17a7cc5b54cf4fceeba933ffc257113fc09e6bfebc73c89389a4ef919fd73d0c

// TSZ_INLINE_TEST_BEGIN e0be43f38670b79fa709d391653c6532a24009546345e6d8c7a0783f5ac59b87 352 skip_quoted_template_spans_newlines
    #[test]
    fn skip_quoted_template_spans_newlines() {
        let s = b"`line1\nline2` rest";
        // Backtick literals are not terminated by raw newlines.
        let end = skip_quoted_literal(s, 0, b'`');
        assert_eq!(s[end - 1], b'`');
        assert_eq!(end, 13);
    }
// TSZ_INLINE_TEST_END e0be43f38670b79fa709d391653c6532a24009546345e6d8c7a0783f5ac59b87

// TSZ_INLINE_TEST_BEGIN cfc07edbbc52c62df96fbaa340ae886f916b9df7bbd6ee6334d7aa0f1da49fb4 361 skip_whitespace_skips_inline_and_line_terminators_only
    #[test]
    fn skip_whitespace_skips_inline_and_line_terminators_only() {
        let s = b" \t\r\nx";
        assert_eq!(skip_ascii_whitespace(s, 0), 4);
        // Form-feed (0x0C) is deliberately not part of the set.
        let ff = b"\x0Cx";
        assert_eq!(skip_ascii_whitespace(ff, 0), 0);
        // From past the end is a no-op clamp.
        assert_eq!(skip_ascii_whitespace(s, s.len()), s.len());
    }
// TSZ_INLINE_TEST_END cfc07edbbc52c62df96fbaa340ae886f916b9df7bbd6ee6334d7aa0f1da49fb4

// TSZ_INLINE_TEST_BEGIN 793399e50438b1d07528fb0d0769f3521155f11b9ac6437254e03b2289578223 372 standalone_token_requires_word_boundaries
    #[test]
    fn standalone_token_requires_word_boundaries() {
        assert!(contains_standalone_token("a + react + b", "react"));
        assert!(!contains_standalone_token("react_2 = 1", "react"));
        assert!(!contains_standalone_token("preact", "react"));
        assert_eq!(find_standalone_token("x react y", "react"), Some(2));
        // Token at the very start and end of input.
        assert!(contains_standalone_token("react", "react"));
        // Empty needle never matches.
        assert!(!contains_standalone_token("anything", ""));
    }
// TSZ_INLINE_TEST_END 793399e50438b1d07528fb0d0769f3521155f11b9ac6437254e03b2289578223

// TSZ_INLINE_TEST_BEGIN 69a0d26b32999bf77d1d769501327ce314832f94ba1e012960934eeaaad44319 384 skip_comment_line_stops_at_line_terminator
    #[test]
    fn skip_comment_line_stops_at_line_terminator() {
        let s = b"// comment\nrest";
        // Returns the index *at* the newline (10), not past it.
        assert_eq!(skip_comment(s, 0), Some(10));
        assert_eq!(s[10], b'\n');
    }
// TSZ_INLINE_TEST_END 69a0d26b32999bf77d1d769501327ce314832f94ba1e012960934eeaaad44319

// TSZ_INLINE_TEST_BEGIN d17c5e3359d516c8826c547d99b003f6c91d43e12148d97c8fa904a6caa29e3e 392 skip_comment_line_runs_to_eof_when_unterminated
    #[test]
    fn skip_comment_line_runs_to_eof_when_unterminated() {
        let s = b"// trailing to end";
        assert_eq!(skip_comment(s, 0), Some(s.len()));
    }
// TSZ_INLINE_TEST_END d17c5e3359d516c8826c547d99b003f6c91d43e12148d97c8fa904a6caa29e3e

// TSZ_INLINE_TEST_BEGIN a2af8e231dcbe821efe4e7ca4bc5785d8878c0213aa1e96c29a1b779cd23f007 398 skip_comment_block_returns_past_close
    #[test]
    fn skip_comment_block_returns_past_close() {
        let s = b"/* c */rest";
        // Closing `*/` ends at index 7; returns 7 (the `r`).
        assert_eq!(skip_comment(s, 0), Some(7));
        assert_eq!(&s[7..], b"rest");
    }
// TSZ_INLINE_TEST_END a2af8e231dcbe821efe4e7ca4bc5785d8878c0213aa1e96c29a1b779cd23f007

// TSZ_INLINE_TEST_BEGIN 1bdab74268bcb39ba0190052586e5d0c5f8b6104c1dec69471c59ab8ddc1f48a 406 skip_comment_block_unterminated_runs_to_eof
    #[test]
    fn skip_comment_block_unterminated_runs_to_eof() {
        let s = b"/* never closed";
        assert_eq!(skip_comment(s, 0), Some(s.len()));
    }
// TSZ_INLINE_TEST_END 1bdab74268bcb39ba0190052586e5d0c5f8b6104c1dec69471c59ab8ddc1f48a

// TSZ_INLINE_TEST_BEGIN c7e150500b548bfc7785b6320fac089cc5cf3af796e4e75ba0a1d3b86f7f8588 412 skip_comment_rejects_non_comment
    #[test]
    fn skip_comment_rejects_non_comment() {
        // Lone slash (division/regex), not a comment.
        assert_eq!(skip_comment(b"/ x", 0), None);
        // A trailing slash at EOF has no second byte.
        assert_eq!(skip_comment(b"/", 0), None);
        // Not positioned on a slash at all.
        assert_eq!(skip_comment(b"abc", 0), None);
    }
// TSZ_INLINE_TEST_END c7e150500b548bfc7785b6320fac089cc5cf3af796e4e75ba0a1d3b86f7f8588

// TSZ_INLINE_TEST_BEGIN 103ae76ede084d67bb8f196861b20f932e803efbbe22a92712f6523b3cff4536 422 skip_trivia_consumes_interleaved_whitespace_and_comments
    #[test]
    fn skip_trivia_consumes_interleaved_whitespace_and_comments() {
        // Whitespace, a line comment, more whitespace, a block comment, then
        // the first real token `x` at the end.
        let s = b"  // a\n\t/* b */ x";
        let pos = skip_trivia(s, 0);
        assert_eq!(s[pos], b'x');
        assert_eq!(pos, s.len() - 1);
    }
// TSZ_INLINE_TEST_END 103ae76ede084d67bb8f196861b20f932e803efbbe22a92712f6523b3cff4536

// TSZ_INLINE_TEST_BEGIN 48d6d30e3fd77f9593661b92cd5130ca6131658cb403f42cdbaa9906afd84e3c 432 skip_trivia_is_a_noop_on_a_real_token
    #[test]
    fn skip_trivia_is_a_noop_on_a_real_token() {
        // A lone `/` is not trivia, so the cursor does not advance.
        assert_eq!(skip_trivia(b"/ rest", 0), 0);
        assert_eq!(skip_trivia(b"xyz", 0), 0);
    }
// TSZ_INLINE_TEST_END 48d6d30e3fd77f9593661b92cd5130ca6131658cb403f42cdbaa9906afd84e3c

// TSZ_INLINE_TEST_BEGIN 3cfe47abde0051a4ffb3a43d7f48767461bd30bf7d9ea9c3348c266dc719304a 439 skip_trivia_runs_to_end_on_all_trivia
    #[test]
    fn skip_trivia_runs_to_end_on_all_trivia() {
        let s = b"  /* only */ // comment\n  ";
        assert_eq!(skip_trivia(s, 0), s.len());
    }
// TSZ_INLINE_TEST_END 3cfe47abde0051a4ffb3a43d7f48767461bd30bf7d9ea9c3348c266dc719304a

// TSZ_INLINE_TEST_BEGIN 2b7ab215ea1372d26834d3d9172b92510854b5b3203ec2a5123fd9f4f44791b0 445 ascii_caps_exactly_at_budget
    #[test]
    fn ascii_caps_exactly_at_budget() {
        assert_eq!(leading_window("hello world", 5), "hello");
    }
// TSZ_INLINE_TEST_END 2b7ab215ea1372d26834d3d9172b92510854b5b3203ec2a5123fd9f4f44791b0

// TSZ_INLINE_TEST_BEGIN b6dcc4693e97342ac0d2bd65b1bc802543b1cef58b330cb29c5c834cc70abb08 450 budget_at_or_past_len_returns_whole_string
    #[test]
    fn budget_at_or_past_len_returns_whole_string() {
        assert_eq!(leading_window("hi", 2), "hi");
        assert_eq!(leading_window("hi", 100), "hi");
    }
// TSZ_INLINE_TEST_END b6dcc4693e97342ac0d2bd65b1bc802543b1cef58b330cb29c5c834cc70abb08

// TSZ_INLINE_TEST_BEGIN 12221d84564a672fde0b60d016abfb7cf2fd3f57b28e27f9c6786728d9c04b85 456 zero_budget_returns_empty
    #[test]
    fn zero_budget_returns_empty() {
        assert_eq!(leading_window("hello", 0), "");
    }
// TSZ_INLINE_TEST_END 12221d84564a672fde0b60d016abfb7cf2fd3f57b28e27f9c6786728d9c04b85

// TSZ_INLINE_TEST_BEGIN 46ad1d8f2e2f1e66337ac1b52034d722294682b922df88abb622cc36ed7bafe1 461 empty_input_is_empty_for_any_budget
    #[test]
    fn empty_input_is_empty_for_any_budget() {
        assert_eq!(leading_window("", 0), "");
        assert_eq!(leading_window("", 4096), "");
    }
// TSZ_INLINE_TEST_END 46ad1d8f2e2f1e66337ac1b52034d722294682b922df88abb622cc36ed7bafe1

// TSZ_INLINE_TEST_BEGIN 22ba3324b4ffeb78ae529b1a516a3006616cbc401376672a453385ceaae28c6a 467 budget_mid_two_byte_codepoint_floors_back
    #[test]
    fn budget_mid_two_byte_codepoint_floors_back() {
        // 'Н' (U+041D) occupies bytes 1..3.
        let s = "aНb";
        assert_eq!(s.len(), 4);
        // Cap inside the codepoint -> floor to its start.
        assert_eq!(leading_window(s, 2), "a");
        // Cap at the codepoint end -> include it.
        assert_eq!(leading_window(s, 3), "aН");
    }
// TSZ_INLINE_TEST_END 22ba3324b4ffeb78ae529b1a516a3006616cbc401376672a453385ceaae28c6a

// TSZ_INLINE_TEST_BEGIN c85d5f71053ce8c1bf8e8570c91b04da9cdd329b4f8e8ef3ec794d4aa61595dc 478 budget_mid_four_byte_codepoint_floors_back
    #[test]
    fn budget_mid_four_byte_codepoint_floors_back() {
        // '😀' (U+1F600) occupies 4 bytes.
        let s = "x😀y";
        assert_eq!(s.len(), 6);
        assert_eq!(leading_window(s, 1), "x");
        assert_eq!(leading_window(s, 2), "x");
        assert_eq!(leading_window(s, 3), "x");
        assert_eq!(leading_window(s, 4), "x");
        assert_eq!(leading_window(s, 5), "x😀");
    }
// TSZ_INLINE_TEST_END c85d5f71053ce8c1bf8e8570c91b04da9cdd329b4f8e8ef3ec794d4aa61595dc

// TSZ_INLINE_TEST_BEGIN 6e2fd28a481b0941279c74a882a3e5aa5ead149435b6e5c511e48bddbe9de711 490 reproduces_issue_window_at_4096
    #[test]
    fn reproduces_issue_window_at_4096() {
        // Mirror the original panic: a two-byte codepoint straddling the
        // 4096-byte cap. Comment-style ASCII padding fills the run-up.
        let mut s = "/".repeat(2) + &" ".repeat(4095 - 2);
        s.push('Н'); // 2-byte 'Н' lands across bytes 4095..4097
        s.push_str(" x\nexport const x = 1;\n");
        // The cap (4096) lands inside 'Н'; must not panic and must floor back.
        let window = leading_window(&s, 4096);
        assert_eq!(window.len(), 4095);
        assert!(window.is_char_boundary(window.len()));
    }
// TSZ_INLINE_TEST_END 6e2fd28a481b0941279c74a882a3e5aa5ead149435b6e5c511e48bddbe9de711
