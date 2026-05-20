use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_source_codes, check_with_options};

fn check_codes_with_options(source: &str, options: CheckerOptions) -> Vec<(u32, String)> {
    check_with_options(source, options)
        .into_iter()
        .map(|diagnostic| (diagnostic.code, diagnostic.message_text))
        .collect()
}

fn check_strict_exact_optional(source: &str) -> Vec<(u32, String)> {
    check_codes_with_options(
        source,
        CheckerOptions {
            strict: true,
            strict_null_checks: true,
            exact_optional_property_types: true,
            ..Default::default()
        },
    )
}

#[test]
fn unrelated_object_union_with_nullish_target_still_reports_ts2322() {
    let codes = check_source_codes(
        r#"
declare const source: { a: number } | { b: string };
const target: { x: boolean } | { y: number } | undefined = source;
"#,
    );
    assert!(
        codes.contains(&2322),
        "unrelated object union arms must not be accepted by top-level arm-kind matching: {codes:?}"
    );
}

#[test]
fn renamed_unrelated_object_union_with_nullish_target_still_reports_ts2322() {
    let codes = check_source_codes(
        r#"
declare const value: { left: 1 } | { right: 2 };
const sink: { alpha: 1 } | { beta: 2 } | null = value;
"#,
    );
    assert!(
        codes.contains(&2322),
        "renamed unrelated object union arms must still emit TS2322: {codes:?}"
    );
}

#[test]
fn exact_optional_conditional_return_difference_reports_ts2322() {
    let diagnostics = check_strict_exact_optional(
        r#"
export let source: <T>() => T extends { prop?: string } ? 0 : 1 = null!;
export let target: <T>() => T extends { prop?: string | undefined } ? 0 : 1 = source;
"#,
    );
    assert!(
        diagnostics.iter().any(|(code, _)| *code == 2322),
        "exact optional conditional returns must preserve explicit undefined differences: {diagnostics:#?}"
    );
}

#[test]
fn exact_optional_conditional_return_difference_reports_ts2322_renamed() {
    let diagnostics = check_strict_exact_optional(
        r#"
export let first: <Value>() => Value extends { slot?: number } ? "yes" : "no" = null!;
export let second: <Value>() => Value extends { slot?: number | undefined } ? "yes" : "no" = first;
"#,
    );
    assert!(
        diagnostics.iter().any(|(code, _)| *code == 2322),
        "renamed exact optional conditional returns must not be accepted by structural fast paths: {diagnostics:#?}"
    );
}
