//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-lsp/src/fourslash_variants.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 2b8f8706f780ee8e546e8772763b671750f0254d72ea2aac849ee10b57ad7cc1 273 preserves_marker_names
    #[test]
    fn preserves_marker_names() {
        let src = "function /*def*/T() {}";
        let out = apply_variant(src, &RENAME_T_TO_K);
        assert_eq!(out, "function /*def*/K() {}");
    }
// TSZ_INLINE_TEST_END 2b8f8706f780ee8e546e8772763b671750f0254d72ea2aac849ee10b57ad7cc1

// TSZ_INLINE_TEST_BEGIN aa22528ca3ada7edd8ad0334c9883c322308532956ab027eaad3ba89eb61f298 280 whole_token_only
    #[test]
    fn whole_token_only() {
        // `Tree` and `Tea` contain `T` as a prefix but are not whole-token T.
        let src = "type T = Tree; type Tea = T;";
        let out = apply_variant(src, &RENAME_T_TO_K);
        assert_eq!(out, "type K = Tree; type Tea = K;");
    }
// TSZ_INLINE_TEST_END aa22528ca3ada7edd8ad0334c9883c322308532956ab027eaad3ba89eb61f298

// TSZ_INLINE_TEST_BEGIN fb2b5dfa889c15e6a521cafd588ccc2cd7f69cca11626a5bc3d343352c2670f7 288 rename_inside_strings_is_left_alone
    #[test]
    fn rename_inside_strings_is_left_alone() {
        let src = r#"const T: string = "T is here"; type T2 = T;"#;
        let out = apply_variant(src, &RENAME_T_TO_K);
        // The string body should be untouched even though it contains "T".
        assert_eq!(out, r#"const K: string = "T is here"; type T2 = K;"#);
    }
// TSZ_INLINE_TEST_END fb2b5dfa889c15e6a521cafd588ccc2cd7f69cca11626a5bc3d343352c2670f7

// TSZ_INLINE_TEST_BEGIN ac443938502d6a77762b851dbc41cc5edc2c67323e11bfc4c5829b7c16672d80 296 rename_inside_block_comments_is_left_alone
    #[test]
    fn rename_inside_block_comments_is_left_alone() {
        let src = "/* T is the type parameter */ type T = number;";
        let out = apply_variant(src, &RENAME_T_TO_K);
        assert_eq!(out, "/* T is the type parameter */ type K = number;");
    }
// TSZ_INLINE_TEST_END ac443938502d6a77762b851dbc41cc5edc2c67323e11bfc4c5829b7c16672d80

// TSZ_INLINE_TEST_BEGIN 4fbffdd96242521875ba453e66ece45d8e77aca325c950c94fe7eae5b430b91d 303 rename_inside_line_comments_is_left_alone
    #[test]
    fn rename_inside_line_comments_is_left_alone() {
        let src = "// T is a comment\ntype T = number;";
        let out = apply_variant(src, &RENAME_T_TO_K);
        assert_eq!(out, "// T is a comment\ntype K = number;");
    }
// TSZ_INLINE_TEST_END 4fbffdd96242521875ba453e66ece45d8e77aca325c950c94fe7eae5b430b91d

// TSZ_INLINE_TEST_BEGIN 5030ca2994b86a576de20e3aa3360eac8ba651cdbb483b4249ba36d0cf7ddf34 310 name_swap_in_one_pass
    #[test]
    fn name_swap_in_one_pass() {
        // Swap T <-> K. Naive sequential application would lose one.
        const SWAP: ShapeVariant = ShapeVariant {
            label: "swap",
            identifier_renames: &[("T", "K"), ("K", "T")],
            path_renames: &[],
        };
        let src = "type T = K; type K = T;";
        let out = apply_variant(src, &SWAP);
        assert_eq!(out, "type K = T; type T = K;");
    }
// TSZ_INLINE_TEST_END 5030ca2994b86a576de20e3aa3360eac8ba651cdbb483b4249ba36d0cf7ddf34

// TSZ_INLINE_TEST_BEGIN ada60f1378e80974df92a5c9d98812e7a59391c5ceb450b2e61b7a629a868038 323 multi_identifier_rename
    #[test]
    fn multi_identifier_rename() {
        let src = "function foo() { return bar(); }";
        let out = apply_variant(src, &RENAME_FOO_AND_BAR);
        assert_eq!(out, "function renamedFoo() { return renamedBar(); }");
    }
// TSZ_INLINE_TEST_END ada60f1378e80974df92a5c9d98812e7a59391c5ceb450b2e61b7a629a868038

// TSZ_INLINE_TEST_BEGIN 732d001b5709d4321d7b0e108012c90e7ebb38dccb2c0b85d5bd508c9d6bf7b7 330 at_filename_directive_path_is_rewritten
    #[test]
    fn at_filename_directive_path_is_rewritten() {
        let src = "// @filename: a.ts\nexport const x = 1;\n// @filename: b.ts\nimport './a';";
        let out = apply_variant(src, &RENAME_A_TS_PATH);
        assert!(out.contains("// @filename: renamed-a.ts"));
        assert!(out.contains("'./renamed-a'"));
        // `b.ts` was not renamed and must remain.
        assert!(out.contains("// @filename: b.ts"));
    }
// TSZ_INLINE_TEST_END 732d001b5709d4321d7b0e108012c90e7ebb38dccb2c0b85d5bd508c9d6bf7b7

// TSZ_INLINE_TEST_BEGIN 3bb7bfa3c7f4465fe82a1193df17f9983573adbd269478a5768163345fb3cc18 340 path_renames_do_not_touch_identifiers
    #[test]
    fn path_renames_do_not_touch_identifiers() {
        // path_renames are substring-only inside strings/filename directives;
        // they must not edit identifiers in code.
        let src = "const a = 1;\nimport './a';";
        let out = apply_variant(src, &RENAME_A_TS_PATH);
        // The identifier `a` is untouched. The string `'./a'` is rewritten.
        assert!(out.contains("const a = 1;"));
        assert!(out.contains("'./renamed-a'"));
    }
// TSZ_INLINE_TEST_END 3bb7bfa3c7f4465fe82a1193df17f9983573adbd269478a5768163345fb3cc18

// TSZ_INLINE_TEST_BEGIN c9100fa8288f62d6bd0d39643bf17ff6218c2bc4f6525af50d320fe35c7108bd 351 empty_variant_returns_input_verbatim
    #[test]
    fn empty_variant_returns_input_verbatim() {
        const NO_RENAMES: ShapeVariant = ShapeVariant {
            label: "noop",
            identifier_renames: &[],
            path_renames: &[],
        };
        let src = "const /*x*/x = 1;";
        let out = apply_variant(src, &NO_RENAMES);
        assert_eq!(out, src);
    }
// TSZ_INLINE_TEST_END c9100fa8288f62d6bd0d39643bf17ff6218c2bc4f6525af50d320fe35c7108bd

// TSZ_INLINE_TEST_BEGIN 053a5074eff4fec011687717d14e2c8bc98bff4dc41c7441f1c551e5b6c153de 363 shape_variants_includes_original_first
    #[test]
    fn shape_variants_includes_original_first() {
        let src = "type T = number;";
        let outs = shape_variants(src, &[RENAME_T_TO_K]);
        assert_eq!(outs.len(), 2);
        assert_eq!(outs[0].label, "original");
        assert_eq!(outs[0].source, src);
        assert_eq!(outs[1].label, "rename_T_to_K");
        assert_eq!(outs[1].source, "type K = number;");
    }
// TSZ_INLINE_TEST_END 053a5074eff4fec011687717d14e2c8bc98bff4dc41c7441f1c551e5b6c153de

// TSZ_INLINE_TEST_BEGIN 65533a62a01feba16ffc9a441b6f23dfbb766cead018d344a5c29726f3bc8ab9 374 multi_line_block_comment_is_preserved_verbatim
    #[test]
    fn multi_line_block_comment_is_preserved_verbatim() {
        // A block comment that spans newlines is not a marker and not
        // identifier-rewritten either; its body is forwarded as-is.
        let src = "/* T is\n   the param */ type T = number;";
        let out = apply_variant(src, &RENAME_T_TO_K);
        assert_eq!(out, "/* T is\n   the param */ type K = number;");
    }
// TSZ_INLINE_TEST_END 65533a62a01feba16ffc9a441b6f23dfbb766cead018d344a5c29726f3bc8ab9

// TSZ_INLINE_TEST_BEGIN 1463c4b0ee68f3f8724a0f889f1c128127c253c602c7dad278f82b9674fa5fa5 383 empty_marker_is_preserved
    #[test]
    fn empty_marker_is_preserved() {
        let src = "foo(/**/);";
        let out = apply_variant(src, &RENAME_FOO_AND_BAR);
        assert_eq!(out, "renamedFoo(/**/);");
    }
// TSZ_INLINE_TEST_END 1463c4b0ee68f3f8724a0f889f1c128127c253c602c7dad278f82b9674fa5fa5

// TSZ_INLINE_TEST_BEGIN a89de91a5b91e5f74a81c4c702f6947deba3a390ef06f4c7855a71daefbb7549 390 non_ascii_bytes_are_copied_as_whole_utf8_chars
    #[test]
    fn non_ascii_bytes_are_copied_as_whole_utf8_chars() {
        // The default-byte branch must copy a multi-byte UTF-8 char in one
        // step. A naive `bytes[i] as char` would emit Latin-1 garbage for
        // the lead byte and then re-enter the loop on a continuation byte.
        let src = "const T = 'π'; type T = number;";
        let out = apply_variant(src, &RENAME_T_TO_K);
        assert_eq!(out, "const K = 'π'; type K = number;");
    }
// TSZ_INLINE_TEST_END a89de91a5b91e5f74a81c4c702f6947deba3a390ef06f4c7855a71daefbb7549

// TSZ_INLINE_TEST_BEGIN dc2fefed17fa57ba7c846fc645ed4af0a004d0c234a8300b6975e694c6aef992 400 unterminated_string_does_not_panic
    #[test]
    fn unterminated_string_does_not_panic() {
        // Defensive: an unterminated string in a fixture must not crash the
        // generator. We forward the unterminated body verbatim.
        let src = "const x = 'unterminated;";
        let out = apply_variant(src, &RENAME_T_TO_K);
        assert_eq!(out, src);
    }
// TSZ_INLINE_TEST_END dc2fefed17fa57ba7c846fc645ed4af0a004d0c234a8300b6975e694c6aef992

// TSZ_INLINE_TEST_BEGIN bca2296b27558aa02960c5e6628c8982b51f7adf5dbd2ba495200babebc5e0d6 409 backtick_template_body_is_not_rewritten_for_identifiers
    #[test]
    fn backtick_template_body_is_not_rewritten_for_identifiers() {
        // Template-literal bodies are treated like string bodies for the
        // purpose of identifier rewriting (i.e. left alone). This is the
        // conservative choice; fourslash fixtures rarely contain templates.
        let src = "const T = `T inside template`; type T2 = T;";
        let out = apply_variant(src, &RENAME_T_TO_K);
        assert_eq!(out, "const K = `T inside template`; type T2 = K;");
    }
// TSZ_INLINE_TEST_END bca2296b27558aa02960c5e6628c8982b51f7adf5dbd2ba495200babebc5e0d6
