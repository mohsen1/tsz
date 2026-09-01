//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-core/src/api/wasm/core_utils.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 8e2ad407cf5c41cc4164efdad4da58df1d48023d3acb77b89069c2f56593b13b 387 compare_strings_case_sensitive_none_handling
    #[test]
    fn compare_strings_case_sensitive_none_handling() {
        assert_eq!(
            compare_strings_case_sensitive(None, None),
            Comparison::EqualTo
        );
        assert_eq!(
            compare_strings_case_sensitive(None, Some(String::from("a"))),
            Comparison::LessThan
        );
        assert_eq!(
            compare_strings_case_sensitive(Some(String::from("a")), None),
            Comparison::GreaterThan
        );
    }
// TSZ_INLINE_TEST_END 8e2ad407cf5c41cc4164efdad4da58df1d48023d3acb77b89069c2f56593b13b

// TSZ_INLINE_TEST_BEGIN a124b10a39a64ac63e609319e0dc609cd3a71ef6532ae926e026d8dc9fe7dbad 403 compare_strings_case_sensitive_orders_by_unicode_code_point
    #[test]
    fn compare_strings_case_sensitive_orders_by_unicode_code_point() {
        // 'B' (66) is less than 'a' (97) under ordinal comparison.
        assert_eq!(
            compare_strings_case_sensitive(Some("B".into()), Some("a".into())),
            Comparison::LessThan,
        );
        assert_eq!(
            compare_strings_case_sensitive(Some("abc".into()), Some("abc".into())),
            Comparison::EqualTo,
        );
    }
// TSZ_INLINE_TEST_END a124b10a39a64ac63e609319e0dc609cd3a71ef6532ae926e026d8dc9fe7dbad

// TSZ_INLINE_TEST_BEGIN 75d79ec583450c33980115cfe65922bd63d8e58c477fd609e250ce8a8372099b 418 compare_strings_case_insensitive_treats_case_alike
    #[test]
    fn compare_strings_case_insensitive_treats_case_alike() {
        assert_eq!(
            compare_strings_case_insensitive(Some("ABC".into()), Some("abc".into())),
            Comparison::EqualTo,
        );
    }
// TSZ_INLINE_TEST_END 75d79ec583450c33980115cfe65922bd63d8e58c477fd609e250ce8a8372099b

// TSZ_INLINE_TEST_BEGIN f57d51639bd0ffbbc73c856f5605721bffcb0e9e2a73d4a564efe304ec9d8f6c 426 compare_strings_case_insensitive_none_handling
    #[test]
    fn compare_strings_case_insensitive_none_handling() {
        assert_eq!(
            compare_strings_case_insensitive(None, None),
            Comparison::EqualTo
        );
        assert_eq!(
            compare_strings_case_insensitive(None, Some(String::from("a"))),
            Comparison::LessThan
        );
        assert_eq!(
            compare_strings_case_insensitive(Some(String::from("a")), None),
            Comparison::GreaterThan
        );
    }
// TSZ_INLINE_TEST_END f57d51639bd0ffbbc73c856f5605721bffcb0e9e2a73d4a564efe304ec9d8f6c

// TSZ_INLINE_TEST_BEGIN d78c4d767dadd88e0a0ff0ad7a8ff7e219aaa49a6243ab61eb4957228b2e81b5 444 equate_strings_case_sensitive_distinguishes_case
    #[test]
    fn equate_strings_case_sensitive_distinguishes_case() {
        assert!(equate_strings_case_sensitive("abc", "abc"));
        assert!(!equate_strings_case_sensitive("abc", "ABC"));
    }
// TSZ_INLINE_TEST_END d78c4d767dadd88e0a0ff0ad7a8ff7e219aaa49a6243ab61eb4957228b2e81b5

// TSZ_INLINE_TEST_BEGIN 2ee3c8f2945bb5796045e02e8eb6e5c95d5b32e1fee81dcecd3ed6a5fbc1b10e 450 equate_strings_case_insensitive_collapses_case
    #[test]
    fn equate_strings_case_insensitive_collapses_case() {
        assert!(equate_strings_case_insensitive("abc", "ABC"));
        assert!(equate_strings_case_insensitive("Hello", "hELLO"));
        assert!(!equate_strings_case_insensitive("abc", "abd"));
    }
