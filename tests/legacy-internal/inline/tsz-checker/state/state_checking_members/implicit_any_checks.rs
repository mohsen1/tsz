//! Disabled inline tests extracted from the pre-reset compiler.
//! Source: `crates/tsz-checker/src/state/state_checking_members/implicit_any_checks.rs`
//! Commit: `2770da88d4456b68fb80a27da3fa41aa5e6d7bf0`

// TSZ_INLINE_TEST_BEGIN 577ecfea8f3cf5426e0003b42202435dd6589bad10bd093bd9433e4a371d3491 1773 filter_callback_on_any_receiver_still_reports_ts7006
    #[test]
    fn filter_callback_on_any_receiver_still_reports_ts7006() {
        let codes =
            check_codes_no_implicit_any("declare const values: any; values.filter(x => x);");
        assert_eq!(
            count_code(&codes, 7006),
            1,
            "expected TS7006 for callback parameter on any.filter, got {codes:?}"
        );
    }
// TSZ_INLINE_TEST_END 577ecfea8f3cf5426e0003b42202435dd6589bad10bd093bd9433e4a371d3491

// TSZ_INLINE_TEST_BEGIN 3079b17cf79eedd91479326e16a9aa28014fc360cc94d047f5292ff8046067aa 1784 filter_callback_on_any_object_keys_result_still_reports_ts7006
    #[test]
    fn filter_callback_on_any_object_keys_result_still_reports_ts7006() {
        let codes = check_codes_no_implicit_any(
            r#"
declare const util: { objectKeys: any };
const keys = util.objectKeys({});
keys.filter(k => k);
"#,
        );
        assert_eq!(
            count_code(&codes, 7006),
            1,
            "expected TS7006 when objectKeys itself is any, got {codes:?}"
        );
    }
// TSZ_INLINE_TEST_END 3079b17cf79eedd91479326e16a9aa28014fc360cc94d047f5292ff8046067aa

// TSZ_INLINE_TEST_BEGIN 5f3339bc1a43f6aeca3c6c0e688cf1ce070efcea125dde3b192210413595113b 1800 filter_callback_on_unresolved_array_receiver_suppresses_ts7006
    #[test]
    fn filter_callback_on_unresolved_array_receiver_suppresses_ts7006() {
        let codes =
            check_codes_no_implicit_any("declare const values: Missing[]; values.filter(x => x);");
        assert_eq!(
            count_code(&codes, 7006),
            0,
            "expected no TS7006 when the receiver is explicitly array-like, got {codes:?}"
        );
    }
// TSZ_INLINE_TEST_END 5f3339bc1a43f6aeca3c6c0e688cf1ce070efcea125dde3b192210413595113b

// TSZ_INLINE_TEST_BEGIN 2289609ade2dbf308b0860aa7b098d1c458c937780ea8f9fbda040f57ec5552a 1811 filter_callback_on_destructured_array_property_suppresses_ts7006
    #[test]
    fn filter_callback_on_destructured_array_property_suppresses_ts7006() {
        let codes = check_codes_no_implicit_any(
            r#"
function makeIssue(params: { errorMaps: (Missing | undefined)[] }) {
    const { errorMaps } = params;
    errorMaps.filter(m => !!m);
}
"#,
        );
        assert_eq!(
            count_code(&codes, 7006),
            0,
            "expected no TS7006 for destructured array-like property filter, got {codes:?}"
        );
    }
// TSZ_INLINE_TEST_END 2289609ade2dbf308b0860aa7b098d1c458c937780ea8f9fbda040f57ec5552a

// TSZ_INLINE_TEST_BEGIN 63a69bc144a525bd36516030e13ab6697e654e8a018186714231d378b6d60dff 1828 unresolved_explicit_callable_annotation_defers_arrow_ts7006
    #[test]
    fn unresolved_explicit_callable_annotation_defers_arrow_ts7006() {
        let codes = check_codes_no_implicit_any(
            r#"
type ErrorMap = (iss: { code: string }, ctx: { defaultError: string }) => { message: string };
const customMap: ErrorMap = (iss, ctx) => {
    return { message: ctx.defaultError };
};

const isInteger: NumberConstructor["isInteger"] =
    typeof Number.isInteger === "function" ? val => true : val => false;

const objectKeys: ObjectConstructor["keys"] = (obj: any) => [];
const firstKeys = objectKeys({});
firstKeys.filter(k => k);
"#,
        );
        assert_eq!(
            count_code(&codes, 7006),
            0,
            "expected no TS7006 for arrows with explicit callable annotations, got {codes:?}"
        );
    }
