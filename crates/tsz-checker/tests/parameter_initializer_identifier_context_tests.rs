use tsz_checker::context::CheckerOptions;

fn diagnostic_codes(source: &str) -> Vec<u32> {
    tsz_checker::test_utils::check_source(source, "test.ts", CheckerOptions::default())
        .into_iter()
        .map(|d| d.code)
        .collect()
}

#[test]
fn annotated_parameter_default_identifier_still_checks_assignability() {
    let source = r#"
const value: number = 1;
function f(p: string = value) {}
"#;
    let codes = diagnostic_codes(source);
    assert!(
        codes.contains(&2322),
        "expected TS2322 for default identifier assignability, got: {codes:?}"
    );
}

#[test]
fn annotated_parameter_default_identifier_keeps_valid_literal_assignment() {
    let source = r#"
const value = 1 as const;
function f(p: 1 = value) {}
"#;
    let codes = diagnostic_codes(source);
    assert!(
        !codes.contains(&2322),
        "literal default identifier should be assignable to literal parameter, got: {codes:?}"
    );
}
