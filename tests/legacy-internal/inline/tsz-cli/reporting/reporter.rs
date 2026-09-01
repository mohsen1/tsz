//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-cli/src/reporting/reporter.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 62389e915d0b22d3ca5312b660ae09194ee274e50db74055ed0082c945a2821b 755 plain_output_suppresses_cross_location_pointers
    /// #16316: `tsc`'s non-pretty formatter renders only the flattened
    /// `messageText`; `relatedInformation` is rendered exclusively by the
    /// pretty formatter. Oracled on `typescript@7.0.2`:
    ///
    /// ```text
    /// $ tsc --noEmit --strict --pretty false rel.ts
    /// rel.ts(2,7): error TS2741: Property 'y' is missing in type '{ x: number; }' but required in type 'Point'.
    /// ```
    ///
    /// — one line, no `'y' is declared here.` beneath it, even though the
    /// pretty run prints that pointer with its own location and snippet.
    #[test]
    fn plain_output_suppresses_cross_location_pointers() {
        let source = "interface Point { x: number; y: number; }\nconst p: Point = { x: 1 };\n";
        let mut reporter = reporter_with_source("rel.ts", source);
        reporter.set_pretty(false);

        let mut diagnostic = Diagnostic::error(
            "rel.ts",
            48,
            1,
            "Property 'y' is missing in type '{ x: number; }' but required in type 'Point'.",
            2741,
        );
        diagnostic
            .related_information
            .push(pointer_at("rel.ts", 29, 1, "'y' is declared here."));

        let mut out = String::new();
        reporter.format_diagnostic_plain(&mut out, &diagnostic);

        assert_eq!(
            out,
            "rel.ts(2,7): error TS2741: Property 'y' is missing in type '{ x: number; }' but \
             required in type 'Point'."
        );
    }
// TSZ_INLINE_TEST_END 62389e915d0b22d3ca5312b660ae09194ee274e50db74055ed0082c945a2821b

// TSZ_INLINE_TEST_BEGIN 639822fb1de2cbe8d63f0aa4d2dac497ddc0500f5c5d76e9356ef1c29fdfef08 786 plain_output_keeps_elaboration_chain_links
    /// The suppression is keyed on the entry's kind, not on whether it names a
    /// file: an elaboration chain link carries a real file and the primary's
    /// own anchor, and `tsc` flattens it into `messageText`, so plain output
    /// must keep printing it.
    #[test]
    fn plain_output_keeps_elaboration_chain_links() {
        let source = "declare const a: string;\nconst b: number = a;\n";
        let mut reporter = reporter_with_source("chain.ts", source);
        reporter.set_pretty(false);

        let mut diagnostic = Diagnostic::error(
            "chain.ts",
            31,
            1,
            "Type 'string' is not assignable to type 'number'.",
            2322,
        );
        diagnostic
            .related_information
            .push(related_at("chain.ts", 31, 1, "First elaboration."));
        diagnostic.related_information.push({
            let mut deeper = related_at("chain.ts", 31, 1, "Nested elaboration.");
            deeper.depth = 1;
            deeper
        });

        let mut out = String::new();
        reporter.format_diagnostic_plain(&mut out, &diagnostic);

        assert_eq!(
            out,
            "chain.ts(2,7): error TS2322: Type 'string' is not assignable to type 'number'.\n  \
             First elaboration.\n    Nested elaboration."
        );
    }
// TSZ_INLINE_TEST_END 639822fb1de2cbe8d63f0aa4d2dac497ddc0500f5c5d76e9356ef1c29fdfef08

// TSZ_INLINE_TEST_BEGIN 67d69b9765937821dbdee192c17af1f1303c6a382d547c43a3fe1caf12034411 820 pretty_output_still_renders_pointers_that_plain_output_drops
    /// A diagnostic carrying both kinds keeps only the chain link in plain
    /// output and renders both in pretty output — the two modes must not agree.
    #[test]
    fn pretty_output_still_renders_pointers_that_plain_output_drops() {
        let source = "interface Point { x: number; y: number; }\nconst p: Point = { x: 1 };\n";
        let mut diagnostic = Diagnostic::error(
            "rel.ts",
            48,
            1,
            "Property 'y' is missing in type '{ x: number; }' but required in type 'Point'.",
            2741,
        );
        diagnostic
            .related_information
            .push(related_at("rel.ts", 48, 1, "Chain link."));
        diagnostic
            .related_information
            .push(pointer_at("rel.ts", 29, 1, "'y' is declared here."));

        let mut plain = reporter_with_source("rel.ts", source);
        plain.set_pretty(false);
        let mut plain_out = String::new();
        plain.format_diagnostic_plain(&mut plain_out, &diagnostic);
        assert!(plain_out.contains("Chain link."), "{plain_out}");
        assert!(!plain_out.contains("declared here"), "{plain_out}");

        let mut pretty = reporter_with_source("rel.ts", source);
        let mut pretty_out = String::new();
        pretty.format_diagnostic_pretty(&mut pretty_out, &diagnostic);
        assert!(pretty_out.contains("Chain link."), "{pretty_out}");
        assert!(
            pretty_out.contains("rel.ts:1:30 - 'y' is declared here."),
            "{pretty_out}"
        );
    }