// TSZ_INLINE_TEST_END 63a69bc144a525bd36516030e13ab6697e654e8a018186714231d378b6d60dff

// TSZ_INLINE_TEST_BEGIN b7ad4a662cbbb655a379db012e17ae30cea07f1433ff1e8979bb2342da1e37bf 1852 ts7031_emitted_for_array_pattern_index_beyond_array_default
    #[test]
    fn ts7031_emitted_for_array_pattern_index_beyond_array_default() {
        // `[x, y] = [1]` — the default literal `[1]` covers index 0 only, so
        // `y` at index 1 must still report TS7031 (implicit any).
        let codes = check_codes_no_implicit_any("function f02([x, y] = [1]) {}");
        assert_eq!(
            count_code(&codes, 7031),
            1,
            "expected exactly one TS7031 (for `y`) in `[x, y] = [1]`, got {codes:?}"
        );
    }
// TSZ_INLINE_TEST_END b7ad4a662cbbb655a379db012e17ae30cea07f1433ff1e8979bb2342da1e37bf

// TSZ_INLINE_TEST_BEGIN f005eee9983afa33f6cc29b924ef191c04717675bda4e1686bee15e2dd2575d7 1864 ts7031_emitted_for_array_pattern_index_beyond_array_default_with_inner_default
    #[test]
    fn ts7031_emitted_for_array_pattern_index_beyond_array_default_with_inner_default() {
        // `[x = 0, y] = [1]` — `x` has its own default, so no TS7031 for x.
        // `y` at index 1 is still uncovered by the literal and has no own
        // default, so TS7031 must fire for `y`.
        let codes = check_codes_no_implicit_any("function f12([x = 0, y] = [1]) {}");
        assert_eq!(
            count_code(&codes, 7031),
            1,
            "expected exactly one TS7031 (for `y`) in `[x = 0, y] = [1]`, got {codes:?}"
        );
    }
// TSZ_INLINE_TEST_END f005eee9983afa33f6cc29b924ef191c04717675bda4e1686bee15e2dd2575d7

// TSZ_INLINE_TEST_BEGIN e8657c909a31664b56f9b09c11ac617a440e4d6e30f23b45fab77e443f53a9e4 1877 no_ts7031_when_array_default_covers_pattern
    #[test]
    fn no_ts7031_when_array_default_covers_pattern() {
        // `[x, y] = [1, 'foo']` — both indices are covered by the literal,
        // so the bindings are implicitly typed `number` / `string`. No TS7031.
        let codes = check_codes_no_implicit_any("function f03([x, y] = [1, 'foo']) {}");
        assert_eq!(
            count_code(&codes, 7031),
            0,
            "expected no TS7031 when literal default covers all binding indices, got {codes:?}"
        );
    }
// TSZ_INLINE_TEST_END e8657c909a31664b56f9b09c11ac617a440e4d6e30f23b45fab77e443f53a9e4

// TSZ_INLINE_TEST_BEGIN c2ba09fff19d4d5cb5c3d3fdd2a55ee2925030312c288deddda57212d533db38 1889 no_ts7031_when_inner_default_present_beyond_array_default
    #[test]
    fn no_ts7031_when_inner_default_present_beyond_array_default() {
        // `[x = 0, y = 'bar'] = [1]` — `y` has an own default `'bar'` so it
        // is typed `string`. Even though the literal does not cover index 1,
        // no TS7031 should fire.
        let codes = check_codes_no_implicit_any("function f22([x = 0, y = 'bar'] = [1]) {}");
        assert_eq!(
            count_code(&codes, 7031),
            0,
            "expected no TS7031 when leaves carry their own default, got {codes:?}"
        );
    }
// TSZ_INLINE_TEST_END c2ba09fff19d4d5cb5c3d3fdd2a55ee2925030312c288deddda57212d533db38

// TSZ_INLINE_TEST_BEGIN 2185e2d8157f9647313921b0b2231569f58604f12d3b535e9053dd1685117471 1902 ts7031_for_each_uncovered_index_in_longer_pattern
    #[test]
    fn ts7031_for_each_uncovered_index_in_longer_pattern() {
        // `[x, y, z] = [1]` — only index 0 is covered. y and z must each
        // report TS7031.
        let codes = check_codes_no_implicit_any("function fN([x, y, z] = [1]) {}");
        assert_eq!(
            count_code(&codes, 7031),
            2,
            "expected TS7031 for both `y` and `z`, got {codes:?}"
        );
    }
