use crate::test_utils::check_source_diagnostics;

fn diagnostic_codes(source: &str) -> Vec<u32> {
    check_source_diagnostics(source)
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect()
}

fn count_code(codes: &[u32], code: u32) -> usize {
    codes.iter().filter(|&&observed| observed == code).count()
}

fn assert_single_ts2536_without_ts2322(codes: &[u32], context: &str) {
    assert_eq!(
        count_code(codes, 2536),
        1,
        "expected exactly one TS2536 {context}, got: {codes:?}"
    );
    assert_eq!(
        count_code(codes, 2322),
        0,
        "inner constrained type parameter should not add TS2322 {context}, got: {codes:?}"
    );
}

#[test]
fn function_type_return_annotation_checks_shadowed_indexed_access() {
    let codes = diagnostic_codes(
        r#"
type Test<T> = {
    [K in keyof T]: (<K extends string>() => { [P in K]: T[P] });
};
"#,
    );

    assert_single_ts2536_without_ts2322(&codes, "in function type return annotation");
}

#[test]
fn function_type_return_annotation_checks_renamed_shadowed_indexed_access() {
    let codes = diagnostic_codes(
        r#"
type Renamed<T> = {
    [Outer in keyof T]: (<Inner extends string>() => { [Key in Inner]: T[Key] });
};
"#,
    );

    assert_single_ts2536_without_ts2322(&codes, "independent of type parameter names");
}

#[test]
fn function_type_parameter_annotation_checks_shadowed_indexed_access() {
    let codes = diagnostic_codes(
        r#"
type Param<T> = {
    [K in keyof T]: (<K extends string>(arg: { [P in K]: T[P] }) => void);
};
"#,
    );

    assert_single_ts2536_without_ts2322(&codes, "in function type parameter annotation");
}

#[test]
fn constructor_type_return_annotation_checks_shadowed_indexed_access() {
    let codes = diagnostic_codes(
        r#"
type Ctor<T> = {
    [K in keyof T]: new <K extends string>() => { [P in K]: T[P] };
};
"#,
    );

    assert_single_ts2536_without_ts2322(&codes, "in constructor type return annotation");
}

#[test]
fn function_type_return_annotation_preserves_outer_mapped_key_scope() {
    let codes = diagnostic_codes(
        r#"
type Ok<T> = {
    [K in keyof T]: (() => { [P in K]: T[P] });
};
"#,
    );

    assert_eq!(
        count_code(&codes, 2536),
        0,
        "outer mapped key should remain valid in function type return annotation, got: {codes:?}"
    );
    assert_eq!(
        count_code(&codes, 2322),
        0,
        "outer mapped key should not produce TS2322, got: {codes:?}"
    );
}
