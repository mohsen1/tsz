//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-lsp/src/fourslash/parsing.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 90bdd4398c5d8b390fe9911e75f5ec6196f6f20b2b5cbcc361fd26f6c344eb6a 172 test_parse_markers_simple
    #[test]
    fn test_parse_markers_simple() {
        let (cleaned, markers) = parse_markers("test.ts", "const /*def*/x = 42;");
        assert_eq!(cleaned, "const x = 42;");
        assert_eq!(markers.len(), 1);
        assert_eq!(markers[0].name, "def");
        assert_eq!(markers[0].character, 6); // position of 'x'
    }
// TSZ_INLINE_TEST_END 90bdd4398c5d8b390fe9911e75f5ec6196f6f20b2b5cbcc361fd26f6c344eb6a

// TSZ_INLINE_TEST_BEGIN 8aeca026c3d2954c6c07f49e3a5011ec5353d25292732d89f3350b80e77fb7d1 181 test_parse_markers_anonymous
    #[test]
    fn test_parse_markers_anonymous() {
        let (cleaned, markers) = parse_markers("test.ts", "foo(/**/);");
        assert_eq!(cleaned, "foo();");
        assert_eq!(markers.len(), 1);
        assert_eq!(markers[0].name, "");
    }
// TSZ_INLINE_TEST_END 8aeca026c3d2954c6c07f49e3a5011ec5353d25292732d89f3350b80e77fb7d1

// TSZ_INLINE_TEST_BEGIN 1b80a4fa8e00e08e37e26cca18ed7fcfc0a9b0b5e97360c8e0228834ae3eb16b 189 test_parse_markers_multiple
    #[test]
    fn test_parse_markers_multiple() {
        let (cleaned, markers) = parse_markers("test.ts", "/*a*/x + /*b*/y");
        assert_eq!(cleaned, "x + y");
        assert_eq!(markers.len(), 2);
        assert_eq!(markers[0].name, "a");
        assert_eq!(markers[1].name, "b");
    }
// TSZ_INLINE_TEST_END 1b80a4fa8e00e08e37e26cca18ed7fcfc0a9b0b5e97360c8e0228834ae3eb16b

// TSZ_INLINE_TEST_BEGIN fbe472eaf3036386cf16cf2d1d6df2a8b83f76efcdc2bf4d68effff32461cd14 198 test_parse_markers_multiline
    #[test]
    fn test_parse_markers_multiline() {
        let (cleaned, markers) = parse_markers("test.ts", "const /*def*/x = 1;\n/*ref*/x;");
        assert_eq!(cleaned, "const x = 1;\nx;");
        assert_eq!(markers.len(), 2);
        assert_eq!(markers[0].name, "def");
        assert_eq!(markers[0].line, 0);
        assert_eq!(markers[0].character, 6);
        assert_eq!(markers[1].name, "ref");
        assert_eq!(markers[1].line, 1);
        assert_eq!(markers[1].character, 0);
    }
// TSZ_INLINE_TEST_END fbe472eaf3036386cf16cf2d1d6df2a8b83f76efcdc2bf4d68effff32461cd14

// TSZ_INLINE_TEST_BEGIN f37b9d48b26df61b82ba61ca554ae5975895337b2de9abf74d0c39e492e9e2b7 211 test_parse_markers_preserves_non_ascii_and_reports_utf16_columns
    #[test]
    fn test_parse_markers_preserves_non_ascii_and_reports_utf16_columns() {
        // "héllo" contains a 2-byte é; 😀 is 4 bytes / 2 UTF-16 units.
        let (cleaned, markers) = parse_markers(
            "test.ts",
            "const h\u{00E9}llo = \"\u{1F600}\";\n/*m*/h\u{00E9}llo;",
        );
        assert_eq!(
            cleaned,
            "const h\u{00E9}llo = \"\u{1F600}\";\nh\u{00E9}llo;"
        );
        assert_eq!(markers.len(), 1);
        assert_eq!(markers[0].line, 1);
        assert_eq!(markers[0].character, 0);
        // Byte offset into the cleaned text (line 1 starts after the 22-byte
        // first line + newline).
        assert_eq!(markers[0].offset, 23);
        assert_eq!(&cleaned[markers[0].offset as usize..], "h\u{00E9}llo;");
    }