// TSZ_INLINE_TEST_END 2ee3c8f2945bb5796045e02e8eb6e5c95d5b32e1fee81dcecd3ed6a5fbc1b10e

// TSZ_INLINE_TEST_BEGIN a37bc61aaeeb5d127ff4a1ead07fef1233629124d58e91b6b35e5d0ba772b096 459 is_any_directory_separator_accepts_both_slashes
    #[test]
    fn is_any_directory_separator_accepts_both_slashes() {
        assert!(is_any_directory_separator(b'/' as u32));
        assert!(is_any_directory_separator(b'\\' as u32));
        assert!(!is_any_directory_separator(b'a' as u32));
        assert!(!is_any_directory_separator(b'.' as u32));
    }
// TSZ_INLINE_TEST_END a37bc61aaeeb5d127ff4a1ead07fef1233629124d58e91b6b35e5d0ba772b096

// TSZ_INLINE_TEST_BEGIN b535cb2e5b70dd8886efc4877374f90468312fc533ed655142539209fd31785d 469 normalize_slashes_replaces_backslashes_with_forward
    #[test]
    fn normalize_slashes_replaces_backslashes_with_forward() {
        assert_eq!(normalize_slashes("a\\b\\c"), "a/b/c");
    }
// TSZ_INLINE_TEST_END b535cb2e5b70dd8886efc4877374f90468312fc533ed655142539209fd31785d

// TSZ_INLINE_TEST_BEGIN b552d976b7df19e5f70b85d975b825aaea69d3d161353fad3b7721e13006fcc9 474 normalize_slashes_returns_input_when_no_backslashes
    #[test]
    fn normalize_slashes_returns_input_when_no_backslashes() {
        // Optimization branch: returns owned copy of input.
        assert_eq!(normalize_slashes("a/b/c"), "a/b/c");
        assert_eq!(normalize_slashes(""), "");
    }
// TSZ_INLINE_TEST_END b552d976b7df19e5f70b85d975b825aaea69d3d161353fad3b7721e13006fcc9

// TSZ_INLINE_TEST_BEGIN c73575c4c7e7ed09bee6043e18da95fe3451012cf01985e9c80a7a26bc1f08a8 483 has_trailing_directory_separator_branches
    #[test]
    fn has_trailing_directory_separator_branches() {
        assert!(has_trailing_directory_separator("a/"));
        assert!(has_trailing_directory_separator("a\\"));
        assert!(!has_trailing_directory_separator("a"));
        assert!(!has_trailing_directory_separator(""));
    }
// TSZ_INLINE_TEST_END c73575c4c7e7ed09bee6043e18da95fe3451012cf01985e9c80a7a26bc1f08a8

// TSZ_INLINE_TEST_BEGIN 930256d11be30c47a1ca87b5206857c032e40a7b6072d83f848b7435ee318a6d 493 path_is_relative_recognizes_dot_and_dot_dot_prefixes
    #[test]
    fn path_is_relative_recognizes_dot_and_dot_dot_prefixes() {
        for p in &["./", ".\\", "../", "..\\", ".", ".."] {
            assert!(path_is_relative(p), "expected relative: {p:?}");
        }
    }
// TSZ_INLINE_TEST_END 930256d11be30c47a1ca87b5206857c032e40a7b6072d83f848b7435ee318a6d

// TSZ_INLINE_TEST_BEGIN ffa1158ffd3e21bf71bbb20ed195ac0061fc8025457870348060dbe836092611 500 path_is_relative_rejects_absolute_and_bare_names
    #[test]
    fn path_is_relative_rejects_absolute_and_bare_names() {
        for p in &["/a", "C:\\a", "a/b", "..a", ".a"] {
            assert!(!path_is_relative(p), "expected NOT relative: {p:?}");
        }
    }
// TSZ_INLINE_TEST_END ffa1158ffd3e21bf71bbb20ed195ac0061fc8025457870348060dbe836092611

// TSZ_INLINE_TEST_BEGIN 71b412352a7a4a77d009f4b2ce92dd78555095f9212abce2dc95b0e9cae932c5 509 remove_trailing_directory_separator_strips_one_separator
    #[test]
    fn remove_trailing_directory_separator_strips_one_separator() {
        assert_eq!(remove_trailing_directory_separator("a/"), "a");
        assert_eq!(remove_trailing_directory_separator("a\\"), "a");
        assert_eq!(remove_trailing_directory_separator("a"), "a");
        // length <= 1 is a guard branch — single-char paths are returned as-is.
        assert_eq!(remove_trailing_directory_separator("/"), "/");
    }