// TSZ_INLINE_TEST_END 2185e2d8157f9647313921b0b2231569f58604f12d3b535e9053dd1685117471

// TSZ_INLINE_TEST_BEGIN 6b865ce16250e4d79046e12c6548dbfd05429d1a299be82cfb5ce7fcfa4a9ebf 1914 no_ts7031_for_array_pattern_with_spread_default
    #[test]
    fn no_ts7031_for_array_pattern_with_spread_default() {
        // `[x, y] = [...rest]` — spread makes the literal's effective length
        // not statically known. We conservatively skip TS7031 (matching tsc,
        // which infers a tuple type from the spread context).
        let codes = check_codes_no_implicit_any(
            "declare const rest: number[]; function f([x, y] = [...rest]) {}",
        );
        assert_eq!(
            count_code(&codes, 7031),
            0,
            "expected no TS7031 when default contains a spread, got {codes:?}"
        );
    }
// TSZ_INLINE_TEST_END 6b865ce16250e4d79046e12c6548dbfd05429d1a299be82cfb5ce7fcfa4a9ebf

// TSZ_INLINE_TEST_BEGIN 7fadf4ddebd4882bff2bd05a1015ce3e255b0924b38c7a37c54cb3bbab6795cb 1929 ts7019_emitted_with_rest_not_last_parse_error
    #[test]
    fn ts7019_emitted_with_rest_not_last_parse_error() {
        // tsc emits TS7019 for rest params even when TS1014 (rest not last) is present.
        // TS1014 is a parser error (not in checker diagnostics), but TS7019 must appear.
        let codes = crate::test_utils::check_source_codes("function f(...x, y) { }");
        assert!(
            codes.contains(&7019),
            "Should have TS7019 for rest param even with parse errors, got {codes:?}"
        );
        // TS7006 should also be emitted for the regular parameter `y`
        assert!(
            codes.contains(&7006),
            "Should have TS7006 for regular param y, got {codes:?}"
        );
    }
// TSZ_INLINE_TEST_END 7fadf4ddebd4882bff2bd05a1015ce3e255b0924b38c7a37c54cb3bbab6795cb

// TSZ_INLINE_TEST_BEGIN a19dad832c86d30dbd46922c30fab7f09673ddc140ddb69e87078d7faf243b60 1945 ts7019_emitted_with_syntax_parse_errors_flag
    #[test]
    fn ts7019_emitted_with_syntax_parse_errors_flag() {
        // When has_syntax_parse_errors is set (as in the CLI driver path),
        // rest params should still get TS7019.
        let source = "function f(...x, y) { }";
        let options = crate::context::CheckerOptions::default();
        let mut parser =
            tsz_parser::parser::ParserState::new("test.ts".to_string(), source.to_string());
        let sf = parser.parse_source_file();
        let mut binder = tsz_binder::BinderState::new();
        binder.bind_source_file(parser.get_arena(), sf);
        let types = crate::query_boundaries::type_construction::TypeInterner::new();
        let mut checker = crate::state::CheckerState::new(
            parser.get_arena(),
            &binder,
            &types,
            "test.ts".to_string(),
            options,
        );
        checker.ctx.set_lib_contexts(Vec::new());
        // Simulate the CLI driver setting has_syntax_parse_errors
        checker.ctx.has_syntax_parse_errors = true;
        checker.check_source_file(sf);
        let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
        assert!(
            codes.contains(&7019),
            "Should have TS7019 for rest param with has_syntax_parse_errors, got {codes:?}"
        );
    }
// TSZ_INLINE_TEST_END a19dad832c86d30dbd46922c30fab7f09673ddc140ddb69e87078d7faf243b60

// TSZ_INLINE_TEST_BEGIN 5f1082cf90f12a8b6e5aa263579d52a8b6f15d119365bcb645b745c1adbdd857 1975 ts7019_emitted_with_optional_rest_parse_error
    #[test]
    fn ts7019_emitted_with_optional_rest_parse_error() {
        // tsc emits TS7019 for rest params even when TS1047 (rest can't be optional) is present.
        // TS1047 is a parser error (not in checker diagnostics), but TS7019 must appear.
        let codes = crate::test_utils::check_source_codes("(...arg?) => 102;");
        assert!(
            codes.contains(&7019),
            "Should have TS7019 for rest param even with parse errors, got {codes:?}"
        );
    }
// TSZ_INLINE_TEST_END 5f1082cf90f12a8b6e5aa263579d52a8b6f15d119365bcb645b745c1adbdd857
