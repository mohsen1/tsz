//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-common/src/diagnostics/mod.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN ee5115924c5381202f4a2e63aa65cad03983fb8be801d3f4d67389c76068f2e0 497 generated_table_codes_are_strictly_increasing
    #[test]
    fn generated_table_codes_are_strictly_increasing() {
        // `lookup_diagnostic` binary-searches each section, so the generated
        // table must be globally sorted by code with no duplicates.
        let codes: Vec<u32> = data::iter_diagnostic_messages().map(|m| m.code).collect();
        assert!(
            codes.len() >= 2000,
            "generated diagnostic table is suspiciously small: {} entries",
            codes.len()
        );
        for window in codes.windows(2) {
            assert!(
                window[0] < window[1],
                "diagnostic codes must be strictly increasing: {} then {}",
                window[0],
                window[1]
            );
        }
    }
// TSZ_INLINE_TEST_END ee5115924c5381202f4a2e63aa65cad03983fb8be801d3f4d67389c76068f2e0

// TSZ_INLINE_TEST_BEGIN 79610b507726e40b16c0cea4581cf3c68a2dac5021fcae7ce55cb264b1ddb6f4 517 generated_views_agree_with_the_lookup_table
    #[test]
    fn generated_views_agree_with_the_lookup_table() {
        // The code constant, template constant, and table entry for a
        // diagnostic all expand from one `define_diagnostics!` declaration.
        // Spot-check that the three views line up for a long-standing entry...
        assert_eq!(diagnostic_codes::UNTERMINATED_STRING_LITERAL, 1002);
        let entry = lookup_diagnostic(diagnostic_codes::UNTERMINATED_STRING_LITERAL)
            .expect("table entry for TS1002");
        assert_eq!(
            entry.message,
            diagnostic_messages::UNTERMINATED_STRING_LITERAL
        );
        assert_eq!(entry.category, DiagnosticCategory::Error);

        // ...and for an entry that the historical split tables had dropped
        // (present in the message table but missing its code/template
        // constants until the views were unified).
        assert_eq!(
            diagnostic_codes::TYPE_IS_REFERENCED_DIRECTLY_OR_INDIRECTLY_IN_THE_FULFILLMENT_CALLBACK_OF_ITS_OWN,
            1062
        );
        let entry = lookup_diagnostic(1062).expect("table entry for TS1062");
        assert_eq!(
            entry.message,
            diagnostic_messages::TYPE_IS_REFERENCED_DIRECTLY_OR_INDIRECTLY_IN_THE_FULFILLMENT_CALLBACK_OF_ITS_OWN
        );

        // Non-error categories survive the expansion as well.
        let suggestion = lookup_diagnostic(95194).expect("table entry for TS95194");
        assert_eq!(suggestion.category, DiagnosticCategory::Message);
        assert_eq!(suggestion.message, diagnostic_messages::WRAP_IN_PARENTHESES);
    }
// TSZ_INLINE_TEST_END 79610b507726e40b16c0cea4581cf3c68a2dac5021fcae7ce55cb264b1ddb6f4

// TSZ_INLINE_TEST_BEGIN 123e16573e49d76373436175546884442474f01dd5998bd4cd3219651d2772e3 550 lookup_diagnostic_finds_known_code_and_rejects_unknown_code
    #[test]
    fn lookup_diagnostic_finds_known_code_and_rejects_unknown_code() {
        let known = data::iter_diagnostic_messages()
            .next()
            .expect("generated diagnostic table should not be empty");

        let lookup = lookup_diagnostic(known.code).expect("known code should resolve");
        assert_eq!(lookup, known);
        assert!(lookup_diagnostic(u32::MAX).is_none());
    }
// TSZ_INLINE_TEST_END 123e16573e49d76373436175546884442474f01dd5998bd4cd3219651d2772e3

// TSZ_INLINE_TEST_BEGIN e622a7c78a4726125092c6c36b3bb1e2153572f0d369dd4817f94fd8ec83c6e3 561 get_message_template_matches_lookup_and_returns_none_for_unknown_code
    #[test]
    fn get_message_template_matches_lookup_and_returns_none_for_unknown_code() {
        let known = data::iter_diagnostic_messages()
            .next()
            .expect("generated diagnostic table should not be empty");

        assert_eq!(get_message_template(known.code), Some(known.message));
        assert_eq!(get_message_template(u32::MAX), None);
    }