// TSZ_INLINE_TEST_END 71b412352a7a4a77d009f4b2ce92dd78555095f9212abce2dc95b0e9cae932c5

// TSZ_INLINE_TEST_BEGIN 58e9cc916ce74899d84e2783035ed8dba004cc058b8e7542946feedcf345442a 518 ensure_trailing_directory_separator_adds_one_when_missing
    #[test]
    fn ensure_trailing_directory_separator_adds_one_when_missing() {
        assert_eq!(ensure_trailing_directory_separator("a"), "a/");
        // Already has one — return unchanged.
        assert_eq!(ensure_trailing_directory_separator("a/"), "a/");
        assert_eq!(ensure_trailing_directory_separator("a\\"), "a\\");
    }
// TSZ_INLINE_TEST_END 58e9cc916ce74899d84e2783035ed8dba004cc058b8e7542946feedcf345442a

// TSZ_INLINE_TEST_BEGIN 4aa520e73d78b8409e74e801e79ebb6a804d04d1d1d30146131a1d40c9628bae 528 has_extension_via_basename
    #[test]
    fn has_extension_via_basename() {
        assert!(has_extension("a.ts"));
        assert!(!has_extension("a"));
        // A dot in the directory should NOT count — only the basename matters.
        assert!(!has_extension("a.dir/file"));
    }
// TSZ_INLINE_TEST_END 4aa520e73d78b8409e74e801e79ebb6a804d04d1d1d30146131a1d40c9628bae

// TSZ_INLINE_TEST_BEGIN 0aaf1ab690260098fd04593478edea8b4dff99da3390d70e88a77ec8e0a20a32 536 get_base_file_name_extracts_last_segment
    #[test]
    fn get_base_file_name_extracts_last_segment() {
        assert_eq!(get_base_file_name("a/b/c.ts"), "c.ts");
        assert_eq!(get_base_file_name("c.ts"), "c.ts");
        // Trailing-separator handling.
        assert_eq!(get_base_file_name("a/b/"), "b");
        // Backslash is normalized.
        assert_eq!(get_base_file_name("a\\b\\c"), "c");
    }
// TSZ_INLINE_TEST_END 0aaf1ab690260098fd04593478edea8b4dff99da3390d70e88a77ec8e0a20a32

// TSZ_INLINE_TEST_BEGIN 835773e3366db480c65eb651b6abc5ec00fd9c928a5fb99279941c142dcefaf8 546 file_extension_is_strict_about_path_length
    #[test]
    fn file_extension_is_strict_about_path_length() {
        // Path strictly LONGER than extension required.
        assert!(file_extension_is("a.ts", ".ts"));
        // Equal length → false (means the path IS the extension).
        assert!(!file_extension_is(".ts", ".ts"));
        // Mismatch → false.
        assert!(!file_extension_is("a.tsx", ".ts"));
    }
// TSZ_INLINE_TEST_END 835773e3366db480c65eb651b6abc5ec00fd9c928a5fb99279941c142dcefaf8

// TSZ_INLINE_TEST_BEGIN f011df8eba57248edc9f9fba7ed44831358cd55d1bfe74fdd557b2a7cfc5442b 558 to_file_name_lower_case_basic_ascii
    #[test]
    fn to_file_name_lower_case_basic_ascii() {
        assert_eq!(to_file_name_lower_case("ABC.TS"), "abc.ts");
        // Already-lowercase + safe chars short-circuits to clone.
        assert_eq!(to_file_name_lower_case("abc.ts"), "abc.ts");
    }
// TSZ_INLINE_TEST_END f011df8eba57248edc9f9fba7ed44831358cd55d1bfe74fdd557b2a7cfc5442b

// TSZ_INLINE_TEST_BEGIN 3937c2ad8c7a702de8caaec6bb51e6e1c28cd6d3e715fc2c8242df9293d27711 565 to_file_name_lower_case_preserves_special_unicode
    #[test]
    fn to_file_name_lower_case_preserves_special_unicode() {
        // \u{0130} (İ), \u{0131} (ı), \u{00DF} (ß) intentionally NOT lowercased.
        assert_eq!(to_file_name_lower_case("\u{0130}"), "\u{0130}");
        assert_eq!(to_file_name_lower_case("\u{0131}"), "\u{0131}");
        assert_eq!(to_file_name_lower_case("\u{00DF}"), "\u{00DF}");
    }
