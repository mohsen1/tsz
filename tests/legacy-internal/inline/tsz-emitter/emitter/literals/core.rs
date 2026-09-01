//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-emitter/src/emitter/literals/core.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 637faaaa5d147e8d2bb660352c65e75789bd41f295417f212fbbbff624591d9b 1049 regex_literal_preserves_non_ascii_flags
    #[test]
    fn regex_literal_preserves_non_ascii_flags() {
        let source = "const 𝘳𝘦𝘨𝘦𝘹 = /(?𝘴𝘪-𝘮:^𝘧𝘰𝘰.)/𝘨𝘮𝘶;";
        let (parser, root) = parse_test_source(source);
        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;
        assert!(
            output.contains("/(?𝘴𝘪-𝘮:^𝘧𝘰𝘰.)/𝘨𝘮𝘶"),
            "Non-ASCII regex flags should be preserved.\nGot: {output}"
        );
    }
// TSZ_INLINE_TEST_END 637faaaa5d147e8d2bb660352c65e75789bd41f295417f212fbbbff624591d9b

// TSZ_INLINE_TEST_BEGIN 1d129f0b97844b4664f4d455066dd8099d53c957fca4ea220c25134ef577f04b 1065 legacy_octal_converted_to_decimal
    /// Legacy octal literals (01, 076, 009) must be converted to decimal
    /// in emitted JS, matching tsc behavior for ALL targets.
    #[test]
    fn legacy_octal_converted_to_decimal() {
        let cases = [("01;", "1;"), ("076;", "62;"), ("00;", "0;"), ("07;", "7;")];
        for (source, expected_fragment) in cases {
            let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
            let root = parser.parse_source_file();
            let mut printer = Printer::new(&parser.arena, PrintOptions::default());
            printer.set_source_text(source);
            printer.print(root);
            let output = printer.finish().code;
            assert!(
                output.contains(expected_fragment),
                "Legacy octal {source} should emit {expected_fragment}\nGot: {output}"
            );
        }
    }
// TSZ_INLINE_TEST_END 1d129f0b97844b4664f4d455066dd8099d53c957fca4ea220c25134ef577f04b

// TSZ_INLINE_TEST_BEGIN e938f7beef5b9e903ab10f867d45acefe72da65f5f1b679ee41861924bc78087 1084 legacy_octal_with_non_octal_digits
    /// Legacy octal with non-octal digits (08, 09, 089) are parsed as decimal
    /// by JS engines. tsc still strips the leading zero.
    #[test]
    fn legacy_octal_with_non_octal_digits() {
        let cases = [("009;", "9;"), ("08;", "8;")];
        for (source, expected_fragment) in cases {
            let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
            let root = parser.parse_source_file();
            let mut printer = Printer::new(&parser.arena, PrintOptions::default());
            printer.set_source_text(source);
            printer.print(root);
            let output = printer.finish().code;
            assert!(
                output.contains(expected_fragment),
                "Non-octal legacy form {source} should emit {expected_fragment}\nGot: {output}"
            );
        }
    }
// TSZ_INLINE_TEST_END e938f7beef5b9e903ab10f867d45acefe72da65f5f1b679ee41861924bc78087

// TSZ_INLINE_TEST_BEGIN cec3893ed533f9703120cf67a6d939772264c670b2f9c8b8a12948d3c59cd2ab 1102 non_octal_literals_unchanged
    /// Regular decimal, hex, and float literals should NOT be modified.
    #[test]
    fn non_octal_literals_unchanged() {
        let cases = ["42;", "0;", "0.5;", "1e3;"];
        for source in cases {
            let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
            let root = parser.parse_source_file();
            let mut printer = Printer::new(&parser.arena, PrintOptions::default());
            printer.set_source_text(source);
            printer.print(root);
            let output = printer.finish().code;
            assert!(
                output.contains(source.trim_end_matches('\n')),
                "Non-octal {source} should be preserved unchanged.\nGot: {output}"
            );
        }
    }
