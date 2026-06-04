#[test]
fn callable_value_to_weak_type_emits_ts2560_not_ts2559() {
    // When passing a callable value to a parameter with a weak type (all optional
    // properties), and calling the value would produce a compatible type,
    // tsc emits TS2560 ("did you mean to call it?") instead of TS2559.
    // See: weakType.ts - `doSomething(getDefaultSettings)`
    let source = r#"
        interface Settings {
            timeout?: number;
            onError?(): void;
        }
        function getDefaultSettings() {
            return { timeout: 1000 };
        }
        function doSomething(settings: Settings) {}
        doSomething(getDefaultSettings);
    "#;

    let diagnostics = get_all_diagnostics(source);
    let has_ts2560 = has_diagnostic_code(&diagnostics, 2560);
    let has_ts2559 = has_diagnostic_code(&diagnostics, 2559);
    assert!(
        has_ts2560,
        "Expected TS2560 for callable value assigned to weak type. Got: {diagnostics:?}"
    );
    assert!(
        !has_ts2559,
        "Should emit TS2560, not TS2559, for callable value. Got: {diagnostics:?}"
    );
}

#[test]
fn arrow_function_to_weak_type_emits_ts2560() {
    // An arrow function returning a compatible type should emit TS2560.
    // See: weakType.ts - `doSomething(() => ({ timeout: 1000 }))`
    let source = r#"
        interface Settings {
            timeout?: number;
            onError?(): void;
        }
        function doSomething(settings: Settings) {}
        doSomething(() => ({ timeout: 1000 }));
    "#;

    let diagnostics = get_all_diagnostics(source);
    let has_ts2560 = has_diagnostic_code(&diagnostics, 2560);
    assert!(
        has_ts2560,
        "Expected TS2560 for arrow function assigned to weak type. Got: {diagnostics:?}"
    );
}

#[test]
fn primitive_still_emits_ts2559_not_ts2560() {
    // Primitives (non-callable) should still emit TS2559, not TS2560.
    let source = r#"
        interface Settings {
            timeout?: number;
            onError?(): void;
        }
        function doSomething(settings: Settings) {}
        doSomething(12);
        doSomething(false);
    "#;

    let diagnostics = get_all_diagnostics(source);
    let has_ts2559 = has_diagnostic_code(&diagnostics, 2559);
    let has_ts2560 = has_diagnostic_code(&diagnostics, 2560);
    assert!(
        has_ts2559,
        "Expected TS2559 for primitives assigned to weak type. Got: {diagnostics:?}"
    );
    assert!(
        !has_ts2560,
        "Should not emit TS2560 for non-callable primitives. Got: {diagnostics:?}"
    );
}

/// Regression: genericFunctionCallSignatureReturnTypeMismatch.ts
/// `{ <S>(): S[] }` assigned to `{ <T>(x: T): T }` should emit TS2322
/// because the return types are incompatible (S[] is not assignable to type param S).
#[test]
fn test_generic_callable_return_type_mismatch_emits_ts2322() {
    let source = r#"
        declare var f: { <T>(x: T): T; };
        declare var g: { <S>(): S[]; };
        f = g;
    "#;

    let diagnostics = get_all_diagnostics(source);
    let has_ts2322 = has_diagnostic_code(&diagnostics, 2322);
    assert!(
        has_ts2322,
        "Expected TS2322 for incompatible generic callable assignment. Got: {diagnostics:?}"
    );
}

/// When a function type is assigned to a class with private members, TSC emits TS2322
/// (generic assignability), not TS2741 (missing property). Private brands should be
/// handled as nominal class mismatches.
#[test]
fn test_function_to_class_with_private_emits_ts2322_not_ts2741() {
    let source = r#"
        class C { private x = 1; }
        class D extends C { }
        function foo(x: "hi", items: string[]): typeof foo;
        function foo(x: string, items: string[]): typeof foo { return null as any; }
        var a: D = foo("hi", []);
    "#;
    let diagnostics = get_all_diagnostics(source);
    let has_ts2741 = has_diagnostic_code(&diagnostics, 2741);
    assert!(
        !has_ts2741,
        "Should not emit TS2741 for function→class assignment with private members. Got: {diagnostics:?}"
    );
    let has_ts2322 = has_diagnostic_code(&diagnostics, 2322);
    assert!(
        has_ts2322,
        "Expected TS2322 for function→class assignment. Got: {diagnostics:?}"
    );
}

/// When assigning to a type with an index signature, and the "missing" property comes
/// from the index signature value type (not a direct named property), TSC emits TS2322.
#[test]
fn test_index_signature_target_missing_prop_emits_ts2322_not_ts2741() {
    let source = r#"
        type A = { a: string };
        type B = { b: string };
        declare let sb1: { x: A } & { y: B };
        declare let tb1: { [key: string]: A };
        tb1 = sb1;
    "#;
    let diagnostics = get_all_diagnostics(source);
    let has_ts2741 = has_diagnostic_code(&diagnostics, 2741);
    assert!(
        !has_ts2741,
        "Should not emit TS2741 for index signature target mismatch. Got: {diagnostics:?}"
    );
    let has_ts2322 = has_diagnostic_code(&diagnostics, 2322);
    assert!(
        has_ts2322,
        "Expected TS2322 for index signature target mismatch. Got: {diagnostics:?}"
    );
}

#[test]
fn test_named_generic_interface_requires_declared_number_index_signature() {
    let source = r#"
namespace __test1__ {
    export interface Box<T, U> {
        one: T;
        two?: U;
    }
    var obj4: Box<number, string> = { one: 1 };
    export var __val__obj4 = obj4;
}
namespace __test2__ {
    export declare var aa: { [index: number]: number };
    export var __val__aa = aa;
}
__test2__.__val__aa = __test1__.__val__obj4;
"#;
    let diagnostics = get_all_diagnostics(source);
    let has_ts2322 = diagnostics
        .iter()
        .any(|(code, message)| *code == 2322 && message.contains("{ [index: number]: number; }"));
    assert!(
        has_ts2322,
        "Expected TS2322 for named generic interface assigned to numeric index target. Got: {diagnostics:?}"
    );
}

#[test]
fn test_union_index_signature_object_literal_value_mismatches_emit_ts2322() {
    let source = r#"
interface IValue {
  value: string
}

interface StringKeys {
    [propertyName: string]: IValue;
};

interface NumberKeys {
    [propertyName: number]: IValue;
}

type ObjectDataSpecification = StringKeys | NumberKeys;

const dataSpecification: ObjectDataSpecification = {
    foo: "asdfsadffsd"
};

const obj1: { [x: string]: number } | { [x: number]: number } = { a: 'abc' };
const obj2: { [x: string]: number } | { a: number } = { a: 5, c: 'abc' };
"#;

    let diagnostics = get_all_diagnostics(source);
    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2322)
        .collect();
    assert_eq!(
        ts2322.len(),
        3,
        "Expected three TS2322 index-signature value mismatches. Got: {diagnostics:?}"
    );
    assert!(
        ts2322
            .iter()
            .any(|(_, message)| message
                .contains("Type 'string' is not assignable to type 'IValue'.")),
        "Expected string-to-IValue mismatch. Got: {diagnostics:?}"
    );
    assert_eq!(
        ts2322
            .iter()
            .filter(|(_, message)| message
                .contains("Type 'string' is not assignable to type 'number'."))
            .count(),
        2,
        "Expected two string-to-number mismatches. Got: {diagnostics:?}"
    );
}
