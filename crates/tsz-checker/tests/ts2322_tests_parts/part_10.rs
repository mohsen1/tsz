#[test]
fn primitive_number_literal_vs_weak_type_emits_ts2559() {
    // A number literal assigned to a weak type (all optional properties)
    // should emit TS2559, not TS2322/TS2345.
    // See: weakType.ts - `doSomething(12)`
    let source = r#"
        interface Settings {
            timeout?: number;
            onError?(): void;
        }
        function doSomething(settings: Settings) {}
        doSomething(12);
    "#;

    let diagnostics = get_all_diagnostics(source);
    let has_ts2559 = diagnostics.iter().any(|(code, _)| *code == 2559);
    assert!(
        has_ts2559,
        "Expected TS2559 for number literal assigned to weak type. Got: {diagnostics:?}"
    );
}

#[test]
fn primitive_string_literal_vs_weak_type_emits_ts2559() {
    // A string literal assigned to a weak type should emit TS2559.
    let source = r#"
        interface Settings {
            timeout?: number;
            onError?(): void;
        }
        function doSomething(settings: Settings) {}
        doSomething("completely wrong");
    "#;

    let diagnostics = get_all_diagnostics(source);
    let has_ts2559 = diagnostics.iter().any(|(code, _)| *code == 2559);
    assert!(
        has_ts2559,
        "Expected TS2559 for string literal assigned to weak type. Got: {diagnostics:?}"
    );
}

#[test]
fn primitive_boolean_literal_vs_weak_type_emits_ts2559() {
    // A boolean literal assigned to a weak type should emit TS2559.
    let source = r#"
        interface Settings {
            timeout?: number;
            onError?(): void;
        }
        function doSomething(settings: Settings) {}
        doSomething(false);
    "#;

    let diagnostics = get_all_diagnostics(source);
    let has_ts2559 = diagnostics.iter().any(|(code, _)| *code == 2559);
    assert!(
        has_ts2559,
        "Expected TS2559 for boolean literal assigned to weak type. Got: {diagnostics:?}"
    );
}

#[test]
fn enum_member_vs_weak_type_emits_ts2559() {
    // A string enum member assigned to a weak type with no common properties
    // should emit TS2559.
    // See: nestedExcessPropertyChecking.ts - `let x: { nope?: any } = E.A`
    let source = r#"
        enum E { A = "A" }
        let x: { nope?: any } = E.A;
    "#;

    let diagnostics = get_all_diagnostics(source);
    let has_ts2559 = diagnostics.iter().any(|(code, _)| *code == 2559);
    assert!(
        has_ts2559,
        "Expected TS2559 for enum member assigned to weak type. Got: {diagnostics:?}"
    );
}

#[test]
fn primitive_with_matching_property_passes_weak_type() {
    // A string assigned to a weak type that has 'length' property should NOT
    // trigger TS2559 because strings have a 'length' property.
    let source = r#"
        let x: { length?: number } = "hello" as any as string;
    "#;

    let diagnostics = get_all_diagnostics(source);
    let has_ts2559 = diagnostics.iter().any(|(code, _)| *code == 2559);
    assert!(
        !has_ts2559,
        "String should not trigger TS2559 for weak type with 'length' property. Got: {diagnostics:?}"
    );
}

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
    let has_ts2560 = diagnostics.iter().any(|(code, _)| *code == 2560);
    let has_ts2559 = diagnostics.iter().any(|(code, _)| *code == 2559);
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
    let has_ts2560 = diagnostics.iter().any(|(code, _)| *code == 2560);
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
    let has_ts2559 = diagnostics.iter().any(|(code, _)| *code == 2559);
    let has_ts2560 = diagnostics.iter().any(|(code, _)| *code == 2560);
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
    let has_ts2322 = diagnostics.iter().any(|(code, _)| *code == 2322);
    assert!(
        has_ts2322,
        "Expected TS2322 for incompatible generic callable assignment. Got: {diagnostics:?}"
    );
}

// ============================================================================
// TS2741 → TS2322 downgrade guards
// ============================================================================

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
    let has_ts2741 = diagnostics.iter().any(|(code, _)| *code == 2741);
    assert!(
        !has_ts2741,
        "Should not emit TS2741 for function→class assignment with private members. Got: {diagnostics:?}"
    );
    let has_ts2322 = diagnostics.iter().any(|(code, _)| *code == 2322);
    assert!(
        has_ts2322,
        "Expected TS2322 for function→class assignment. Got: {diagnostics:?}"
    );
}