// TSZ_INLINE_TEST_END e622a7c78a4726125092c6c36b3bb1e2153572f0d369dd4817f94fd8ec83c6e3

// TSZ_INLINE_TEST_BEGIN f06a54379a7ff5f0d34af4ce0b8db3a17d725359e4d037572f4a63b9c4d8b085 571 format_message_replaces_placeholders_and_leaves_missing_ones_intact
    #[test]
    fn format_message_replaces_placeholders_and_leaves_missing_ones_intact() {
        let formatted = format_message("{0} + {1} + {0} + {2}", &["a", "b"]);
        assert_eq!(formatted, "a + b + a + {2}");
    }
// TSZ_INLINE_TEST_END f06a54379a7ff5f0d34af4ce0b8db3a17d725359e4d037572f4a63b9c4d8b085

// TSZ_INLINE_TEST_BEGIN aed66d5646fc0cc8403d390f8b1b7026be3fa2fd19ab2e4dec30cba4dafae704 577 diagnostic_from_code_uses_table_entry_for_known_code
    #[test]
    fn diagnostic_from_code_uses_table_entry_for_known_code() {
        let known = data::iter_diagnostic_messages()
            .next()
            .expect("generated diagnostic table should not be empty");
        let args = ["left", "right", "extra"];
        let expected_message = format_message(known.message, &args);

        let diagnostic = Diagnostic::from_code(known.code, "test.ts", 4, 8, &args);

        assert_eq!(diagnostic.category, known.category);
        assert_eq!(diagnostic.code, known.code);
        assert_eq!(diagnostic.file, "test.ts");
        assert_eq!(diagnostic.start, 4);
        assert_eq!(diagnostic.length, 8);
        assert_eq!(diagnostic.message_text, expected_message);
        assert!(diagnostic.related_information.is_empty());
    }
// TSZ_INLINE_TEST_END aed66d5646fc0cc8403d390f8b1b7026be3fa2fd19ab2e4dec30cba4dafae704

// TSZ_INLINE_TEST_BEGIN cdfd21860a1277f3fb49a96fbf12236300fbc40ae8929a08a48d6a6b44664fb4 596 diagnostic_from_code_uses_unknown_fallback_for_missing_code
    #[test]
    fn diagnostic_from_code_uses_unknown_fallback_for_missing_code() {
        let result = std::panic::catch_unwind(|| {
            Diagnostic::from_code(u32::MAX, "missing.ts", 1, 2, &["ignored"])
        });

        if cfg!(debug_assertions) {
            assert!(
                result.is_err(),
                "debug builds should trip the diagnostic lookup assertion"
            );
        } else {
            let diagnostic = result.expect("release builds should return the fallback diagnostic");
            assert_eq!(diagnostic.category, DiagnosticCategory::Error);
            assert_eq!(diagnostic.code, u32::MAX);
            assert_eq!(diagnostic.file, "missing.ts");
            assert_eq!(diagnostic.start, 1);
            assert_eq!(diagnostic.length, 2);
            assert_eq!(diagnostic.message_text, "Unknown diagnostic");
            assert!(diagnostic.related_information.is_empty());
        }
    }
// TSZ_INLINE_TEST_END cdfd21860a1277f3fb49a96fbf12236300fbc40ae8929a08a48d6a6b44664fb4

// TSZ_INLINE_TEST_BEGIN 9791bd1d3a75e4868006fc5e7169aba8a210b43f52853fbcea9fb4b55d651915 657 compare_orders_by_file_then_start_then_length_then_code_then_message
    #[test]
    fn compare_orders_by_file_then_start_then_length_then_code_then_message() {
        // Canonical tsc order: file, then start, then length, then code, then
        // message text. Several pairs deliberately tie on the earlier keys so
        // the later tiebreakers are exercised.
        let canonical = vec![
            diag("a.ts", 0, 5, 2304, "alpha"),
            // same file+start as next, shorter length sorts first
            diag("a.ts", 10, 2, 9999, "zzz"),
            diag("a.ts", 10, 4, 1000, "aaa"),
            // same file+start+length, lower code first
            diag("a.ts", 20, 3, 2322, "msg"),
            diag("a.ts", 20, 3, 2345, "msg"),
            // same file+start+length+code, message breaks the tie
            diag("a.ts", 30, 1, 2304, "aaa"),
            diag("a.ts", 30, 1, 2304, "bbb"),
            // file name is the highest-priority key
            diag("b.ts", 0, 1, 1000, "anything"),
        ];

        assert_canonical_order_is_permutation_invariant(&canonical);
    }
