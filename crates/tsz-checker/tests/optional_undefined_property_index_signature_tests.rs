use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_source;

/// `{ k1?: undefined }` is NOT assignable to `{ [key: string]: string }`.
///
/// tsc emits TS2322 because the property `k1`, when present, is `undefined`
/// and `undefined` is not a member of the index signature value type
/// `string`. Stripping the implicit `| undefined` from optional properties
/// (which is correct for `{ k1?: number }` against `{ [key: string]: number }`)
/// must not eat the explicit `undefined` here — that would yield `never`,
/// which is vacuously assignable to anything and silences the real
/// mismatch.
#[test]
fn optional_undefined_property_against_string_index_emits_ts2322() {
    let opts = CheckerOptions {
        strict_null_checks: true,
        ..CheckerOptions::default()
    };
    let source = r#"
declare let optionalUndefined: { k1?: undefined };
let dict: { [key: string]: string } = optionalUndefined;
"#;
    let diags = check_source(source, "test.ts", opts);
    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "Expected one TS2322 for optional undefined vs string index, got: {diags:#?}"
    );
}

/// Sanity check the inverse: `{ k1?: number }` IS assignable to
/// `{ [key: string]: number }`. This is the case the implicit-undefined
/// strip exists to support; the fix above must not regress it.
#[test]
fn optional_number_property_against_number_string_index_no_error() {
    let opts = CheckerOptions {
        strict_null_checks: true,
        ..CheckerOptions::default()
    };
    let source = r#"
declare let optionalNumber: { k1?: number };
let dict: { [key: string]: number } = optionalNumber;
"#;
    let diags = check_source(source, "test.ts", opts);
    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "Expected no TS2322 for optional number vs string-index of number, got: {diags:#?}"
    );
}

/// `{ k1?: number | undefined }` is also assignable — the explicit
/// `| undefined` gets folded into the optional-implicit `| undefined`
/// and stripping leaves `number`, which matches the index value.
#[test]
fn optional_number_or_undefined_against_number_string_index_no_error() {
    let opts = CheckerOptions {
        strict_null_checks: true,
        ..CheckerOptions::default()
    };
    let source = r#"
declare let optionalNumberOrUndef: { k1?: number | undefined };
let dict: { [key: string]: number } = optionalNumberOrUndef;
"#;
    let diags = check_source(source, "test.ts", opts);
    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "Expected no TS2322 for optional number|undefined vs string-index of number, got: {diags:#?}"
    );
}
