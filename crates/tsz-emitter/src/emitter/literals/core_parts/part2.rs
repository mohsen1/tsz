#[cfg(test)]
mod tests {
    use super::{normalized_bigint_literal_text, radix_digits_to_decimal_string};
    use crate::output::printer::{PrintOptions, Printer};
    use tsz_parser::ParserState;
    fn parse_test_source<S: Into<String>>(
        source: S,
    ) -> (tsz_parser::ParserState, tsz_parser::parser::NodeIndex) {
        let mut parser = tsz_parser::ParserState::new("test.ts".to_string(), source.into());
        let root = parser.parse_source_file();
        (parser, root)
    }

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
}