// TSZ_INLINE_TEST_END 67d69b9765937821dbdee192c17af1f1303c6a382d547c43a3fe1caf12034411

// TSZ_INLINE_TEST_BEGIN b4b78d6852fae79353dc43b01bf185c2b40391f0450732d1e10b559f8064aa92 889 pretty_located_related_puts_message_on_the_location_line
    /// tsc 7.0.2, `--pretty --strict`, on
    /// `function f(a = 1) { "use strict"; }`:
    ///
    /// ```text
    /// us.ts:1:12 - error TS1346: This parameter is not allowed with 'use strict' directive.
    ///
    /// 1 function f(a = 1) { "use strict"; }
    ///              ~~~~~
    ///
    ///   us.ts:1:21 - 'use strict' directive used here.
    ///     1 function f(a = 1) { "use strict"; }
    ///                           ~~~~~~~~~~~~~
    /// ```
    ///
    /// The load-bearing details: a blank line separates the related entry from
    /// the primary's underline, and the related message sits on the location
    /// line after ` - `, with its snippet underneath.
    #[test]
    fn pretty_located_related_puts_message_on_the_location_line() {
        let source = "function f(a = 1) { \"use strict\"; }\n";
        let mut reporter = reporter_with_source("us.ts", source);

        let mut diagnostic = Diagnostic::error(
            "us.ts".to_string(),
            11,
            5,
            "This parameter is not allowed with 'use strict' directive.",
            1346,
        );
        diagnostic.related_information.push(pointer_at(
            "us.ts",
            20,
            13,
            "'use strict' directive used here.",
        ));

        let mut out = String::new();
        reporter.format_diagnostic_pretty(&mut out, &diagnostic);

        assert_eq!(
            out,
            "us.ts:1:12 - error TS1346: This parameter is not allowed with 'use strict' directive.\n\
             \n\
             1 function f(a = 1) { \"use strict\"; }\n\
             \x20            ~~~~~\n\
             \n\
             \x20 us.ts:1:21 - 'use strict' directive used here.\n\
             \x20   1 function f(a = 1) { \"use strict\"; }\n\
             \x20                         ~~~~~~~~~~~~~",
            "rendered:\n{out}"
        );
    }
// TSZ_INLINE_TEST_END b4b78d6852fae79353dc43b01bf185c2b40391f0450732d1e10b559f8064aa92

// TSZ_INLINE_TEST_BEGIN 5ff66b171f8c8c6369ac6c7fda2b32c820f0f661361adb9f7c24c59aa1f22c13 929 pretty_blank_line_precedes_every_located_related_entry
    /// Every located entry gets its own blank line, not just the first — tsc
    /// on `function g(a = 1, [b] = [2]) {{ \"use strict\"; }}` reports TS1347
    /// with two pointers (`Non-simple parameter declared here.` / `and here.`)
    /// separated by a blank line each.
    #[test]
    fn pretty_blank_line_precedes_every_located_related_entry() {
        let source = "function g(a = 1, [b] = [2]) { \"use strict\"; }\n";
        let mut reporter = reporter_with_source("multi.ts", source);

        let mut diagnostic = Diagnostic::error(
            "multi.ts".to_string(),
            31,
            13,
            "'use strict' directive cannot be used with non-simple parameter list.",
            1347,
        );
        diagnostic.related_information.push(pointer_at(
            "multi.ts",
            11,
            5,
            "Non-simple parameter declared here.",
        ));
        diagnostic
            .related_information
            .push(pointer_at("multi.ts", 18, 9, "and here."));

        let mut out = String::new();
        reporter.format_diagnostic_pretty(&mut out, &diagnostic);

        let related_block = out
            .split_once("~~~~~~~~~~~~~\n")
            .expect("primary underline present")
            .1;
        assert_eq!(
            related_block,
            "\n\
             \x20 multi.ts:1:12 - Non-simple parameter declared here.\n\
             \x20   1 function g(a = 1, [b] = [2]) { \"use strict\"; }\n\
             \x20                ~~~~~\n\
             \n\
             \x20 multi.ts:1:19 - and here.\n\
             \x20   1 function g(a = 1, [b] = [2]) { \"use strict\"; }\n\
             \x20                       ~~~~~~~~~",
            "rendered:\n{out}"
        );
    }