// TSZ_INLINE_TEST_END cec3893ed533f9703120cf67a6d939772264c670b2f9c8b8a12948d3c59cd2ab

// TSZ_INLINE_TEST_BEGIN fb4a7959650ecd97790390acf696de6f27fba1bb9b04cd91b31f1feccb8ad181 1119 decimal_numeric_separators_with_exponents_downlevel_to_number_text
    #[test]
    fn decimal_numeric_separators_with_exponents_downlevel_to_number_text() {
        let source = [
            "1e1_0;",
            "1e+1_0;",
            "1e-1_0;",
            "1.1e10_0;",
            "1.1e+10_0;",
            "1.1e-10_0;",
            "1_2.3_4e5_6;",
            "1_2.3_4e+5_6;",
            "1_2.3_4e-5_6;",
        ]
        .join("\n");

        let (parser, root) = parse_test_source(source);
        let mut printer = Printer::new(&parser.arena, PrintOptions::es5());
        printer.print(root);
        let output = printer.finish().code;

        for expected in [
            "10000000000;",
            "1e-10;",
            "1.1e+100;",
            "1.1e-100;",
            "1.234e+57;",
            "1.234e-55;",
        ] {
            assert!(
                output.contains(expected),
                "Expected downleveled decimal separator exponent {expected}\nGot: {output}"
            );
        }
        assert!(
            !output.contains("1e10;") && !output.contains("12.34e56;"),
            "Decimal exponent separators should be normalized through the numeric value.\nGot: {output}"
        );
    }
// TSZ_INLINE_TEST_END fb4a7959650ecd97790390acf696de6f27fba1bb9b04cd91b31f1feccb8ad181

// TSZ_INLINE_TEST_BEGIN ea94d93eff35e4d5ef3152129fdef1f78279f85dd421b202bc5123aaa1301d89 1158 unterminated_codepoint_escape_string_downlevels_to_cooked_text
    #[test]
    fn unterminated_codepoint_escape_string_downlevels_to_cooked_text() {
        let source = "var x = \"\\u{00000000000067}\r\n";
        let (parser, root) = parse_test_source(source);
        let mut printer = Printer::new(&parser.arena, PrintOptions::es5());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("var x = \"g\";"),
            "ES5 should downlevel unterminated codepoint escape strings through cooked text.\nGot: {output}"
        );
    }
// TSZ_INLINE_TEST_END ea94d93eff35e4d5ef3152129fdef1f78279f85dd421b202bc5123aaa1301d89

// TSZ_INLINE_TEST_BEGIN 00b20de19efacc43e26609c41ae3ce48b2306f3cdb51a86c932213367ecc6117 1173 incomplete_codepoint_escape_string_keeps_missing_close_quote
    #[test]
    fn incomplete_codepoint_escape_string_keeps_missing_close_quote() {
        let source = "var x = \"\\u{00000000000067";
        let (parser, root) = parse_test_source(source);
        let mut printer = Printer::new(&parser.arena, PrintOptions::es5());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert!(
            output.contains("var x = \"\\u{00000000000067;"),
            "ES5 should preserve tsc's unterminated invalid codepoint escape shape.\nGot: {output}"
        );
        assert!(
            !output.contains("var x = \"\\u{00000000000067\";"),
            "ES5 should not synthesize a closing quote for incomplete codepoint escapes.\nGot: {output}"
        );
    }
// TSZ_INLINE_TEST_END 00b20de19efacc43e26609c41ae3ce48b2306f3cdb51a86c932213367ecc6117

