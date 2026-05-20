use crate::test_utils::check_source_diagnostics;

fn codes_for(source: &str) -> Vec<u32> {
    check_source_diagnostics(source)
        .into_iter()
        .map(|diagnostic| diagnostic.code)
        .collect()
}

#[test]
fn function_to_callable_interface_reports_signature_mismatch_after_valid_assignment() {
    let source = r#"
interface Callable {
  (x: number): string;
}

const c1: Callable = (x) => "";
const c2: Callable = (x: string) => x;

export {};
"#;

    let codes = codes_for(source);
    assert!(
        codes.contains(&2322),
        "Expected TS2322 for callable signature mismatch, got: {codes:?}"
    );
    assert!(
        !codes.contains(&2741),
        "Callable interface assignment must not fall back to missing `call` property, got: {codes:?}"
    );
}

#[test]
fn renamed_function_to_callable_interface_reports_signature_mismatch() {
    let source = r#"
interface Invokable {
  (value: boolean): string;
}

const ok: Invokable = (value) => "";
const bad: Invokable = (value: string) => value;

export {};
"#;

    let codes = codes_for(source);
    assert!(
        codes.contains(&2322),
        "Expected TS2322 for renamed callable interface mismatch, got: {codes:?}"
    );
    assert!(
        !codes.contains(&2741),
        "Renamed callable interface must not produce missing-property TS2741, got: {codes:?}"
    );
}

#[test]
fn valid_function_to_callable_interface_stays_assignable_after_prior_assignment() {
    let source = r#"
interface Callable {
  (x: number): string;
}

const c1: Callable = (x) => "";
const c2: Callable = (x) => "";

export {};
"#;

    let codes = codes_for(source);
    assert!(
        !codes.contains(&2741),
        "Repeated valid callable interface assignment must not require materialized Function members, got: {codes:?}"
    );
    assert!(
        !codes.contains(&2322),
        "Repeated valid callable interface assignment must remain assignable, got: {codes:?}"
    );
}

#[test]
fn function_to_inline_callable_object_reports_signature_mismatch() {
    let source = r#"
type Target = {
  (input: number): string;
};

const ok: Target = (input) => "";
const bad: Target = (input: string) => input;

export {};
"#;

    let codes = codes_for(source);
    assert!(
        codes.contains(&2322),
        "Expected TS2322 for inline callable object mismatch, got: {codes:?}"
    );
    assert!(
        !codes.contains(&2741),
        "Inline callable object must compare call signatures before properties, got: {codes:?}"
    );
}