// TSZ_INLINE_TEST_END f37b9d48b26df61b82ba61ca554ae5975895337b2de9abf74d0c39e492e9e2b7

// TSZ_INLINE_TEST_BEGIN a440b76a508473bd8b75f842d0ab5ce099e27cea2f94203b03d05bb263365611 231 test_parse_markers_after_non_ascii_on_same_line_counts_utf16
    #[test]
    fn test_parse_markers_after_non_ascii_on_same_line_counts_utf16() {
        let (cleaned, markers) = parse_markers("test.ts", "\u{1F600} + /*m*/x");
        assert_eq!(cleaned, "\u{1F600} + x");
        assert_eq!(markers.len(), 1);
        assert_eq!(markers[0].line, 0);
        // Emoji = 2 UTF-16 units, then " + " = 3.
        assert_eq!(markers[0].character, 5);
        // Byte offset: emoji = 4 bytes, " + " = 3.
        assert_eq!(markers[0].offset, 7);
    }
// TSZ_INLINE_TEST_END a440b76a508473bd8b75f842d0ab5ce099e27cea2f94203b03d05bb263365611

// TSZ_INLINE_TEST_BEGIN c3e48e204e7bceff34899dc31a8fe0839330696368abca0f62d4e46cfe2acd2e 243 test_parse_multi_file
    #[test]
    fn test_parse_multi_file() {
        let content =
            "// @filename: a.ts\nexport const x = 1;\n// @filename: b.ts\nimport { x } from './a';";
        let files = parse_multi_file(content);
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].0, "a.ts");
        assert_eq!(files[0].1, "export const x = 1;");
        assert_eq!(files[1].0, "b.ts");
        assert_eq!(files[1].1, "import { x } from './a';");
    }
// TSZ_INLINE_TEST_END c3e48e204e7bceff34899dc31a8fe0839330696368abca0f62d4e46cfe2acd2e

// TSZ_INLINE_TEST_BEGIN a88c1798b435a2a63c6d5d37ca5a159a3a6bf7fc4d2da86782746cd537ea78be 255 test_dedent
    #[test]
    fn test_dedent() {
        let input = "
            const x = 1;
            x + 1;
        ";
        let result = dedent(input);
        assert_eq!(result, "const x = 1;\nx + 1;");
    }
// TSZ_INLINE_TEST_END a88c1798b435a2a63c6d5d37ca5a159a3a6bf7fc4d2da86782746cd537ea78be

// TSZ_INLINE_TEST_BEGIN e7a041efba63fa58abb16a55dab512db78c82f2babd61ca6e42d865637101f74 265 test_fourslash_go_to_definition
    #[test]
    fn test_fourslash_go_to_definition() {
        let mut t = FourslashTest::new(
            "
            const /*def*/x = 1;
            /*ref*/x + 1;
        ",
        );
        t.go_to_definition("ref").expect_at_marker("def");
    }
// TSZ_INLINE_TEST_END e7a041efba63fa58abb16a55dab512db78c82f2babd61ca6e42d865637101f74

// TSZ_INLINE_TEST_BEGIN 34999e7d6b07e5d7fa1774208153a1048cc69f1a427f71ee9432b9e5ad092a25 276 test_fourslash_hover
    #[test]
    fn test_fourslash_hover() {
        let mut t = FourslashTest::new(
            "
            const /*x*/x = 42;
        ",
        );
        t.hover("x").expect_found();
    }
// TSZ_INLINE_TEST_END 34999e7d6b07e5d7fa1774208153a1048cc69f1a427f71ee9432b9e5ad092a25