// TSZ_INLINE_TEST_END 9791bd1d3a75e4868006fc5e7169aba8a210b43f52853fbcea9fb4b55d651915

// TSZ_INLINE_TEST_BEGIN 0dd204233773269c237a14c348df6804dfd330989a3e442678ccc44258700bc1 680 compare_breaks_ties_on_related_information
    #[test]
    fn compare_breaks_ties_on_related_information() {
        // Two diagnostics identical on every primary field differ only in
        // related information; the shorter related list sorts first, then the
        // lists compare element-by-element.
        let bare = diag("a.ts", 0, 1, 2304, "msg");
        let with_one = diag("a.ts", 0, 1, 2304, "msg").with_related("a.ts", 5, 1, "see a");
        let with_two = diag("a.ts", 0, 1, 2304, "msg")
            .with_related("a.ts", 5, 1, "see a")
            .with_related("a.ts", 9, 1, "see b");

        let canonical = vec![bare, with_one, with_two];
        assert_canonical_order_is_permutation_invariant(&canonical);
    }
// TSZ_INLINE_TEST_END 0dd204233773269c237a14c348df6804dfd330989a3e442678ccc44258700bc1

// TSZ_INLINE_TEST_BEGIN 9d42fd7e6357487392e0bfabedf365fddca2d8b29e10deecac86d5cea1de8723 695 compare_is_a_total_order_consistent_with_equality
    #[test]
    fn compare_is_a_total_order_consistent_with_equality() {
        let a = diag("a.ts", 0, 1, 2304, "msg");
        let b = a.clone();
        assert_eq!(a.compare(&b), std::cmp::Ordering::Equal);

        let c = diag("a.ts", 0, 1, 2304, "msg2");
        // Antisymmetry: a < c implies c > a.
        assert_eq!(a.compare(&c), std::cmp::Ordering::Less);
        assert_eq!(c.compare(&a), std::cmp::Ordering::Greater);
    }
// TSZ_INLINE_TEST_END 9d42fd7e6357487392e0bfabedf365fddca2d8b29e10deecac86d5cea1de8723

// TSZ_INLINE_TEST_BEGIN 10142394989a05313c2f97b8e83f6a8c5fca624f4291a6d2577fce00354571a0 707 diagnostic_with_related_appends_message_information
    #[test]
    fn diagnostic_with_related_appends_message_information() {
        let diagnostic = Diagnostic::error("file.ts", 10, 3, "message", 1234)
            .with_related("other.ts", 20, 5, "see also");

        assert_eq!(diagnostic.related_information.len(), 1);
        let related = &diagnostic.related_information[0];
        assert_eq!(related.category, DiagnosticCategory::Message);
        assert_eq!(related.code, 0);
        assert_eq!(related.file, "other.ts");
        assert_eq!(related.start, 20);
        assert_eq!(related.length, 5);
        assert_eq!(related.message_text, "see also");
    }
// TSZ_INLINE_TEST_END 10142394989a05313c2f97b8e83f6a8c5fca624f4291a6d2577fce00354571a0

// TSZ_INLINE_TEST_BEGIN 028cf3c3308f7167d54d255d495a72e607522b2d14c3c3a6ae0549bef4bf5065 730 is_parser_grammar_diagnostic_covers_inclusive_lower_bound
    #[test]
    fn is_parser_grammar_diagnostic_covers_inclusive_lower_bound() {
        assert!(is_parser_grammar_diagnostic(1000));
        assert!(is_parser_grammar_diagnostic(1001));
    }