// TSZ_INLINE_TEST_BEGIN 3c4af44ae1532d6c5374d77428bbb52e1d8ee7360e6290dad23587330307e33f 1192 recovered_multiline_string_literals_preserve_source_semicolon_and_eof_space
    #[test]
    fn recovered_multiline_string_literals_preserve_source_semicolon_and_eof_space() {
        use crate::emitter::{Printer as EmitterPrinter, PrinterOptions};

        let source = "var es1 = \"line 1\n\";\nvar es13 = \" \nvar es14 = \"";
        let (parser, root) = parse_test_source(source);
        let mut printer = EmitterPrinter::with_options(&parser.arena, PrinterOptions::default());
        printer.set_source_text(source);
        printer.emit(root);
        let output = printer.get_output().to_string();

        assert!(
            output.contains("var es1 = \"line 1;\n\";;"),
            "Recovered multiline string literals should preserve source semicolons.\nGot: {output}"
        );
        assert!(
            output.contains("var es13 = \" ;"),
            "EOF-terminated string literal recovery should preserve trailing source text.\nGot: {output}"
        );
        assert!(
            output.contains("var es14 = \" ;"),
            "EOF-terminated string literal recovery should synthesize tsc's separator space.\nGot: {output}"
        );
    }
// TSZ_INLINE_TEST_END 3c4af44ae1532d6c5374d77428bbb52e1d8ee7360e6290dad23587330307e33f

// TSZ_INLINE_TEST_BEGIN 67f6cd96f38b22afd99d09fda48e2afaeba01448df5b8ed95c2af77ca3a81f21 1217 unterminated_regex_in_call_does_not_duplicate_recovery_paren
    #[test]
    fn unterminated_regex_in_call_does_not_duplicate_recovery_paren() {
        let source = "foo(/notregexp);";
        let (parser, root) = parse_test_source(source);
        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;
        assert!(
            output.contains("foo(/notregexp);"),
            "Unterminated regex recovery should leave the call paren to the call emitter.\nGot: {output}"
        );
        assert!(
            !output.contains("foo(/notregexp););"),
            "Unterminated regex recovery should not duplicate the call paren.\nGot: {output}"
        );
    }
// TSZ_INLINE_TEST_END 67f6cd96f38b22afd99d09fda48e2afaeba01448df5b8ed95c2af77ca3a81f21

// TSZ_INLINE_TEST_BEGIN a8e288b48dd8c994d0ff5e8e2dabf2441a27d8e929501315cdcc5029db3883f7 1237 unicode_escape_in_identifier_preserved
    /// Unicode escape sequences in identifiers must be preserved in emitted JS,
    /// matching tsc behavior. `var \u0041 = 1;` should NOT resolve to `var A = 1;`.
    #[test]
    fn unicode_escape_in_identifier_preserved() {
        let source = "var \\u0041 = 1;";
        let (parser, root) = parse_test_source(source);
        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;
        assert!(
            output.contains("\\u0041"),
            "Unicode escape \\u0041 should be preserved in identifier.\nGot: {output}"
        );
        assert!(
            !output.starts_with("var A ="),
            "Unicode escape should NOT be resolved to 'A'.\nGot: {output}"
        );
    }
// TSZ_INLINE_TEST_END a8e288b48dd8c994d0ff5e8e2dabf2441a27d8e929501315cdcc5029db3883f7

// TSZ_INLINE_TEST_BEGIN e10ee3e6e67be5237633ff27b28afcd1a12a683723397aab079b16f8d5f366be 1257 numeric_separator_hex_converted_to_decimal_below_es2021
    /// When numeric literals have separators (ES2021 feature) and the target
    /// is < ES2021, tsc converts 0b/0o/0x prefixed literals to decimal.
    #[test]
    fn numeric_separator_hex_converted_to_decimal_below_es2021() {
        use tsz_common::ScriptTarget;
        // 0x00_11 → 17 (after stripping separators: 0x0011 → 17)
        let cases = [
            ("0x00_11;", "17;"),
            ("0X0_1;", "1;"),
            ("0x1100_0011;", "285212689;"),
            ("0xA0_B0_C0;", "10531008;"),
        ];
        for (source, expected) in cases {
            let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
            let root = parser.parse_source_file();
            let opts = PrintOptions {
                target: ScriptTarget::ES2015,
                ..Default::default()
            };
            let mut printer = Printer::new(&parser.arena, opts);
            printer.set_source_text(source);
            printer.print(root);
            let output = printer.finish().code;
            assert!(
                output.contains(expected),
                "Hex with separators {source} at ES2015 should emit {expected}\nGot: {output}"
            );
        }
    }