// TSZ_INLINE_TEST_END 5ff66b171f8c8c6369ac6c7fda2b32c820f0f661361adb9f7c24c59aa1f22c13

// TSZ_INLINE_TEST_BEGIN 71ea26a685da27800b0012885b2f06f34fb342701b1362e2260e707cb8cf42a8 983 pretty_unlocated_related_renders_as_indented_chain_text
    /// An entry with no file is a message-chain link, not a cross-location
    /// pointer. tsc renders those as plain indented text in *both* modes —
    /// `tsc --pretty` on a JS root without `allowJs` prints
    ///
    /// ```text
    /// error TS6504: File 'root.js' is a JavaScript file. Did you mean to enable the 'allowJs' option?
    ///   The file is in the program because:
    ///     Root file specified for compilation
    /// ```
    ///
    /// with no blank line, no location, and 2 spaces per nesting level.
    #[test]
    fn pretty_unlocated_related_renders_as_indented_chain_text() {
        let mut reporter = Reporter::new(false);
        reporter.set_pretty(true);
        reporter.cwd = None;

        let mut diagnostic = Diagnostic::error(
            String::new(),
            0,
            0,
            "File 'root.js' is a JavaScript file. Did you mean to enable the 'allowJs' option?",
            6504,
        );
        diagnostic
            .related_information
            .push(DiagnosticRelatedInformation {
                depth: 0,
                ..related_at("", 0, 0, "The file is in the program because:")
            });
        diagnostic
            .related_information
            .push(DiagnosticRelatedInformation {
                depth: 1,
                ..related_at("", 0, 0, "Root file specified for compilation")
            });

        let mut out = String::new();
        reporter.format_diagnostic_pretty(&mut out, &diagnostic);

        assert_eq!(
            out,
            "error TS6504: File 'root.js' is a JavaScript file. Did you mean to enable the 'allowJs' option?\n\
             \x20 The file is in the program because:\n\
             \x20   Root file specified for compilation",
            "rendered:\n{out}"
        );
    }
// TSZ_INLINE_TEST_END 71ea26a685da27800b0012885b2f06f34fb342701b1362e2260e707cb8cf42a8

// TSZ_INLINE_TEST_BEGIN 8383cb0a0ea9ff368282262a57e00d6bb00058dc0f3e1053cb6ae3b63674c34b 1042 pretty_chain_link_with_real_file_renders_before_the_primary_snippet
    /// A chain link commonly carries the *same* file/span as its own parent
    /// diagnostic — `push_elaboration` anchors nested property-mismatch
    /// elaboration at the primary's own span, not a distinct location — so
    /// dispatch must key off `kind`, never off whether `file` is empty, or a
    /// same-file chain link gets misrendered as a second, spurious located
    /// block. tsc 7.0.2, `--pretty --strict`, on
    /// `interface A { x: { a: string } } interface B { x: { a: number } }
    /// const b: B = {} as A;`:
    ///
    /// ```text
    /// rel2.ts:3:7 - error TS2322: Type 'A' is not assignable to type 'B'.
    ///   The types of 'x.a' are incompatible between these types.
    ///     Type 'string' is not assignable to type 'number'.
    ///
    /// 3 const b: B = {} as A;
    ///         ~
    /// ```
    /// The chain lines sit directly under the header with no blank line
    /// before them, and the primary's own snippet comes *after* the whole
    /// chain — the reverse of a pointer, whose snippet always follows a
    /// blank line and its own location header.
    #[test]
    fn pretty_chain_link_with_real_file_renders_before_the_primary_snippet() {
        let source = "const b: B = {} as A;\n";
        let mut reporter = reporter_with_source("rel2.ts", source);

        let mut diagnostic = Diagnostic::error(
            "rel2.ts".to_string(),
            6,
            1,
            "Type 'A' is not assignable to type 'B'.",
            2322,
        );
        diagnostic.related_information.push(related_at(
            "rel2.ts",
            6,
            1,
            "The types of 'x.a' are incompatible between these types.",
        ));
        diagnostic
            .related_information
            .push(DiagnosticRelatedInformation {
                depth: 1,
                ..related_at(
                    "rel2.ts",
                    6,
                    1,
                    "Type 'string' is not assignable to type 'number'.",
                )
            });

        let mut out = String::new();
        reporter.format_diagnostic_pretty(&mut out, &diagnostic);

        assert_eq!(
            out,
            "rel2.ts:1:7 - error TS2322: Type 'A' is not assignable to type 'B'.\n\
             \x20 The types of 'x.a' are incompatible between these types.\n\
             \x20   Type 'string' is not assignable to type 'number'.\n\
             \n\
             1 const b: B = {} as A;\n\
             \x20       ~",
            "rendered:\n{out}"
        );
    }