// TSZ_INLINE_TEST_END 028cf3c3308f7167d54d255d495a72e607522b2d14c3c3a6ae0549bef4bf5065

// TSZ_INLINE_TEST_BEGIN 2c8df632b8aebd0494d9bcee67dd06d99a35ce1bda52100e88f6ca81133f4474 736 is_parser_grammar_diagnostic_covers_typical_codes
    #[test]
    fn is_parser_grammar_diagnostic_covers_typical_codes() {
        // TS1005 ("X expected"), TS1109 ("Expression expected"), TS1128, TS1434.
        assert!(is_parser_grammar_diagnostic(1005));
        assert!(is_parser_grammar_diagnostic(1109));
        assert!(is_parser_grammar_diagnostic(1128));
        assert!(is_parser_grammar_diagnostic(1434));
        assert!(is_parser_grammar_diagnostic(1999));
    }
// TSZ_INLINE_TEST_END 2c8df632b8aebd0494d9bcee67dd06d99a35ce1bda52100e88f6ca81133f4474

// TSZ_INLINE_TEST_BEGIN 1ed7b791e6243210cf547477b227ba0ce28c94a08f0402cc496a3a87598e750a 746 is_parser_grammar_diagnostic_excludes_exclusive_upper_bound
    #[test]
    fn is_parser_grammar_diagnostic_excludes_exclusive_upper_bound() {
        assert!(!is_parser_grammar_diagnostic(2000));
        assert!(!is_parser_grammar_diagnostic(2001));
    }
// TSZ_INLINE_TEST_END 1ed7b791e6243210cf547477b227ba0ce28c94a08f0402cc496a3a87598e750a

// TSZ_INLINE_TEST_BEGIN 0ca11b4df729ea8f2ac29f502613c35744f6d9d3f07ad9e98898e2967bb337f5 752 is_parser_grammar_diagnostic_excludes_codes_below_range
    #[test]
    fn is_parser_grammar_diagnostic_excludes_codes_below_range() {
        assert!(!is_parser_grammar_diagnostic(0));
        assert!(!is_parser_grammar_diagnostic(999));
    }
// TSZ_INLINE_TEST_END 0ca11b4df729ea8f2ac29f502613c35744f6d9d3f07ad9e98898e2967bb337f5

// TSZ_INLINE_TEST_BEGIN c20ee808ed71587bf0fb41451126eecdc5b0c2c899d3ebd51f8ab9d32e352a8d 758 is_parser_grammar_diagnostic_excludes_semantic_and_js_grammar_codes
    #[test]
    fn is_parser_grammar_diagnostic_excludes_semantic_and_js_grammar_codes() {
        // Semantic (TS2xxx-TS7xxx) and JS-grammar (TS8xxx) codes are out of range.
        assert!(!is_parser_grammar_diagnostic(2322)); // assignability
        assert!(!is_parser_grammar_diagnostic(2345)); // call argument mismatch
        assert!(!is_parser_grammar_diagnostic(7053)); // implicit any index
        assert!(!is_parser_grammar_diagnostic(8000));
        assert!(!is_parser_grammar_diagnostic(9000));
        assert!(!is_parser_grammar_diagnostic(u32::MAX));
    }
// TSZ_INLINE_TEST_END c20ee808ed71587bf0fb41451126eecdc5b0c2c899d3ebd51f8ab9d32e352a8d

// TSZ_INLINE_TEST_BEGIN 5ebea593333b7136d457d61f8ecbe1e7709f8f6811e3a587fe799be02ec6efdc 769 is_js_grammar_diagnostic_covers_inclusive_lower_bound
    #[test]
    fn is_js_grammar_diagnostic_covers_inclusive_lower_bound() {
        assert!(is_js_grammar_diagnostic(8000));
        assert!(is_js_grammar_diagnostic(8001));
    }
// TSZ_INLINE_TEST_END 5ebea593333b7136d457d61f8ecbe1e7709f8f6811e3a587fe799be02ec6efdc