// TSZ_INLINE_TEST_END e10ee3e6e67be5237633ff27b28afcd1a12a683723397aab079b16f8d5f366be

// TSZ_INLINE_TEST_BEGIN adbd7d2bd3f8e1da1debd22f88daac0ee3d495ebb5079d8a7cb47972c3f4780b 1285 numeric_separator_decimal_exponents_normalized_below_es2021
    #[test]
    fn numeric_separator_decimal_exponents_normalized_below_es2021() {
        use tsz_common::ScriptTarget;
        let source = "1e1_0\n1e+1_0\n1.1e10_0\n1_2.3_4e5_6\n1_2.3_4e-5_6";
        let (parser, root) = parse_test_source(source);
        let opts = PrintOptions {
            target: ScriptTarget::ES2020,
            ..Default::default()
        };
        let mut printer = Printer::new(&parser.arena, opts);
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        for expected in ["10000000000;", "1.1e+100;", "1.234e+57;", "1.234e-55;"] {
            assert!(
                output.contains(expected),
                "Decimal numeric separator exponent should contain {expected}\nGot: {output}"
            );
        }
    }
// TSZ_INLINE_TEST_END adbd7d2bd3f8e1da1debd22f88daac0ee3d495ebb5079d8a7cb47972c3f4780b

// TSZ_INLINE_TEST_BEGIN 3a65be4e9bccd7e4d8341aa4c9cbba20b2d00b6a6ebe2ff6e7328a3a34cbb05f 1307 bigint_separators_are_canonicalized_even_for_esnext
    #[test]
    fn bigint_separators_are_canonicalized_even_for_esnext() {
        let source = "\
const separatedBin = 0b010_10_1n;
const separatedOct = 0o1234_567n;
const separatedDec = 123_456__789n;
const separatedHex = 0x0_ABCDEFn;
";
        let (parser, root) = parse_test_source(source);
        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        for expected in [
            "const separatedBin = 21n;",
            "const separatedOct = 342391n;",
            "const separatedDec = 123456789n;",
            "const separatedHex = 0x0abcdefn;",
        ] {
            assert!(
                output.contains(expected),
                "BigInt literal emit should contain {expected}\nGot: {output}"
            );
        }
    }
// TSZ_INLINE_TEST_END 3a65be4e9bccd7e4d8341aa4c9cbba20b2d00b6a6ebe2ff6e7328a3a34cbb05f

// TSZ_INLINE_TEST_BEGIN f3655159a25cfd79f839e34175488eaea431365a36e4ae7f5714040a19b3121b 1334 malformed_empty_prefixed_bigint_literals_match_tsc_recovery_text
    #[test]
    fn malformed_empty_prefixed_bigint_literals_match_tsc_recovery_text() {
        let source = "const emptyBinary = 0bn;\nconst emptyOct = 0on;\nconst emptyHex = 0xn;\n";
        let (parser, root) = parse_test_source(source);
        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        for expected in [
            "const emptyBinary = 0n;",
            "const emptyOct = 0n;",
            "const emptyHex = 0x0n;",
        ] {
            assert!(
                output.contains(expected),
                "Malformed BigInt recovery emit should contain {expected}\nGot: {output}"
            );
        }
    }
// TSZ_INLINE_TEST_END f3655159a25cfd79f839e34175488eaea431365a36e4ae7f5714040a19b3121b

// TSZ_INLINE_TEST_BEGIN 75704ac916669a1a0a4e6dfb62d9922f168e1397bf15b68cbd9287c6957d957f 1355 bigint_radix_conversion_is_not_limited_to_machine_integers
    #[test]
    fn bigint_radix_conversion_is_not_limited_to_machine_integers() {
        assert_eq!(
            radix_digits_to_decimal_string(
                &normalized_bigint_literal_text(
                    "1_00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
                ),
                2,
            ),
            Some("340282366920938463463374607431768211456".to_string())
        );
    }
