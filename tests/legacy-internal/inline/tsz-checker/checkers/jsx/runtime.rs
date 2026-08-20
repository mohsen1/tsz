//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/checkers/jsx/runtime.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 707f20ea171998b82b62dbcbff21490642eefc3f88788520c9b0d28fd6318159 1063 jsx_runtime_classic_recognized
    #[test]
    fn jsx_runtime_classic_recognized() {
        assert_eq!(
            extract_jsx_runtime_pragma("/* @jsxRuntime classic */\nconst x = 1;"),
            Some("classic")
        );
    }
// TSZ_INLINE_TEST_END 707f20ea171998b82b62dbcbff21490642eefc3f88788520c9b0d28fd6318159

// TSZ_INLINE_TEST_BEGIN 2fe4c1ab1210953ec2394641618c6387c85571d483896069f8b73b6057aaf148 1071 jsx_runtime_automatic_recognized
    #[test]
    fn jsx_runtime_automatic_recognized() {
        assert_eq!(
            extract_jsx_runtime_pragma("/** @jsxRuntime automatic */\n"),
            Some("automatic")
        );
    }
// TSZ_INLINE_TEST_END 2fe4c1ab1210953ec2394641618c6387c85571d483896069f8b73b6057aaf148

// TSZ_INLINE_TEST_BEGIN d0d8061d203c8e95bdbec3398e86efaf6c661cd71e07a641b080c4b872d399d1 1079 jsx_runtime_prefix_tag_is_ignored
    #[test]
    fn jsx_runtime_prefix_tag_is_ignored() {
        // `@jsxRuntimeautomatic` is not the @jsxRuntime tag — it is some
        // unknown JSDoc tag. Must not switch to automatic mode.
        assert_eq!(
            extract_jsx_runtime_pragma("/** @jsxRuntimeautomatic */\n"),
            None
        );
        assert_eq!(
            extract_jsx_runtime_pragma("/* @jsxRuntimeclassic */\n"),
            None
        );
    }
// TSZ_INLINE_TEST_END d0d8061d203c8e95bdbec3398e86efaf6c661cd71e07a641b080c4b872d399d1

// TSZ_INLINE_TEST_BEGIN d4b30affd40947679cd36c29fcbad2e809cc97bc9c1fcc2066b5cd423f10fe40 1093 jsx_runtime_invalid_value_with_suffix_is_ignored
    #[test]
    fn jsx_runtime_invalid_value_with_suffix_is_ignored() {
        // Tag boundary holds, but the value `automaticx` is not `automatic`.
        assert_eq!(
            extract_jsx_runtime_pragma("/** @jsxRuntime automaticx */\n"),
            None
        );
        assert_eq!(
            extract_jsx_runtime_pragma("/** @jsxRuntime classicx */\n"),
            None
        );
    }
// TSZ_INLINE_TEST_END d4b30affd40947679cd36c29fcbad2e809cc97bc9c1fcc2066b5cd423f10fe40

// TSZ_INLINE_TEST_BEGIN 0f7068865f422fbe3625a3d02de1f7bae967edc80495194aa5f342e47a8feb9b 1106 jsx_runtime_unknown_value_is_ignored
    #[test]
    fn jsx_runtime_unknown_value_is_ignored() {
        assert_eq!(
            extract_jsx_runtime_pragma("/** @jsxRuntime hybrid */\n"),
            None
        );
    }
// TSZ_INLINE_TEST_END 0f7068865f422fbe3625a3d02de1f7bae967edc80495194aa5f342e47a8feb9b

// TSZ_INLINE_TEST_BEGIN bf9f13cdee32f992dfcb16178281ef320f2ec5dfbd07913718279ecb993b58ac 1114 jsx_runtime_later_valid_pragma_still_wins_after_invalid_prefix
    #[test]
    fn jsx_runtime_later_valid_pragma_still_wins_after_invalid_prefix() {
        // tsc keeps the last valid occurrence; a junk `@jsxRuntimeautomatic`
        // earlier must not poison a later real `@jsxRuntime classic`.
        let src = "/** @jsxRuntimeautomatic */\n/** @jsxRuntime classic */\n";
        assert_eq!(extract_jsx_runtime_pragma(src), Some("classic"));
    }
// TSZ_INLINE_TEST_END bf9f13cdee32f992dfcb16178281ef320f2ec5dfbd07913718279ecb993b58ac

// TSZ_INLINE_TEST_BEGIN 777ac6700a47627cf9495c69620d09458965782d33756bf14f232d2d94508f8d 1124 jsx_import_source_recognized
    #[test]
    fn jsx_import_source_recognized() {
        assert_eq!(
            extract_jsx_import_source_pragma_text_only_for_test("/** @jsxImportSource preact */\n"),
            Some("preact".to_string())
        );
    }
// TSZ_INLINE_TEST_END 777ac6700a47627cf9495c69620d09458965782d33756bf14f232d2d94508f8d

// TSZ_INLINE_TEST_BEGIN 1c9ce885f258b140772a0f0d3d111448e3860ad5c392aefb0f278548795dd879 1132 jsx_import_source_prefix_tag_is_ignored
    #[test]
    fn jsx_import_source_prefix_tag_is_ignored() {
        // `@jsxImportSourcex preact` is an unrelated tag — must not yield
        // package `x` (the previous bug) or `preact`.
        assert_eq!(
            extract_jsx_import_source_pragma_text_only_for_test(
                "/** @jsxImportSourcex preact */\n"
            ),
            None
        );
    }