// TSZ_INLINE_TEST_BEGIN 7de2cf91af9af20d13697c835557562ad22efaf2bdc9f251df4b63e7dd062e95 775 is_js_grammar_diagnostic_covers_typical_codes
    #[test]
    fn is_js_grammar_diagnostic_covers_typical_codes() {
        // TS8002, TS8005, TS8006 are emitted for JS-only syntactic constructs.
        assert!(is_js_grammar_diagnostic(8002));
        assert!(is_js_grammar_diagnostic(8005));
        assert!(is_js_grammar_diagnostic(8500));
        assert!(is_js_grammar_diagnostic(8999));
    }
// TSZ_INLINE_TEST_END 7de2cf91af9af20d13697c835557562ad22efaf2bdc9f251df4b63e7dd062e95

// TSZ_INLINE_TEST_BEGIN d3581bc9eb05c0475a63cf3aa912c0ce04ec8db3975257bf772cf8f68a944b09 784 is_js_grammar_diagnostic_excludes_exclusive_upper_bound
    #[test]
    fn is_js_grammar_diagnostic_excludes_exclusive_upper_bound() {
        assert!(!is_js_grammar_diagnostic(9000));
        assert!(!is_js_grammar_diagnostic(9001));
    }
// TSZ_INLINE_TEST_END d3581bc9eb05c0475a63cf3aa912c0ce04ec8db3975257bf772cf8f68a944b09

// TSZ_INLINE_TEST_BEGIN 30cbece45dcafeae3824961a9630061e60134feedb951c54f3784c306ab82731 790 is_js_grammar_diagnostic_excludes_codes_below_range
    #[test]
    fn is_js_grammar_diagnostic_excludes_codes_below_range() {
        assert!(!is_js_grammar_diagnostic(0));
        assert!(!is_js_grammar_diagnostic(7999));
    }
// TSZ_INLINE_TEST_END 30cbece45dcafeae3824961a9630061e60134feedb951c54f3784c306ab82731

// TSZ_INLINE_TEST_BEGIN c72ae33b8840064d19268a4ef6f30242ea160fe88a889b89ad531f88aa2668a1 796 is_js_grammar_diagnostic_excludes_parser_and_semantic_codes
    #[test]
    fn is_js_grammar_diagnostic_excludes_parser_and_semantic_codes() {
        // The two helpers MUST be disjoint — a parser-grammar code is never a
        // JS-grammar code (and vice versa). Lock that contract.
        assert!(!is_js_grammar_diagnostic(1005));
        assert!(!is_js_grammar_diagnostic(1999));
        assert!(!is_js_grammar_diagnostic(2322));
        assert!(!is_js_grammar_diagnostic(u32::MAX));
        assert!(!is_parser_grammar_diagnostic(8005));
    }
// TSZ_INLINE_TEST_END c72ae33b8840064d19268a4ef6f30242ea160fe88a889b89ad531f88aa2668a1

// TSZ_INLINE_TEST_BEGIN 1197d24c89a77d660e673c5d8b711b4ddd44b3b041fdd81db59e050c8ed988da 813 diagnostic_error_constructor_sets_fields_and_empty_related
    #[test]
    fn diagnostic_error_constructor_sets_fields_and_empty_related() {
        let diagnostic = Diagnostic::error("file.ts", 7, 4, "boom", 9001);
        assert_eq!(diagnostic.category, DiagnosticCategory::Error);
        assert_eq!(diagnostic.code, 9001);
        assert_eq!(diagnostic.file, "file.ts");
        assert_eq!(diagnostic.start, 7);
        assert_eq!(diagnostic.length, 4);
        assert_eq!(diagnostic.message_text, "boom");
        assert!(diagnostic.related_information.is_empty());
    }
// TSZ_INLINE_TEST_END 1197d24c89a77d660e673c5d8b711b4ddd44b3b041fdd81db59e050c8ed988da

// TSZ_INLINE_TEST_BEGIN 54102a8e69b70466a5bbff47dd602af7de97917c4e61c1f0e16c85ae40e637f8 825 diagnostic_error_constructor_accepts_string_and_str_via_into
    #[test]
    fn diagnostic_error_constructor_accepts_string_and_str_via_into() {
        // The `impl Into<String>` arms accept both `&str` and `String` callers
        // — verify both work without surprises.
        let from_str = Diagnostic::error("file.ts", 0, 1, "literal", 1);
        assert_eq!(from_str.message_text, "literal");

        let from_string = Diagnostic::error(
            String::from("owned.ts"),
            0,
            1,
            String::from("owned message"),
            2,
        );
        assert_eq!(from_string.file, "owned.ts");
        assert_eq!(from_string.message_text, "owned message");
    }