// TSZ_INLINE_TEST_END 75704ac916669a1a0a4e6dfb62d9922f168e1397bf15b68cbd9287c6957d957f

// TSZ_INLINE_TEST_BEGIN d5f1fedc68a183174daf9a2e51ae717b6e39fe3aca1c815b2bb8d81be44f1374 1368 numeric_separator_leading_decimal_fraction_gets_zero_prefix_below_es2021
    #[test]
    fn numeric_separator_leading_decimal_fraction_gets_zero_prefix_below_es2021() {
        use tsz_common::ScriptTarget;
        let source = "00.5_5;\n01.5_5;\n";
        let (parser, root) = parse_test_source(source);
        let opts = PrintOptions {
            target: ScriptTarget::ES2020,
            ..Default::default()
        };
        let mut printer = Printer::new(&parser.arena, opts);
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;

        assert_eq!(
            output.matches("0.55;").count(),
            2,
            "Separator downlevel for leading decimal fractions should emit 0.55.\nGot: {output}"
        );
        assert!(
            !output.lines().any(|line| line.trim() == ".55;"),
            "Separator downlevel should not leave bare .55 fractions.\nGot: {output}"
        );
    }
// TSZ_INLINE_TEST_END d5f1fedc68a183174daf9a2e51ae717b6e39fe3aca1c815b2bb8d81be44f1374

// TSZ_INLINE_TEST_BEGIN 97242a83fab7053bba4d09717901926a1e3a4c52943ec4390baad852d4d8791f 1394 numeric_separator_octal_converted_to_decimal_below_es2021
    /// Octal literals with separators converted to decimal at < ES2021
    #[test]
    fn numeric_separator_octal_converted_to_decimal_below_es2021() {
        use tsz_common::ScriptTarget;
        let cases = [
            ("0o00_11;", "9;"),
            ("0O0_1;", "1;"),
            ("0o1100_0011;", "2359305;"),
        ];
        for (source, expected) in cases {
            let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
            let root = parser.parse_source_file();
            let opts = PrintOptions {
                target: ScriptTarget::ES2020,
                ..Default::default()
            };
            let mut printer = Printer::new(&parser.arena, opts);
            printer.set_source_text(source);
            printer.print(root);
            let output = printer.finish().code;
            assert!(
                output.contains(expected),
                "Octal with separators {source} at ES2020 should emit {expected}\nGot: {output}"
            );
        }
    }
// TSZ_INLINE_TEST_END 97242a83fab7053bba4d09717901926a1e3a4c52943ec4390baad852d4d8791f

// TSZ_INLINE_TEST_BEGIN 83ed121527515aa754b253f49bc778b91d92924cb1b32008a2700287ef891006 1421 numeric_separator_binary_converted_to_decimal_below_es2021
    /// Binary literals with separators converted to decimal at < ES2021
    #[test]
    fn numeric_separator_binary_converted_to_decimal_below_es2021() {
        use tsz_common::ScriptTarget;
        let source = "0b1010_0001_1000_0101;";
        let expected = "41349;";
        let (parser, root) = parse_test_source(source);
        let opts = PrintOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        };
        let mut printer = Printer::new(&parser.arena, opts);
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;
        assert!(
            output.contains(expected),
            "Binary with separators {source} at ES2015 should emit {expected}\nGot: {output}"
        );
    }
// TSZ_INLINE_TEST_END 83ed121527515aa754b253f49bc778b91d92924cb1b32008a2700287ef891006

// TSZ_INLINE_TEST_BEGIN 3e1bf22a86a97d1ea2133cf37412df1377820add899d387216fe50aa8a1c5e3d 1443 prefixed_literals_without_separators_unchanged_at_es2015
    /// Hex/octal/binary WITHOUT separators should NOT be converted at ES2015+
    /// (they are only converted at ES5 for 0b/0o syntax support)
    #[test]
    fn prefixed_literals_without_separators_unchanged_at_es2015() {
        use tsz_common::ScriptTarget;
        let cases = ["0x0011;", "0o0011;", "0b1010;"];
        for source in cases {
            let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
            let root = parser.parse_source_file();
            let opts = PrintOptions {
                target: ScriptTarget::ES2015,
                ..Default::default()
            };
            let mut printer = Printer::new(&parser.arena, opts);
            printer.set_source_text(source);
            printer.print(root);
            let output = printer.finish().code;
            assert!(
                output.contains(source.trim_end_matches('\n')),
                "Prefixed literal {source} without separators should be unchanged at ES2015.\nGot: {output}"
            );
        }
    }
