#[test]
fn test_ts2322_return_wrong_primitive() {
    let source = r#"
        function returnNumber(): number {
            return "string";
        }
    "#;

    assert!(has_error_with_code(
        source,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
    ));
}

#[test]
fn test_ts2322_return_wrong_object_property() {
    let source = r#"
        function returnObject(): { a: number } {
            return { a: "string" };
        }
    "#;

    assert!(has_error_with_code(
        source,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
    ));
}

#[test]
fn test_ts2322_return_wrong_array_element() {
    let source = r#"
        function returnArray(): number[] {
            return ["string"];
        }
    "#;

    assert!(has_error_with_code(
        source,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
    ));
}

#[test]
fn test_promise_is_assignable_to_promise_like_with_real_libs() {
    let libs = load_lib_files_for_test();
    if libs.is_empty() {
        return; // lib files not available
    }
    let source = r#"
declare const p: Promise<number>;
const q: PromiseLike<number> = p;
"#;

    let diagnostics = diagnostics_for_source(source);
    let relevant: Vec<_> = diagnostics.iter().filter(|d| d.code != 2318).collect();

    assert!(
        relevant.is_empty(),
        "Expected Promise<T> to be assignable to PromiseLike<T>, got: {relevant:?}"
    );
}

#[test]
fn test_ts2322_return_alias_instantiation_mismatch() {
    let source = r#"
        type Box<T> = { value: T };

        function returnBox(): Box<number> {
            const box: Box<string> = { value: "x" };
            return box;
        }
    "#;

    assert!(has_error_with_code(
        source,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
    ));
}

#[test]
fn mapped_type_inference_from_apparent_type_reports_ts2322() {
    let source = r#"
type Obj = {
    [s: string]: number;
};

type foo = <T>(target: { [K in keyof T]: T[K] }) => void;
type bar = <U extends string[]>(source: { [K in keyof U]: Obj[K] }) => void;

declare let f: foo;
declare let b: bar;
b = f;
"#;

    assert!(
        has_error_with_code(source, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "generic mapped assignment should preserve the apparent array constraint and report TS2322"
    );
}

#[test]
fn generic_signature_assignment_reports_expected_ts2322s() {
    let source = r#"
type A3 = <T>(x: T) => void;
type B3 = <T>(x: T) => T;
declare let a3: A3;
declare let b3: B3;
a3 = b3;
b3 = a3;

type A11 = <T>(x: { foo: T }, y: { foo: T; bar: T }) => void;
type B11 = <T, U>(x: { foo: T }, y: { foo: U; bar: U }) => void;
declare let a11: A11;
declare let b11: B11;
a11 = b11;
b11 = a11;

type Base = { foo: string };
type A16 = <T extends Base>(x: { a: T; b: T }) => T[];
type B16 = <T>(x: { a: T; b: T }) => T[];
declare let a16: A16;
declare let b16: B16;
a16 = b16;
b16 = a16;
"#;

    let diagnostics = get_all_diagnostics(source);
    let ts2322_errors: Vec<_> = diagnostics
        .into_iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();

    assert_eq!(
        ts2322_errors.len(),
        3,
        "Expected the three invalid reverse generic signature assignments to report TS2322, got: {ts2322_errors:?}"
    );
    assert!(
        ts2322_errors
            .iter()
            .any(|(_, message)| message.contains("Type 'A3' is not assignable to type 'B3'")),
        "Expected the void-return reverse assignment to surface as the A3/B3 TS2322, got: {ts2322_errors:?}"
    );
    assert!(
        ts2322_errors
            .iter()
            .any(|(_, message)| message.contains("Type 'A11' is not assignable to type 'B11'")),
        "Expected the mismatched correlated generic assignment to surface as the A11/B11 TS2322, got: {ts2322_errors:?}"
    );
    assert!(
        ts2322_errors
            .iter()
            .any(|(_, message)| message.contains("Type 'A16' is not assignable to type 'B16'")),
        "Expected the constrained generic reverse assignment to surface as the A16/B16 TS2322, got: {ts2322_errors:?}"
    );
}

#[test]
fn generic_construct_signature_assignment_reports_expected_ts2322s() {
    let source = r#"
type Base = { foo: string };

type A3 = new <T>(x: T) => void;
type B3 = new <T>(x: T) => T;
declare let a3: A3;
declare let b3: B3;
a3 = b3;
b3 = a3;

type A11 = new <T>(x: { foo: T }, y: { foo: T; bar: T }) => Base;
type B11 = new <T, U>(x: { foo: T }, y: { foo: U; bar: U }) => Base;
declare let a11: A11;
declare let b11: B11;
a11 = b11;
b11 = a11;

type A16 = new <T extends Base>(x: { a: T; b: T }) => T[];
type B16 = new <U, V>(x: { a: U; b: V }) => U[];
declare let a16: A16;
declare let b16: B16;
a16 = b16;
b16 = a16;
"#;

    let diagnostics = get_all_diagnostics(source);
    let ts2322_errors: Vec<_> = diagnostics
        .into_iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();

    assert_eq!(
        ts2322_errors.len(),
        3,
        "Expected the three invalid reverse generic construct-signature assignments to report TS2322, got: {ts2322_errors:?}"
    );
}

#[test]
fn generic_interface_member_signature_assignments_report_ts2322s() {
    let source = r#"
type Base = { foo: string };

interface A {
    a3: <T>(x: T) => void;
    a11: <T>(x: { foo: T }, y: { foo: T; bar: T }) => Base;
    a16: <T extends Base>(x: { a: T; b: T }) => T[];
}

declare let x: A;

declare let b3: <T>(x: T) => T;
x.a3 = b3;
b3 = x.a3;

declare let b11: <T, U>(x: { foo: T }, y: { foo: U; bar: U }) => Base;
x.a11 = b11;
b11 = x.a11;

declare let b16: <T>(x: { a: T; b: T }) => T[];
x.a16 = b16;
b16 = x.a16;
"#;

    let diagnostics = get_all_diagnostics(source);
    let ts2322_errors: Vec<_> = diagnostics
        .into_iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();

    assert_eq!(
        ts2322_errors.len(),
        3,
        "Expected the three invalid reverse member-signature assignments to report TS2322, got: {ts2322_errors:?}"
    );
}

#[test]
fn generic_interface_member_construct_signature_assignments_report_ts2322s() {
    let source = r#"
type Base = { foo: string };

interface A {
    a3: new <T>(x: T) => void;
    a11: new <T>(x: { foo: T }, y: { foo: T; bar: T }) => Base;
    a16: new <T extends Base>(x: { a: T; b: T }) => T[];
}

declare let x: A;

declare let b3: new <T>(x: T) => T;
x.a3 = b3;
b3 = x.a3;

declare let b11: new <T, U>(x: { foo: T }, y: { foo: U; bar: U }) => Base;
x.a11 = b11;
b11 = x.a11;

declare let b16: new <T>(x: { a: T; b: T }) => T[];
x.a16 = b16;
b16 = x.a16;
"#;

    let diagnostics = get_all_diagnostics(source);
    let ts2322_errors: Vec<_> = diagnostics
        .into_iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();

    assert_eq!(
        ts2322_errors.len(),
        3,
        "Expected the three invalid reverse member construct-signature assignments to report TS2322, got: {ts2322_errors:?}"
    );
}