// TSZ_INLINE_TEST_END 54102a8e69b70466a5bbff47dd602af7de97917c4e61c1f0e16c85ae40e637f8

// TSZ_INLINE_TEST_BEGIN 5b5d8f913312d7912bde69748e1342ce81eeeaecadf63f2566fa4be3b7855e74 853 format_message_passes_through_arg_without_template_placeholder
    #[test]
    fn format_message_passes_through_arg_without_template_placeholder() {
        // No `${` -> arg is substituted byte-for-byte (including its own braces).
        let formatted = format_message("got {0}", &["plain {value}"]);
        assert_eq!(formatted, "got plain {value}");
    }
// TSZ_INLINE_TEST_END 5b5d8f913312d7912bde69748e1342ce81eeeaecadf63f2566fa4be3b7855e74

// TSZ_INLINE_TEST_BEGIN 6c12b7052e21c8d5be32940ba8313d4e85a2c8809547c19a71692a67f459b42d 860 format_message_strips_whitespace_inside_template_placeholder
    #[test]
    fn format_message_strips_whitespace_inside_template_placeholder() {
        let formatted = format_message("got {0}", &["${  number  }"]);
        assert_eq!(formatted, "got ${number}");
    }
// TSZ_INLINE_TEST_END 6c12b7052e21c8d5be32940ba8313d4e85a2c8809547c19a71692a67f459b42d

// TSZ_INLINE_TEST_BEGIN 5489175220f263413f88ced44adea286ff30b15e9e65c8d447181ecaa41b447f 866 format_message_strips_only_outer_whitespace_in_template_placeholder
    #[test]
    fn format_message_strips_only_outer_whitespace_in_template_placeholder() {
        // Internal whitespace between tokens is preserved; only leading after
        // `${` and trailing before `}` are stripped.
        let formatted = format_message("got {0}", &["${  string | number  }"]);
        assert_eq!(formatted, "got ${string | number}");
    }
// TSZ_INLINE_TEST_END 5489175220f263413f88ced44adea286ff30b15e9e65c8d447181ecaa41b447f

// TSZ_INLINE_TEST_BEGIN 43b4015b2b389a5560993b2217680c8ae67fb52acab7c32a481c6701a90cae6b 874 format_message_preserves_nested_braces_in_template_placeholder
    #[test]
    fn format_message_preserves_nested_braces_in_template_placeholder() {
        // `${ {a: number} }` should yield `${{a: number}}` — the inner `{...}`
        // is balanced by depth counting and not mistaken for the placeholder
        // close.
        let formatted = format_message("got {0}", &["${ {a: number} }"]);
        assert_eq!(formatted, "got ${{a: number}}");
    }
// TSZ_INLINE_TEST_END 43b4015b2b389a5560993b2217680c8ae67fb52acab7c32a481c6701a90cae6b

// TSZ_INLINE_TEST_BEGIN 87f4379c22be1590b7e3578e5f16dc330ccd8197a8053cd0d5755e9f68719167 883 format_message_handles_multiple_template_placeholders_in_one_arg
    #[test]
    fn format_message_handles_multiple_template_placeholders_in_one_arg() {
        let formatted = format_message("x: {0}", &["before ${ first } middle ${  second  } after"]);
        assert_eq!(formatted, "x: before ${first} middle ${second} after");
    }
// TSZ_INLINE_TEST_END 87f4379c22be1590b7e3578e5f16dc330ccd8197a8053cd0d5755e9f68719167

// TSZ_INLINE_TEST_BEGIN 2ab56a133ba569967945014c0c98407a18351d6fc7934cfe4e075e9162733f38 889 format_message_normalizes_each_arg_independently
    #[test]
    fn format_message_normalizes_each_arg_independently() {
        let formatted = format_message("{0} -> {1}", &["${  source  }", "plain {x}"]);
        assert_eq!(formatted, "${source} -> plain {x}");
    }
