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

#[test]
fn default_parameter_arrow_initializer_uses_parameter_function_context() {
    let source = r#"
function withDefault(fn: (x: number) => string = (x) => "") {
    return fn(42);
}

const withDefault2 = (fn: (x: number) => string = (x) => "") => fn(42);
"#;
    let codes = diagnostic_codes(source);
    assert!(
        !codes.contains(&7006),
        "default arrow initializers should be contextually typed by parameter annotations, got: {codes:?}"
    );
}

#[test]
fn default_parameter_function_expression_initializer_uses_parameter_function_context() {
    let source = r#"
function withDefault(fn: (x: number) => string = function (x) { return ""; }) {
    return fn(42);
}
"#;
    let codes = diagnostic_codes(source);
    assert!(
        !codes.contains(&7006),
        "default function expression initializer should be contextually typed by parameter annotation, got: {codes:?}"
    );
}

#[test]
fn default_parameter_function_initializer_context_checks_return_type() {
    let source = r#"
function withDefault(fn: (x: number) => string = (x) => x) {
    return fn(42);
}
"#;
    let codes = diagnostic_codes(source);
    assert!(
        !codes.contains(&7006),
        "contextual default initializer should not emit TS7006, got: {codes:?}"
    );
    assert!(
        codes.contains(&2322),
        "contextual parameter type should make `(x) => x` incompatible with `(x: number) => string`, got: {codes:?}"
    );
}
