use crate::test_utils::check_source_diagnostics;

fn diagnostic_codes(source: &str) -> Vec<u32> {
    check_source_diagnostics(source)
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect()
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

    assert!(
        codes.contains(&2536),
        "expected TS2536 in function type return annotation, got: {codes:?}"
    );
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

    assert!(
        codes.contains(&2536),
        "expected TS2536 independent of type parameter names, got: {codes:?}"
    );
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

    assert!(
        codes.contains(&2536),
        "expected TS2536 in function type parameter annotation, got: {codes:?}"
    );
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

    assert!(
        codes.contains(&2536),
        "expected TS2536 in constructor type return annotation, got: {codes:?}"
    );
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

    assert!(
        !codes.contains(&2536),
        "outer mapped key should remain valid in function type return annotation, got: {codes:?}"
    );
}
