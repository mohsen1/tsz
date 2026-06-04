#[test]
fn test_ts2322_strict_js_strictness_affects_nullability() {
    let source = r"
        // @ts-check
        /** @type {number} */
        const maybeNumber = null;
    ";

    let loose = compile_with_options(
        source,
        "test.js",
        CheckerOptions {
            check_js: true,
            strict: false,
            strict_null_checks: false,
            ..CheckerOptions::default()
        },
    );
    let strict = compile_with_options(
        source,
        "test.js",
        CheckerOptions {
            check_js: true,
            strict: true,
            ..CheckerOptions::default()
        },
    );

    let strict_has_2322 =
        has_diagnostic_code(&strict, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE);
    assert!(
        strict_has_2322,
        "Expected strict+checkJs to emit TS2322 for null -> number jsdoc mismatch, got: {strict:?}"
    );
    assert!(
        strict.len() > loose.len(),
        "Expected strict mode to increase diagnostics for nullability in checkJs source"
    );
}

#[test]
fn test_ts2322_target_es2015_enables_template_lib_type_checks_without_falsely_reporting_target() {
    let source = r#"
        const x: number = 1;
        const y = "2";
        const z: number = y as any;
    "#;

    let diagnostics = compile_with_options(
        source,
        "test.ts",
        CheckerOptions {
            strict: true,
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );
    let has_2322 = has_diagnostic_code(
        &diagnostics,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
    );
    assert!(
        !has_2322,
        "No TS2322 expected in valid ES2015 + strict baseline case: {diagnostics:?}"
    );
}

#[test]
fn test_ts2322_target_es3_vs_target_es2015_jsdoc_annotation_mismatch() {
    let source = r#"
        // @ts-check
        /** @type {number} */
        const value = "bad";
    "#;

    let es3 = compile_with_options(
        source,
        "test.js",
        CheckerOptions {
            check_js: true,
            target: ScriptTarget::ES3,
            strict: true,
            ..Default::default()
        },
    );
    let es2022 = compile_with_options(
        source,
        "test.js",
        CheckerOptions {
            check_js: true,
            target: ScriptTarget::ES2022,
            strict: true,
            ..Default::default()
        },
    );
    let es3_has_2322 = has_diagnostic_code(&es3, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE);
    let es2022_has_2322 =
        has_diagnostic_code(&es2022, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE);
    assert!(
        es3_has_2322 && es2022_has_2322,
        "Expected jsdoc mismatch TS2322 under both targets, got es3={es3:?}, es2022={es2022:?}"
    );
}

#[test]
fn test_call_object_literal_optional_param_prefers_property_ts2322_over_ts2345() {
    let source = r#"
function foo({ x, y, z }?: { x: string; y: number; z: boolean }) {}
foo({ x: false, y: 0, z: "" });
"#;

    let diagnostics = get_all_diagnostics(source);
    let ts2322_count = diagnostic_count(
        &diagnostics,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
    );
    let has_ts2345 = diagnostics.iter().any(|(code, _)| {
        *code == diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE
    });

    assert!(
        ts2322_count >= 2,
        "Expected property-level TS2322 for the mismatched object-literal fields, got: {diagnostics:?}"
    );
    assert!(
        !has_ts2345,
        "Did not expect outer TS2345 once property-level elaboration applies, got: {diagnostics:?}"
    );
}

#[test]
fn test_generic_callback_return_mismatch_reports_ts2345_for_identifier_expression_body() {
    // For contextually-typed expression-bodied arrow functions with identifier bodies
    // (like `undefined`), tsc elaborates the return type mismatch and reports TS2322
    // on the body expression rather than TS2345 on the whole callback argument.
    // This matches tsc behavior for contextual callbacks (no explicit param annotations).
    let source = r#"
function someGenerics3<T>(producer: () => T) { }
someGenerics3<number>(() => undefined);
"#;

    let diagnostics = get_all_diagnostics(source);
    let has_ts2322 = has_diagnostic_code(
        &diagnostics,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
    );

    assert!(
        has_ts2322,
        "Expected TS2322 on the body expression for contextual callback, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts2322_check_js_true_does_not_relabel_with_unrelated_diagnostics() {
    let source = r#"
        // @ts-check
        /** @template T */
        /** @returns {{ value: T }} */
        function wrap(value) {
            return { value };
        }
        /** @type {number} */
        const n = wrap("string");
    "#;

    let diagnostics = compile_with_options(
        source,
        "test.js",
        CheckerOptions {
            check_js: true,
            strict: false,
            ..Default::default()
        },
    );
    let has_2322 = has_diagnostic_code(
        &diagnostics,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
    );
    assert!(
        has_2322,
        "Expected TS2322 for generic helper return mismatched with number annotation in JS, got: {diagnostics:?}"
    );
}
