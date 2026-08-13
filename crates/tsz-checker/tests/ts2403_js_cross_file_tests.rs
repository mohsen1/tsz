use tsz_checker::context::CheckerOptions;

/// `a.ts` is file 0 (checked; the only file this harness actually runs
/// `check_source_file` on), `a.js` is file 1. Oracle-verified (typescript-go
/// 7.0.2, `scripts/conformance/oracle.sh`): for a cross-file merged global
/// `var`, the LAST file in program order establishes the symbol's canonical
/// type, and earlier-declaring files are checked against it — regardless of
/// whether that later JS file is itself checked. Putting the JS file last
/// here (rather than first) is what makes that canonical-direction exercised
/// by a harness that only checks the entry file.
fn check_ts_file_with_later_js_global(ts_source: &str, js_source: &str) -> Vec<u32> {
    tsz_checker::test_utils::check_multi_file(
        &[("a.ts", ts_source), ("a.js", js_source)],
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
fn unchecked_js_global_establishes_cross_file_ts2403_type() {
    let codes = check_ts_file_with_later_js_global(r#"var t: [any, any];"#, r#"var t = [1, "x"];"#);

    assert!(
        codes.contains(&2403),
        "A later JS global establishes the merged var's type for cross-file TS2403 even when \
         the JS file is unchecked; the error is reported at the earlier TS declaration \
         (oracle-verified: `var t: [any, any];` in a.ts, then `var t = [1, \"x\"];` in a.js \
         reports TS2403 at a.ts, while the reverse file order reports nothing since a.ts, \
         declared last, becomes canonical). Actual codes: {codes:?}"
    );
}

#[test]
fn checked_js_global_establishes_cross_file_ts2403_type() {
    let codes = check_ts_file_with_later_js_global(
        r#"var t: [any, any];"#,
        "// @ts-check\nvar t = [1, \"x\"];",
    );

    assert!(
        codes.contains(&2403),
        "A later checked-JS global establishes the merged var's type for cross-file TS2403; \
         the error is reported at the earlier TS declaration. Actual codes: {codes:?}"
    );
}

#[test]
fn matching_js_global_type_does_not_trigger_cross_file_ts2403() {
    let codes = check_ts_file_with_later_js_global(r#"var t: number;"#, r#"var t = 1;"#);

    assert!(
        !codes.contains(&2403),
        "Identical merged var types across a JS/TS pair must not report TS2403. \
         Actual codes: {codes:?}"
    );
}