// TSZ_INLINE_TEST_END 8383cb0a0ea9ff368282262a57e00d6bb00058dc0f3e1053cb6ae3b63674c34b

// TSZ_INLINE_TEST_BEGIN 432cd66dc9d055321451dc30f474fbd25b26247ca162cad2c229ed1c54fed199 1087 decode_utf8
    #[test]
    fn decode_utf8() {
        let text = "hello world";
        assert_eq!(
            decode_source_bytes(text.as_bytes()),
            Some("hello world".to_string())
        );
    }
// TSZ_INLINE_TEST_END 432cd66dc9d055321451dc30f474fbd25b26247ca162cad2c229ed1c54fed199

// TSZ_INLINE_TEST_BEGIN 7a7c466244966690c90ddb176e58722f3b1b577f4419efafafd661e5d4e916f2 1096 decode_utf16_le_bom
    #[test]
    fn decode_utf16_le_bom() {
        let text = "AB";
        let mut bytes = vec![0xFF, 0xFE]; // UTF-16 LE BOM
        for ch in text.encode_utf16() {
            bytes.extend_from_slice(&ch.to_le_bytes());
        }
        assert_eq!(decode_source_bytes(&bytes), Some("AB".to_string()));
    }
// TSZ_INLINE_TEST_END 7a7c466244966690c90ddb176e58722f3b1b577f4419efafafd661e5d4e916f2

// TSZ_INLINE_TEST_BEGIN 36de86ada021931a1bcb379206260e81249d5275811aa7bc748e80e05c268c98 1106 decode_utf16_be_bom
    #[test]
    fn decode_utf16_be_bom() {
        let text = "AB";
        let mut bytes = vec![0xFE, 0xFF]; // UTF-16 BE BOM
        for ch in text.encode_utf16() {
            bytes.extend_from_slice(&ch.to_be_bytes());
        }
        assert_eq!(decode_source_bytes(&bytes), Some("AB".to_string()));
    }
// TSZ_INLINE_TEST_END 36de86ada021931a1bcb379206260e81249d5275811aa7bc748e80e05c268c98

// TSZ_INLINE_TEST_BEGIN a9593a19ff58693edc0add46fc9f4a5456b75eb3637e2951576c8ca210ac38ae 1116 decode_invalid_utf8_returns_none
    #[test]
    fn decode_invalid_utf8_returns_none() {
        let bytes = vec![0xFF, 0x00, 0x80]; // Invalid UTF-8 without BOM
        assert_eq!(decode_source_bytes(&bytes), None);
    }
// TSZ_INLINE_TEST_END a9593a19ff58693edc0add46fc9f4a5456b75eb3637e2951576c8ca210ac38ae

// TSZ_INLINE_TEST_BEGIN 9aeabd7b8204696d11b0b4db4e80b7b9640fd2f479e8abeef6293e15fe37aa41 1122 decode_utf16_le_multiline
    #[test]
    fn decode_utf16_le_multiline() {
        let text = "line1\nline2\nline3";
        let mut bytes = vec![0xFF, 0xFE];
        for ch in text.encode_utf16() {
            bytes.extend_from_slice(&ch.to_le_bytes());
        }
        let decoded = decode_source_bytes(&bytes).unwrap();
        assert_eq!(decoded.lines().count(), 3);
        assert_eq!(decoded, text);
    }
// TSZ_INLINE_TEST_END 9aeabd7b8204696d11b0b4db4e80b7b9640fd2f479e8abeef6293e15fe37aa41