// TSZ_INLINE_TEST_END 1c9ce885f258b140772a0f0d3d111448e3860ad5c392aefb0f278548795dd879

// TSZ_INLINE_TEST_BEGIN 764aba4f8088b44e4a09dd0e11169c1280598624ed6f99caa2ef5df396ad0170 1144 jsx_import_source_scoped_package_recognized
    #[test]
    fn jsx_import_source_scoped_package_recognized() {
        assert_eq!(
            extract_jsx_import_source_pragma_text_only_for_test(
                "/* @jsxImportSource @emotion/react */\n"
            ),
            Some("@emotion/react".to_string())
        );
    }
// TSZ_INLINE_TEST_END 764aba4f8088b44e4a09dd0e11169c1280598624ed6f99caa2ef5df396ad0170

// TSZ_INLINE_TEST_BEGIN 53116d157bf05d78bcd17250317ff6b0872b25cec627165cf57d38bf238268a7 1156 jsx_frag_recognized
    #[test]
    fn jsx_frag_recognized() {
        assert_eq!(
            extract_jsx_frag_pragma("/** @jsxFrag Fragment */\n"),
            Some("Fragment".to_string())
        );
    }
// TSZ_INLINE_TEST_END 53116d157bf05d78bcd17250317ff6b0872b25cec627165cf57d38bf238268a7

// TSZ_INLINE_TEST_BEGIN d4a2005dc0ddc53e400df63645dc17e88fd51e60782caa23bcf107d426caf4f2 1164 jsx_fragment_long_form_recognized
    #[test]
    fn jsx_fragment_long_form_recognized() {
        // tsc accepts `@jsxFragment` as a synonym; previously the longer form
        // would be parsed as `@jsxFrag` plus an `ment` suffix, which now
        // (correctly) fails the boundary check — so the longer form must be
        // tried first.
        assert_eq!(
            extract_jsx_frag_pragma("/** @jsxFragment Foo */\n"),
            Some("Foo".to_string())
        );
    }
// TSZ_INLINE_TEST_END d4a2005dc0ddc53e400df63645dc17e88fd51e60782caa23bcf107d426caf4f2

// TSZ_INLINE_TEST_BEGIN aca4413e86fb54e405adf67eb487b4e1bd95e332f6c1d12b74a04467289c0a5e 1176 jsx_frag_prefix_tag_is_ignored
    #[test]
    fn jsx_frag_prefix_tag_is_ignored() {
        assert_eq!(extract_jsx_frag_pragma("/** @jsxFragx Fragment */\n"), None);
        assert_eq!(extract_jsx_frag_pragma("/** @jsxFragmentx Foo */\n"), None);
    }
// TSZ_INLINE_TEST_END aca4413e86fb54e405adf67eb487b4e1bd95e332f6c1d12b74a04467289c0a5e

// TSZ_INLINE_TEST_BEGIN d707ebfc6d906d412fc368a629a98b71c62cee3f9d164172fb2bb205a137f746 1184 jsx_factory_pragma_still_recognized
    #[test]
    fn jsx_factory_pragma_still_recognized() {
        assert_eq!(extract_jsx_pragma("/** @jsx h */\n"), Some("h".to_string()));
        assert_eq!(
            extract_jsx_pragma("/** @jsx React.createElement */\n"),
            Some("React.createElement".to_string())
        );
    }
// TSZ_INLINE_TEST_END d707ebfc6d906d412fc368a629a98b71c62cee3f9d164172fb2bb205a137f746

// TSZ_INLINE_TEST_BEGIN 0416f96859f602379e97dc8bad34be81e38f96938eb2e6695d3f4e078f2e7cc9 1210 jsx_extractors_do_not_panic_on_multibyte_at_scan_cap
    #[test]
    fn jsx_extractors_do_not_panic_on_multibyte_at_scan_cap() {
        // No pragma present: the contract is simply "never panic" on a file
        // whose multi-byte char straddles the 4096-byte scan cap.
        let src = straddle_scan_cap("// header ", 'Н');
        assert!(src.len() > 4096);
        assert_eq!(extract_jsx_pragma(&src), None);
        assert_eq!(extract_jsx_frag_pragma(&src), None);
        assert_eq!(extract_jsx_runtime_pragma(&src), None);
        assert_eq!(
            extract_jsx_import_source_pragma_text_only_for_test(&src),
            None
        );
    }
// TSZ_INLINE_TEST_END 0416f96859f602379e97dc8bad34be81e38f96938eb2e6695d3f4e078f2e7cc9

// TSZ_INLINE_TEST_BEGIN 97ad0edfba41344ed67219a4b7e6fc2406050e86fbec5a06b228c40afed2dff3 1225 jsx_import_source_found_even_when_file_straddles_scan_cap
    #[test]
    fn jsx_import_source_found_even_when_file_straddles_scan_cap() {
        // The real pragma sits in the leading comment (well inside the window);
        // the multi-byte char straddling byte 4096 must not break detection and
        // must not panic.
        let src = straddle_scan_cap("/* @jsxImportSource preact */\n", 'Н');
        assert!(src.len() > 4096);
        assert_eq!(
            extract_jsx_import_source_pragma_text_only_for_test(&src),
            Some("preact".to_string())
        );
    }
// TSZ_INLINE_TEST_END 97ad0edfba41344ed67219a4b7e6fc2406050e86fbec5a06b228c40afed2dff3