// TSZ_INLINE_TEST_END 3e1bf22a86a97d1ea2133cf37412df1377820add899d387216fe50aa8a1c5e3d

// TSZ_INLINE_TEST_BEGIN a02378f011b0c33e22c934ad73b86143c6d7aa59fd028991447da2edc88ff212 1467 unicode_escape_in_property_name_preserved
    /// Unicode escape sequences in property names must be preserved.
    /// `{ \u0061: "ss" }` should NOT resolve to `{ a: "ss" }`.
    #[test]
    fn unicode_escape_in_property_name_preserved() {
        let source = "var x = { \\u0061: \"ss\" };";
        let (parser, root) = parse_test_source(source);
        let mut printer = Printer::new(&parser.arena, PrintOptions::default());
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;
        assert!(
            output.contains("\\u0061"),
            "Unicode escape \\u0061 should be preserved in property name.\nGot: {output}"
        );
    }
// TSZ_INLINE_TEST_END a02378f011b0c33e22c934ad73b86143c6d7aa59fd028991447da2edc88ff212

// TSZ_INLINE_TEST_BEGIN 81b22a07d44202d0971ad7f3e0d3bec80c2c43ea60129c47cfabe99e0eedaa9c 1486 backslash_followed_by_multibyte_utf8_no_panic
    /// Backslash followed by a multi-byte UTF-8 character (e.g. U+2028 LINE
    /// SEPARATOR) in a string literal must not panic during downlevel emit.
    /// Previously, `downlevel_codepoint_escapes_in_literal_text` treated the
    /// byte after `\` as a single ASCII byte and advanced by 2, landing in the
    /// middle of a multi-byte character.
    #[test]
    fn backslash_followed_by_multibyte_utf8_no_panic() {
        use tsz_common::ScriptTarget;
        // U+2028 LINE SEPARATOR is 3 bytes in UTF-8: E2 80 A8
        // The source string: var x = "line 1\<LS> line 2";
        let source = "var x = \"line 1\\\u{2028} line 2\";";
        let (parser, root) = parse_test_source(source);
        let opts = PrintOptions {
            target: ScriptTarget::ES5,
            ..Default::default()
        };
        let mut printer = Printer::new(&parser.arena, opts);
        printer.set_source_text(source);
        printer.print(root);
        let output = printer.finish().code;
        // Should not panic, and should contain the string literal
        assert!(
            output.contains("line 1"),
            "Output should contain the string literal.\nGot: {output}"
        );
    }
// TSZ_INLINE_TEST_END 81b22a07d44202d0971ad7f3e0d3bec80c2c43ea60129c47cfabe99e0eedaa9c

// TSZ_INLINE_TEST_BEGIN 5e40169fc2d64ce8cbd27997bb36724c14c0372c9675363521ae43754a05970e 1511 es5_string_literal_line_continuations_stay_continuations
    /// ES5 string-literal downleveling still preserves source line
    /// continuations as continuations. Normalize the source line ending to LF,
    /// matching `tsc`, instead of escaping the CR/LF bytes into the string.
    #[test]
    fn es5_string_literal_line_continuations_stay_continuations() {
        use tsz_common::ScriptTarget;
        let cases = [
            ("var x = \"a\\\r\nb\";", "\"a\\\nb\""),
            ("var x = 'a\\\nb';", "'a\\\nb'"),
            ("var x = \"a\\\rb\";", "\"a\\\nb\""),
        ];
        for (source, expected) in cases {
            let (parser, root) = parse_test_source(source);
            let opts = PrintOptions {
                target: ScriptTarget::ES5,
                ..Default::default()
            };
            let mut printer = Printer::new(&parser.arena, opts);
            printer.set_source_text(source);
            printer.print(root);
            let output = printer.finish().code;
            assert!(
                output.contains(expected),
                "ES5 line continuation should stay a normalized line continuation.\nSource: {source:?}\nExpected fragment: {expected:?}\nGot: {output:?}"
            );
        }
    }