// TSZ_INLINE_TEST_END 3937c2ad8c7a702de8caaec6bb51e6e1c28cd6d3e715fc2c8242df9293d27711

// TSZ_INLINE_TEST_BEGIN fd67bcce97ce996f3c887a0c5f51562fc51bf7c1cf55928d901947d3dc9ae34a 575 is_line_break_recognizes_lf_cr_ls_ps
    #[test]
    fn is_line_break_recognizes_lf_cr_ls_ps() {
        assert!(is_line_break(0x0A)); // \n
        assert!(is_line_break(0x0D)); // \r
        assert!(is_line_break(0x2028)); // line separator
        assert!(is_line_break(0x2029)); // paragraph separator
        assert!(!is_line_break(b' ' as u32));
    }
// TSZ_INLINE_TEST_END fd67bcce97ce996f3c887a0c5f51562fc51bf7c1cf55928d901947d3dc9ae34a

// TSZ_INLINE_TEST_BEGIN 9ddc0a53de125913b3d7d2e462fb0c84e4239a813658ce61b573589625b567d4 584 is_white_space_single_line_includes_horizontal_only
    #[test]
    fn is_white_space_single_line_includes_horizontal_only() {
        // Spaces and tabs — yes.
        assert!(is_white_space_single_line(b' ' as u32));
        assert!(is_white_space_single_line(b'\t' as u32));
        // Newlines — NO (those are line breaks, not single-line whitespace).
        assert!(!is_white_space_single_line(0x0A));
    }
// TSZ_INLINE_TEST_END 9ddc0a53de125913b3d7d2e462fb0c84e4239a813658ce61b573589625b567d4

// TSZ_INLINE_TEST_BEGIN ab8e6aac51f2d410ff96481cef9bb794b0ca9d759cd9514c6818d9b8801f4eab 593 is_white_space_like_includes_both_newlines_and_horizontal
    #[test]
    fn is_white_space_like_includes_both_newlines_and_horizontal() {
        // Horizontal whitespace.
        assert!(is_white_space_like(b' ' as u32));
        // Newlines also count.
        assert!(is_white_space_like(0x0A));
        // Letters do not.
        assert!(!is_white_space_like(b'a' as u32));
    }
// TSZ_INLINE_TEST_END ab8e6aac51f2d410ff96481cef9bb794b0ca9d759cd9514c6818d9b8801f4eab

// TSZ_INLINE_TEST_BEGIN 6057d0cb1538c321cb3796d0520296113ebf7081fb492f73962043de16bd3646 605 is_digit_octal_hex_letter_word_classifications
    #[test]
    fn is_digit_octal_hex_letter_word_classifications() {
        assert!(is_digit(b'0' as u32));
        assert!(is_digit(b'9' as u32));
        assert!(!is_digit(b'a' as u32));

        assert!(is_octal_digit(b'0' as u32));
        assert!(is_octal_digit(b'7' as u32));
        assert!(!is_octal_digit(b'8' as u32));
        assert!(!is_octal_digit(b'9' as u32));

        assert!(is_hex_digit(b'0' as u32));
        assert!(is_hex_digit(b'9' as u32));
        assert!(is_hex_digit(b'a' as u32));
        assert!(is_hex_digit(b'f' as u32));
        assert!(is_hex_digit(b'F' as u32));
        assert!(!is_hex_digit(b'g' as u32));

        assert!(is_ascii_letter(b'a' as u32));
        assert!(is_ascii_letter(b'Z' as u32));
        assert!(!is_ascii_letter(b'0' as u32));
        assert!(!is_ascii_letter(b'_' as u32));

        // word = letter | digit | underscore.
        assert!(is_word_character(b'a' as u32));
        assert!(is_word_character(b'0' as u32));
        assert!(is_word_character(b'_' as u32));
        assert!(!is_word_character(b'-' as u32));
        assert!(!is_word_character(b' ' as u32));
    }
// TSZ_INLINE_TEST_END 6057d0cb1538c321cb3796d0520296113ebf7081fb492f73962043de16bd3646
