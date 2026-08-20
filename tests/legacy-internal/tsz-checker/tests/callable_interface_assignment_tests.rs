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
fn nonfunction_source_to_callsignature_interface_reports_signature_mismatch_not_missing_call() {
    // A call-signature interface that happens to be *named* `Callable` must be
    // compared structurally: a non-function object source provides no match for
    // the call signature (TS2322), and must NOT fall back to a manufactured
    // "Property 'call' is missing" (TS2741) keyed on the type's display name.
    let source = r#"
interface Callable {
  (x: number): string;
}

declare const obj: { other: number };
const c: Callable = obj;

export {};
"#;

    let codes = codes_for(source);
    assert!(
        codes.contains(&2322),
        "Expected TS2322 signature mismatch for non-function source, got: {codes:?}"
    );
    assert!(
        !codes.contains(&2741),
        "Display-name `Callable` must not manufacture a missing-`call` TS2741, got: {codes:?}"
    );
}

#[test]
fn renamed_nonfunction_source_to_callsignature_interface_reports_signature_mismatch() {
    // Same shape with every binder renamed: the structural result is identical,
    // proving the diagnostic does not depend on the `Callable`/`Applicable`
    // identifier strings.
    let source = r#"
interface Dispatcher {
  (payload: boolean): number;
}

declare const obj: { tag: string };
const d: Dispatcher = obj;

export {};
"#;

    let codes = codes_for(source);
    assert!(
        codes.contains(&2322),
        "Expected TS2322 signature mismatch for renamed call-signature interface, got: {codes:?}"
    );
    assert!(
        !codes.contains(&2741),
        "Renamed call-signature interface must not manufacture TS2741, got: {codes:?}"
    );
}

#[test]
fn missing_single_required_property_reports_real_property_name_structurally() {
    // When the target genuinely has exactly one missing required *property*, tsc
    // reports that property by its real name. The reported name is derived
    // structurally from the missing member, not from the target's display name,
    // so a target named `Callable` whose missing property is `summon` reports
    // `summon` (never the hardcoded `call`).
    let source = r#"
interface Callable {
  summon: () => void;
}

declare const obj: { other: number };
const c: Callable = obj;

export {};
"#;

    let diagnostics = check_source_diagnostics(source);
    let has_summon_missing = diagnostics.iter().any(|d| {
        d.code == 2741 && d.message_text.contains("summon") && !d.message_text.contains("'call'")
    });
    assert!(
        has_summon_missing,
        "Expected TS2741 naming the real missing property `summon`, got: {:?}",
        diagnostics
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
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