// TSZ_INLINE_TEST_END 5e40169fc2d64ce8cbd27997bb36724c14c0372c9675363521ae43754a05970e

// TSZ_INLINE_TEST_BEGIN 606b521ff659ff1f6185bd3feea1532654b8e8d2cfc5c367ec1f4e765a16d2aa 1542 null_codepoint_followed_by_digit_uses_x00_escape
    /// When a null codepoint (`\u{0}`) is downleveled to ES5, and the
    /// immediately following character is an ASCII digit, emit `\x00` instead
    /// of `\0` to avoid creating an octal escape sequence in the output.
    ///
    /// Structural rule: when `cp == 0` and the next source byte is 0-9,
    /// use `\x00`; otherwise use `\0`.
    #[test]
    fn null_codepoint_followed_by_digit_uses_x00_escape() {
        use tsz_common::ScriptTarget;
        // All digits 0-9 must trigger \x00 when they immediately follow \u{0}.
        // Two different hex forms for null to prove the rule isn't spelling-dependent.
        let cases = [
            ("var x = \"\\u{0}0\";", "\\x000"),
            ("var x = \"\\u{0}1\";", "\\x001"),
            ("var x = \"\\u{0}5\";", "\\x005"),
            ("var x = \"\\u{0}9\";", "\\x009"),
            ("var x = \"\\u{00}3\";", "\\x003"),
            ("var x = \"\\u{000}7\";", "\\x007"),
        ];
        for (source, expected) in cases {
            let (parser, root) = parse_test_source(source);
            let opts = PrintOptions {
                target: ScriptTarget::ES5,
                ..Default::default()
            };
            let mut printer = Printer::new(&parser.arena, opts);
            printer.set_source_text(source);
            printer.print(root);
            let output = printer.finish().code;
            assert!(
                output.contains(expected),
                "null codepoint before digit should use \\x00 escape.\nSource: {source}\nExpected fragment: {expected}\nGot: {output}"
            );
        }
    }
// TSZ_INLINE_TEST_END 606b521ff659ff1f6185bd3feea1532654b8e8d2cfc5c367ec1f4e765a16d2aa

// TSZ_INLINE_TEST_BEGIN 5632f63c8370aa02f9f4f075a01d8d5052489243480bb0f085eeac4edcca30bb 1574 null_codepoint_not_followed_by_digit_uses_standard_escape
    /// When a null codepoint (`\u{0}`) is NOT followed by an ASCII digit, the
    /// standard `\0` escape is correct and must be used.
    #[test]
    fn null_codepoint_not_followed_by_digit_uses_standard_escape() {
        use tsz_common::ScriptTarget;
        let cases = [
            ("var x = \"\\u{0}a\";", "\\0a"),
            ("var x = \"\\u{0}z\";", "\\0z"),
            ("var x = \"\\u{0}\";", "\\0"),
            ("var x = \"a\\u{0}b\";", "\\0b"),
        ];
        for (source, expected) in cases {
            let (parser, root) = parse_test_source(source);
            let opts = PrintOptions {
                target: ScriptTarget::ES5,
                ..Default::default()
            };
            let mut printer = Printer::new(&parser.arena, opts);
            printer.set_source_text(source);
            printer.print(root);
            let output = printer.finish().code;
            assert!(
                output.contains(expected),
                "null codepoint not before digit should use standard \\0.\nSource: {source}\nExpected fragment: {expected}\nGot: {output}"
            );
        }
    }
// TSZ_INLINE_TEST_END 5632f63c8370aa02f9f4f075a01d8d5052489243480bb0f085eeac4edcca30bb