// TSZ_INLINE_TEST_END 2ab56a133ba569967945014c0c98407a18351d6fc7934cfe4e075e9162733f38

// TSZ_INLINE_TEST_BEGIN 4ea4396a551325f9b06674ae61169d63a3c593b4b3471cf10819b698580eabc5 895 format_message_handles_unterminated_template_placeholder_gracefully
    #[test]
    fn format_message_handles_unterminated_template_placeholder_gracefully() {
        // No closing `}` — function consumes to end without panicking and
        // emits the (trimmed) inner content followed by a synthesized `}`.
        let formatted = format_message("got {0}", &["prefix ${ unterminated"]);
        assert_eq!(formatted, "got prefix ${unterminated}");
    }
// TSZ_INLINE_TEST_END 4ea4396a551325f9b06674ae61169d63a3c593b4b3471cf10819b698580eabc5

// TSZ_INLINE_TEST_BEGIN 759ef3ebc650787255a7e8520ab33b6c29baa85f04997e4f77bc127ddd0b91f9 903 format_message_handles_empty_template_placeholder
    #[test]
    fn format_message_handles_empty_template_placeholder() {
        let formatted = format_message("got {0}", &["${}"]);
        assert_eq!(formatted, "got ${}");
    }
// TSZ_INLINE_TEST_END 759ef3ebc650787255a7e8520ab33b6c29baa85f04997e4f77bc127ddd0b91f9

// TSZ_INLINE_TEST_BEGIN 169ffd71392db3a43ae6748d695df5f992e0600745fd440d624747350c9b0f53 909 format_message_dollar_without_brace_is_literal
    #[test]
    fn format_message_dollar_without_brace_is_literal() {
        // A bare `$` not followed by `{` is passed through as-is.
        let formatted = format_message("got {0}", &["price: $5"]);
        assert_eq!(formatted, "got price: $5");
    }
// TSZ_INLINE_TEST_END 169ffd71392db3a43ae6748d695df5f992e0600745fd440d624747350c9b0f53

// TSZ_INLINE_TEST_BEGIN 70383f3bb471669040a43a5ff58d606e08ea950baf9f05f568765d6c0c92de9d 916 error_with_span_matches_error_start_length
    #[test]
    fn error_with_span_matches_error_start_length() {
        // `error_with_span(file, Span::new(start, end), msg, code)` must
        // produce the same diagnostic as `error(file, start, end-start, msg, code)`.
        // The span uses half-open `[start, end)` semantics.
        let span = crate::span::Span::new(10, 17);
        let lhs = Diagnostic::error_with_span("a.ts", span, "hello", 2322);
        let rhs = Diagnostic::error("a.ts", 10, 7, "hello", 2322);
        assert_eq!(lhs, rhs);
    }
// TSZ_INLINE_TEST_END 70383f3bb471669040a43a5ff58d606e08ea950baf9f05f568765d6c0c92de9d

// TSZ_INLINE_TEST_BEGIN cb32c7ed272c530ef3ffa85f634233b050c5d28ba6773bf0c7d9bd1048e20dff 927 span_accessor_round_trips_with_error_with_span
    #[test]
    fn span_accessor_round_trips_with_error_with_span() {
        // `Diagnostic::span()` reconstructs the half-open `Span` that
        // `error_with_span` stored — round-trip identity.
        let span = crate::span::Span::new(100, 105);
        let diag = Diagnostic::error_with_span("a.ts", span, "x", 2322);
        assert_eq!(diag.span(), span);
        assert_eq!(diag.span().len(), 5);
    }
// TSZ_INLINE_TEST_END cb32c7ed272c530ef3ffa85f634233b050c5d28ba6773bf0c7d9bd1048e20dff

// TSZ_INLINE_TEST_BEGIN 920775f9aaeff09a86b4cd75a33c2d7d0842609f22bb610bf6abda368d0c45e3 937 from_code_with_span_matches_from_code_start_length
    #[test]
    fn from_code_with_span_matches_from_code_start_length() {
        // Same equivalence, for `from_code` / `from_code_with_span`.
        let span = crate::span::Span::new(0, 4);
        // Use a known-existing code so format-message lookup behaves the
        // same on both sides.
        let code = 2322;
        let lhs = Diagnostic::from_code_with_span(code, "a.ts", span, &["string", "number"]);
        let rhs = Diagnostic::from_code(code, "a.ts", 0, 4, &["string", "number"]);
        assert_eq!(lhs, rhs);
    }