// TSZ_INLINE_TEST_BEGIN 4338ea0e0ba0fddc7954378e7ebec5bd6fb7a2cda7458fa3f6872ef05b4123ff 286 test_fourslash_references
    #[test]
    fn test_fourslash_references() {
        let mut t = FourslashTest::new(
            "
            const /*def*/x = 1;
            /*ref1*/x + /*ref2*/x;
        ",
        );
        // Should find references (the definition + usages)
        t.references("def").expect_found();
    }
// TSZ_INLINE_TEST_END 4338ea0e0ba0fddc7954378e7ebec5bd6fb7a2cda7458fa3f6872ef05b4123ff

// TSZ_INLINE_TEST_BEGIN e21b6f302fb14ba06924cd691a838c53ed5bc61a899740923b377833757e921c 298 test_fourslash_multi_file
    #[test]
    fn test_fourslash_multi_file() {
        let mut t = FourslashTest::multi_file(&[
            ("a.ts", "export const x = 1;"),
            ("b.ts", "const /*def*/y = 2;\n/*ref*/y;"),
        ]);
        // Definition within same file should work
        t.go_to_definition("ref").expect_at_marker("def");
    }
// TSZ_INLINE_TEST_END e21b6f302fb14ba06924cd691a838c53ed5bc61a899740923b377833757e921c

// TSZ_INLINE_TEST_BEGIN 57b600093bd9d2c6ca87fb87e06ed46b899db23f33278b37d33d983684f8dea3 308 test_fourslash_document_symbols
    #[test]
    fn test_fourslash_document_symbols() {
        let mut t = FourslashTest::new(
            "
            function foo() {}
            class Bar {}
            const baz = 1;
        ",
        );
        t.document_symbols("test.ts")
            .expect_found()
            .expect_symbol("foo")
            .expect_symbol("Bar")
            .expect_symbol("baz");
    }
// TSZ_INLINE_TEST_END 57b600093bd9d2c6ca87fb87e06ed46b899db23f33278b37d33d983684f8dea3

// TSZ_INLINE_TEST_BEGIN ba3b0ff025f9f22c291d73cf4fa5a140d4f67a21a321881ffb4840fa54de2853 324 test_fourslash_completions
    #[test]
    fn test_fourslash_completions() {
        let mut t = FourslashTest::new(
            "
            const myVariable = 42;
            /**/my
        ",
        );
        // At the marker position, we should get completions including our variable
        let result = t.completions("");
        // Completions may or may not include myVariable depending on implementation
        // This just verifies the framework works
        // Framework test - completions query should work without panic
        let _ = result.items.len();
    }
// TSZ_INLINE_TEST_END ba3b0ff025f9f22c291d73cf4fa5a140d4f67a21a321881ffb4840fa54de2853

// TSZ_INLINE_TEST_BEGIN 26ac5e295e5065fde5bfe93d9c2dd08738fadd5e974227339a7b70655e57ca16 340 test_fourslash_rename
    #[test]
    fn test_fourslash_rename() {
        let mut t = FourslashTest::new(
            "
            const /*x*/x = 1;
            x + x;
        ",
        );
        t.rename("x", "y")
            .expect_success()
            .expect_edits_in_file("test.ts");
    }
// TSZ_INLINE_TEST_END 26ac5e295e5065fde5bfe93d9c2dd08738fadd5e974227339a7b70655e57ca16

// TSZ_INLINE_TEST_BEGIN 57f598fee9cd804b632b0fb325ce66fb0da759eaee173786a66d7cacea2a04c8 353 test_fourslash_at_filename_parsing
    #[test]
    fn test_fourslash_at_filename_parsing() {
        let t = FourslashTest::from_content(
            "// @filename: utils.ts\nexport function /*def*/helper() {}\n// @filename: main.ts\nimport { /*ref*/helper } from './utils';\nhelper();",
        );
        // Verify markers were parsed in correct files
        assert_eq!(t.marker_file("def"), "utils.ts");
        assert_eq!(t.marker_file("ref"), "main.ts");
    }
// TSZ_INLINE_TEST_END 57f598fee9cd804b632b0fb325ce66fb0da759eaee173786a66d7cacea2a04c8
