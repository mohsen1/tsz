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
    let has_ts2741 = diagnostics.iter().any(|(code, _)| *code == 2741);
    assert!(
        !has_ts2741,
        "Should not emit TS2741 for index signature target mismatch. Got: {diagnostics:?}"
    );
    let has_ts2322 = diagnostics.iter().any(|(code, _)| *code == 2322);
    assert!(
        has_ts2322,
        "Expected TS2322 for index signature target mismatch. Got: {diagnostics:?}"
    );
}

/// Regression: assignFromStringInterface2.ts
/// When both source and target have number index signatures but the source is
/// missing named properties from the target, TS2739/TS2740 should be emitted
/// (not TS2322). Number index signatures (common on String, Array, etc.) must
/// NOT suppress the missing-properties diagnostic.
#[test]
fn test_missing_properties_not_suppressed_by_number_index_signatures() {
    let source = r#"
        interface Target {
            foo(): string;
            bar(): string;
            baz(): string;
            qux(): string;
            quux(): string;
            corge(): string;
            grault(): string;
            [index: number]: string;
        }

        interface Source {
            foo(): string;
            [index: number]: string;
        }

        declare var target: Target;
        declare var source: Source;
        target = source;
    "#;

    let diagnostics = get_all_diagnostics(source);
    // TS2740 = "missing the following properties ... and N more" (6+ missing)
    let has_missing_props = diagnostics.iter().any(|(code, _)| {
        *code == diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE_AND_MORE
    });
    assert!(
        has_missing_props,
        "Expected TS2740 (missing properties) when both types have number index signatures \
         but source is missing named properties. Number index signatures should NOT suppress \
         missing-property diagnostics in favor of TS2322. Got: {diagnostics:?}"
    );
    // Should NOT have TS2322 for this case — TS2740 replaces it
    let has_ts2322 = diagnostics.iter().any(|(code, _)| *code == 2322);
    assert!(
        !has_ts2322,
        "Expected TS2740, not TS2322, when source is missing named properties. Got: {diagnostics:?}"
    );
}

/// When `strictBuiltinIteratorReturn` is true, `BuiltinIteratorReturn` resolves to `undefined`.
/// Assigning `undefined` to `number` must produce TS2322.
#[test]
fn test_strict_builtin_iterator_return_ts2322() {
    // Use BuiltinIteratorReturn directly — it's defined as `type BuiltinIteratorReturn = intrinsic`
    // in lib.es2015.iterable.d.ts and resolves to `undefined` when strict.
    let source = r#"
type R = BuiltinIteratorReturn;
const x: number = undefined as R;
"#;
    let options = CheckerOptions {
        strict_builtin_iterator_return: true,
        strict_null_checks: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_with_libs_for_ts(source, "test.ts", options);

    let ts2322_count = diagnostics
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .count();
    assert!(
        ts2322_count >= 1,
        "Expected TS2322 for assigning BuiltinIteratorReturn (=undefined) to number when \
         strictBuiltinIteratorReturn is true. Got: {diagnostics:?}"
    );
}

/// When `strictBuiltinIteratorReturn` is false, `BuiltinIteratorReturn` resolves to `any`.
/// Assigning `any` to `number` is always allowed, so no error.
#[test]
fn test_no_error_without_strict_builtin_iterator_return() {
    let source = r#"
declare const x: BuiltinIteratorReturn;
const r1: number = x;
"#;
    let options = CheckerOptions {
        strict_builtin_iterator_return: false,
        strict_null_checks: true,
        ..CheckerOptions::default()
    };
    let diagnostics = compile_with_libs_for_ts(source, "test.ts", options);

    let ts2322_count = diagnostics
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .count();
    assert!(
        ts2322_count == 0,
        "Expected no TS2322 when strictBuiltinIteratorReturn is false \
         (BuiltinIteratorReturn=any). Got: {diagnostics:?}"
    );
}

#[test]
fn test_ts2322_no_false_positive_const_type_param_multi() {
    // When a function has multiple type params and the first is `const`,
    // the solver's full inference path (used for >1 type params) must not
    // produce a false TS2322 on the argument. Previously, the final argument
    // check compared the checker's const-asserted arg type against the
    // solver's independently const-inferred type (different TypeIds for
    // semantically identical readonly types).
    let source = r#"
function f<const T, U>(x: T): T { return x; }
const t = f({ a: 1, b: "c", d: ["e", 2] });
"#;
    assert!(
        !has_error_with_code(source, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Should not emit TS2322 for const type parameter with multiple type params"
    );
}