// TSZ_INLINE_TEST_END 920775f9aaeff09a86b4cd75a33c2d7d0842609f22bb610bf6abda368d0c45e3

// TSZ_INLINE_TEST_BEGIN a4b9255c59cfea3bad8fde893dd71f0b23a61242666363a8662a41f03d725fbe 949 with_related_span_matches_with_related_start_length
    #[test]
    fn with_related_span_matches_with_related_start_length() {
        let main_span = crate::span::Span::new(0, 3);
        let related_span = crate::span::Span::new(20, 25);
        let lhs = Diagnostic::error_with_span("a.ts", main_span, "x", 2322).with_related_span(
            "b.ts",
            related_span,
            "see here",
        );
        let rhs =
            Diagnostic::error("a.ts", 0, 3, "x", 2322).with_related("b.ts", 20, 5, "see here");
        assert_eq!(lhs, rhs);
    }
// TSZ_INLINE_TEST_END a4b9255c59cfea3bad8fde893dd71f0b23a61242666363a8662a41f03d725fbe

// TSZ_INLINE_TEST_BEGIN 00fcbecc44fd57da2738dda0d34ce25fd2459c7679622778e9b54e6a98453ef0 963 push_elaboration_reuses_own_span_and_tags_message
    #[test]
    fn push_elaboration_reuses_own_span_and_tags_message() {
        let mut diag = Diagnostic::error("a.ts", 7, 4, "headline", 2322);
        diag.push_elaboration("detail", 2728, 1);
        assert_eq!(diag.related_information.len(), 1);
        let related = &diag.related_information[0];
        assert_eq!(related.file, "a.ts");
        assert_eq!(related.start, 7);
        assert_eq!(related.length, 4);
        assert_eq!(related.message_text, "detail");
        assert_eq!(related.code, 2728);
        assert_eq!(related.depth, 1);
        assert_eq!(related.category, DiagnosticCategory::Message);
    }
// TSZ_INLINE_TEST_END 00fcbecc44fd57da2738dda0d34ce25fd2459c7679622778e9b54e6a98453ef0

// TSZ_INLINE_TEST_BEGIN f9a48736664ef9350623d49ee905d4194bc17f8ca49e803f195816029c9e53fc 978 push_elaboration_in_span_keeps_self_file_but_explicit_span
    #[test]
    fn push_elaboration_in_span_keeps_self_file_but_explicit_span() {
        let mut diag = Diagnostic::error("a.ts", 7, 4, "headline", 2322);
        diag.push_elaboration_in_span(40, 9, "member", 2322, 0);
        let related = &diag.related_information[0];
        assert_eq!(related.file, "a.ts");
        assert_eq!((related.start, related.length), (40, 9));
        assert_eq!(related.depth, 0);
    }
// TSZ_INLINE_TEST_END f9a48736664ef9350623d49ee905d4194bc17f8ca49e803f195816029c9e53fc

// TSZ_INLINE_TEST_BEGIN b2d962a79b2f7ea52e0c37f9b2a894d11a2add3e2c3cc95e5df484b4e18c427d 988 push_elaboration_at_clamps_depth_into_u8_range
    #[test]
    fn push_elaboration_at_clamps_depth_into_u8_range() {
        let mut diag = Diagnostic::error("a.ts", 0, 1, "headline", 2322);
        // A depth past `u8::MAX` must clamp rather than panic on the cast.
        diag.push_elaboration_at("b.ts", 2, 3, "deep", 2322, u32::MAX);
        let related = &diag.related_information[0];
        assert_eq!(related.file, "b.ts");
        assert_eq!((related.start, related.length), (2, 3));
        assert_eq!(related.depth, u8::MAX);
        assert_eq!(related.category, DiagnosticCategory::Message);
    }
// TSZ_INLINE_TEST_END b2d962a79b2f7ea52e0c37f9b2a894d11a2add3e2c3cc95e5df484b4e18c427d
