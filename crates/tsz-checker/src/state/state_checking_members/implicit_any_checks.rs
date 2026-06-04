use crate::state::CheckerState;

use tsz_parser::parser::NodeIndex;

use tsz_solver::TypeId;

include!("implicit_any_checks_parts/part1.rs");
include!("implicit_any_checks_parts/part2.rs");

#[cfg(test)]
mod tests {
    fn check_codes_no_implicit_any(source: &str) -> Vec<u32> {
        crate::test_utils::check_source(
            source,
            "test.ts",
            crate::context::CheckerOptions {
                no_implicit_any: true,
                ..crate::context::CheckerOptions::default()
            },
        )
        .iter()
        .map(|d| d.code)
        .collect()
    }

    fn count_code(codes: &[u32], code: u32) -> usize {
        codes.iter().filter(|c| **c == code).count()
    }

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
}
