//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-cli/src/bin/tsz_server/handlers_quickinfo_text.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 503658bd794c542b90dcf4da4e410b9059533841432fb95f9ce9ce5dc7979eec 850 normalize_quickinfo_display_string_normalizes_object_call_signature_spacing
    #[test]
    fn normalize_quickinfo_display_string_normalizes_object_call_signature_spacing() {
        let display = "var c3t7: {\n    (n: number) : number;\n    (s1: string) : number;\n}";
        let normalized = normalize_quickinfo_display_string(display);
        assert_eq!(
            normalized,
            "var c3t7: {\n    (n: number): number;\n    (s1: string): number;\n}"
        );
    }
// TSZ_INLINE_TEST_END 503658bd794c542b90dcf4da4e410b9059533841432fb95f9ce9ce5dc7979eec

// TSZ_INLINE_TEST_BEGIN 61b8827819b740d20f62ca1c453d9ce8c19f2a3afcf77f682230511289dd9a60 860 assignment_lhs_property_offset_before_function_supports_array_wrapped_rhs
    #[test]
    fn assignment_lhs_property_offset_before_function_supports_array_wrapped_rhs() {
        let source = "objc8.t11 = [function(n, s) { return s; }];";
        let function_pos = source
            .find("function")
            .expect("function keyword should exist") as u32;
        let offset = assignment_lhs_property_offset_before_function(source, function_pos)
            .expect("should find lhs property offset");
        assert_eq!(
            source[offset as usize..]
                .chars()
                .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
                .collect::<String>(),
            "t11"
        );
    }
// TSZ_INLINE_TEST_END 61b8827819b740d20f62ca1c453d9ce8c19f2a3afcf77f682230511289dd9a60

// TSZ_INLINE_TEST_BEGIN c5292c64049aea9125a924e731a2d8823be7091d19a785c9d7eed59e216a204f 877 contextual_parameter_type_from_text_extracts_function_array_parameter
    #[test]
    fn contextual_parameter_type_from_text_extracts_function_array_parameter() {
        let type_text = "((n: number, s: string) => string)[]";
        assert_eq!(
            contextual_parameter_type_from_text(type_text, 0).as_deref(),
            Some("number")
        );
        assert_eq!(
            contextual_parameter_type_from_text(type_text, 1).as_deref(),
            Some("string")
        );
    }
// TSZ_INLINE_TEST_END c5292c64049aea9125a924e731a2d8823be7091d19a785c9d7eed59e216a204f

// TSZ_INLINE_TEST_BEGIN d839ae8013b433dfb6bbb63d7f85ac6ea89e6981fb02f21fe7cd328ad2ecd18d 890 normalize_quickinfo_display_string_converts_single_call_signature_object_array
    #[test]
    fn normalize_quickinfo_display_string_converts_single_call_signature_object_array() {
        let display = "var c3t11: {(n: number, s: string): string;}[]";
        let normalized = normalize_quickinfo_display_string(display);
        assert_eq!(
            normalized,
            "var c3t11: ((n: number, s: string) => string)[]"
        );
    }
// TSZ_INLINE_TEST_END d839ae8013b433dfb6bbb63d7f85ac6ea89e6981fb02f21fe7cd328ad2ecd18d

// TSZ_INLINE_TEST_BEGIN 6999552a03280e11271e60c362171107244c86b30e664bd3c89f2c1d33e1671a 900 normalize_quickinfo_display_string_keeps_multi_signature_object_literal
    #[test]
    fn normalize_quickinfo_display_string_keeps_multi_signature_object_literal() {
        let display = "var c3t7: {\n    (n: number): number;\n    (s1: string): number;\n}";
        let normalized = normalize_quickinfo_display_string(display);
        assert_eq!(
            normalized,
            "var c3t7: {\n    (n: number): number;\n    (s1: string): number;\n}"
        );
    }
// TSZ_INLINE_TEST_END 6999552a03280e11271e60c362171107244c86b30e664bd3c89f2c1d33e1671a

// TSZ_INLINE_TEST_BEGIN 3cc359d68ae987a1080506213dcf596f2568212470fb4575bca3cff353493f51 910 normalize_quickinfo_display_string_multiline_index_signature_object
    #[test]
    fn normalize_quickinfo_display_string_multiline_index_signature_object() {
        let display = "(local var) r2: { [x: string]: T; }";
        let normalized = normalize_quickinfo_display_string(display);
        assert_eq!(normalized, "(local var) r2: {\n    [x: string]: T;\n}");
    }
// TSZ_INLINE_TEST_END 3cc359d68ae987a1080506213dcf596f2568212470fb4575bca3cff353493f51

// TSZ_INLINE_TEST_BEGIN e75e738ce77958eb5270ab0694ce0e8861a1c3ce14b2929d0823bdf06f5ef680 917 normalize_quickinfo_display_string_flattens_single_rest_tuple_param
    #[test]
    fn normalize_quickinfo_display_string_flattens_single_rest_tuple_param() {
        let display = "var fnWrapped: (...a: [str: string, num: number]) => void";
        let normalized = normalize_quickinfo_display_string(display);
        assert_eq!(
            normalized,
            "var fnWrapped: (str: string, num: number) => void"
        );
    }
// TSZ_INLINE_TEST_END e75e738ce77958eb5270ab0694ce0e8861a1c3ce14b2929d0823bdf06f5ef680

// TSZ_INLINE_TEST_BEGIN 7cd7cff009da58c25b6fe0a8deb6e7abc2ba100978896160aea0f34d97701e44 927 normalize_quickinfo_display_string_flattens_single_rest_tuple_param_variadic
    #[test]
    fn normalize_quickinfo_display_string_flattens_single_rest_tuple_param_variadic() {
        let display = "var fnVariadicWrapped: (...a: [str: string, ...num: number[]]) => void";
        let normalized = normalize_quickinfo_display_string(display);
        assert_eq!(
            normalized,
            "var fnVariadicWrapped: (str: string, ...num: number[]) => void"
        );
    }
// TSZ_INLINE_TEST_END 7cd7cff009da58c25b6fe0a8deb6e7abc2ba100978896160aea0f34d97701e44

// TSZ_INLINE_TEST_BEGIN 03493977d3412910fc66374391cf2136c417a738e1222ebf96ff98b10d41ecee 937 normalize_quickinfo_display_string_collapses_single_any_rest_param
    #[test]
    fn normalize_quickinfo_display_string_collapses_single_any_rest_param() {
        let display = "var fnNoParamsWrapped: (...a: any[]) => void";
        let normalized = normalize_quickinfo_display_string(display);
        assert_eq!(normalized, "var fnNoParamsWrapped: () => void");
    }
// TSZ_INLINE_TEST_END 03493977d3412910fc66374391cf2136c417a738e1222ebf96ff98b10d41ecee

// TSZ_INLINE_TEST_BEGIN 719f76c75ab6011b2ff4b3231b104a8e15dee04c4ba5d603cf8ffc9b2e9b696e 944 is_type_annotation_context_detects_marker_after_type_reference
    #[test]
    fn is_type_annotation_context_detects_marker_after_type_reference() {
        let source = "const i: foo/*m*/ = { x: 1 };";
        let marker_start = source.find("/*m*/").expect("marker") as u32;
        assert!(is_type_annotation_context(source, marker_start));
    }
// TSZ_INLINE_TEST_END 719f76c75ab6011b2ff4b3231b104a8e15dee04c4ba5d603cf8ffc9b2e9b696e
