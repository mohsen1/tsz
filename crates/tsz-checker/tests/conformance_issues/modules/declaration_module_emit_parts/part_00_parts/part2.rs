#[test]
fn test_type_parameter_function_return_type_not_equivalent() {
    // Function types with different type parameter return types should NOT be assignable.
    // This is the typeParameterArgumentEquivalence conformance test family.

    // () => T is NOT assignable to () => U (and vice versa)
    let d = compile_and_get_diagnostics(
        "function f<T,U>() { var x!: () => U; var y!: () => T; x = y; y = x; }",
    );
    let ts2322_count = d.iter().filter(|(c, _)| *c == 2322).count();
    assert_eq!(
        ts2322_count, 2,
        "Expected 2 TS2322 for () => T vs () => U, got: {d:?}"
    );

    // (a: T) => boolean is NOT assignable to (a: U) => boolean (and vice versa)
    let d = compile_and_get_diagnostics(
        "function f<T,U>() { var x!: (a: U) => boolean; var y!: (a: T) => boolean; x = y; y = x; }",
    );
    let ts2322_count = d.iter().filter(|(c, _)| *c == 2322).count();
    assert_eq!(
        ts2322_count, 2,
        "Expected 2 TS2322 for (a:T) vs (a:U), got: {d:?}"
    );

    // But () => T IS assignable to () => T (same type parameter)
    let d =
        compile_and_get_diagnostics("function f<T>() { var x!: () => T; var y!: () => T; x = y; }");
    let ts2322_count = d.iter().filter(|(c, _)| *c == 2322).count();
    assert_eq!(
        ts2322_count, 0,
        "Same type param should be assignable, got: {d:?}"
    );
}

/// TS2416: class method `f(a: T): void` is not compatible with interface
/// property `f: (a: { a: number }) => void` because T extends { a: string }
/// and { a: number } is not assignable to { a: string }.
#[test]
fn test_generic_type_with_non_generic_base_mismatch_ts2416() {
    let source = r#"
interface I {
    f: (a: { a: number }) => void
}
class X<T extends { a: string }> implements I {
    f(a: T): void { }
}
var x = new X<{ a: string }>();
var i: I = x;
"#;
    let options = CheckerOptions {
        strict: true,
        strict_null_checks: true,
        strict_function_types: true,
        ..CheckerOptions::default()
    };
    let d = compile_and_get_diagnostics_with_options(source, options);
    let ts2416_count = d.iter().filter(|(c, _)| *c == 2416).count();
    assert!(
        ts2416_count >= 1,
        "Expected TS2416 for property 'f' incompatibility, got: {d:?}"
    );
    // Should also emit TS2322 for the assignment `var i: I = x`
    let ts2322_count = d.iter().filter(|(c, _)| *c == 2322).count();
    assert!(
        ts2322_count >= 1,
        "Expected TS2322 for incompatible assignment, got: {d:?}"
    );
}

#[test]
fn test_type_parameter_nested_function_return_type_not_equivalent() {
    // Nested function types with different type parameter return types should NOT be assignable.
    // This is the typeParameterArgumentEquivalence5 conformance test.

    // () => (item: any) => T is NOT assignable to () => (item: any) => U (and vice versa)
    let d = compile_and_get_diagnostics(
        "function foo<T,U>() { var x!: () => (item: any) => U; var y!: () => (item: any) => T; x = y; y = x; }",
    );
    let ts2322_count = d.iter().filter(|(c, _)| *c == 2322).count();
    assert_eq!(
        ts2322_count, 2,
        "Expected 2 TS2322 for () => (item: any) => T vs () => (item: any) => U, got: {d:?}"
    );

    // But same type parameter through nesting IS assignable
    let d = compile_and_get_diagnostics(
        "function foo<T>() { var x!: () => (item: any) => T; var y!: () => (item: any) => T; x = y; }",
    );
    let ts2322_count = d.iter().filter(|(c, _)| *c == 2322).count();
    assert_eq!(
        ts2322_count, 0,
        "Same type param through nesting should be assignable, got: {d:?}"
    );
}

/// TS7005 should fire for variables inside `declare namespace` that lack
/// a type annotation, when `noImplicitAny` is enabled.
/// Regression test for: conformance/implicitAnyAmbients.ts
#[test]
fn test_ts7005_emitted_for_ambient_namespace_variables() {
    let source = r#"
declare namespace m {
    var x;
    var y: any;
    namespace n {
        var z;
    }
}
"#;
    let diagnostics = compile_and_get_diagnostics_with_options(
        source,
        CheckerOptions {
            no_implicit_any: true,
            ..CheckerOptions::default()
        },
    );

    let ts7005_diags: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == 7005)
        .collect();

    // Should emit TS7005 for `var x;` and `var z;` (no type annotation)
    // but NOT for `var y: any;` (has explicit type annotation)
    assert_eq!(
        ts7005_diags.len(),
        2,
        "Expected exactly 2 TS7005 diagnostics (for `x` and `z`), got: {ts7005_diags:?}"
    );

    // Verify the messages reference the correct variable names
    let messages: Vec<&str> = ts7005_diags.iter().map(|(_, m)| m.as_str()).collect();
    assert!(
        messages.iter().any(|m| m.contains("'x'")),
        "Expected TS7005 for variable 'x', got: {messages:?}"
    );
    assert!(
        messages.iter().any(|m| m.contains("'z'")),
        "Expected TS7005 for variable 'z', got: {messages:?}"
    );

    // `var y: any` should NOT trigger TS7005 — it has an explicit type annotation
    assert!(
        !messages.iter().any(|m| m.contains("'y'")),
        "var y: any should NOT trigger TS7005, got: {messages:?}"
    );
}

/// TS7005 should NOT fire for ambient namespace variables in .d.ts files.
#[test]
fn test_ts7005_not_emitted_for_dts_ambient_namespace_variables() {
    let source = r#"
declare namespace m {
    var x;
}
"#;
    let diagnostics = compile_and_get_diagnostics_named(
        "test.d.ts",
        source,
        CheckerOptions {
            no_implicit_any: true,
            ..CheckerOptions::default()
        },
    );

    let ts7005_count = diagnostics.iter().filter(|(code, _)| *code == 7005).count();
    assert_eq!(
        ts7005_count, 0,
        "TS7005 should not fire in .d.ts files, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts7005_not_emitted_for_for_of_const_binding_with_inferred_element_type() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        r#"
for (const value of [1, 2, 3]) {
    value.toFixed();
}
"#,
        CheckerOptions {
            no_implicit_any: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    let ts7005_count = diagnostics.iter().filter(|(code, _)| *code == 7005).count();
    assert_eq!(
        ts7005_count, 0,
        "Loop element inference should suppress TS7005 for `for...of` bindings, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts7005_emitted_for_plain_const_without_initializer() {
    let diagnostics = compile_and_get_diagnostics_with_options(
        "const value",
        CheckerOptions {
            no_implicit_any: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    assert!(
        diagnostics.iter().any(|(code, _)| *code == 7005),
        "Plain `const` declarations without initializers should still report TS7005, got: {diagnostics:?}"
    );
}
