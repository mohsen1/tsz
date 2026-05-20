use tsz_checker::test_utils::check_source_codes;

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
