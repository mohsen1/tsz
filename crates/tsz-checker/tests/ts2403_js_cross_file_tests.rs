use tsz_checker::context::CheckerOptions;

fn check_ts_file_with_prior_js_global(js_source: &str, ts_source: &str) -> Vec<u32> {
    tsz_checker::test_utils::check_multi_file(
        &[("a.js", js_source), ("a.ts", ts_source)],
        "a.ts",
        CheckerOptions {
            allow_js: true,
            check_js: false,
            no_lib: true,
            ..Default::default()
        },
    )
    .into_iter()
    .map(|d| d.code)
    .collect()
}

#[test]
fn unchecked_js_global_does_not_trigger_cross_file_ts2403() {
    let codes = check_ts_file_with_prior_js_global(r#"var t = [1, "x"];"#, r#"var t: [any, any];"#);

    assert!(
        !codes.contains(&2403),
        "Unchecked JS globals should not participate in cross-file TS2403 comparisons. Actual codes: {codes:?}"
    );
}

#[test]
fn checked_js_global_does_not_trigger_cross_file_ts2403() {
    let codes = check_ts_file_with_prior_js_global(
        "// @ts-check\nvar t = [1, \"x\"];",
        r#"var t: [any, any];"#,
    );

    assert!(
        !codes.contains(&2403),
        "Checked JS globals should not act as the source side of cross-file TS2403 comparisons. Actual codes: {codes:?}"
    );
}
