#[test]
fn test_ts2345_missing_index_signature_includes_related_detail() {
    let source = r#"
        declare function takes(value: { [index: number]: number }): void;
        interface Arg { one: number; two?: string; }
        const arg: Arg = { one: 1 };
        takes(arg);
    "#;

    let diagnostics = diagnostics_for_source(source);
    let ts2345 = diagnostics
        .iter()
        .find(|diag| {
            diag.code == diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE
        })
        .expect("expected TS2345 for missing-index-signature mismatch");

    assert!(
        ts2345.related_information.iter().any(|info| {
            info.code == diagnostic_codes::INDEX_SIGNATURE_FOR_TYPE_IS_MISSING_IN_TYPE
                && info
                    .message_text
                    .contains("Index signature for type 'number' is missing in type 'Arg'.")
        }),
        "Expected TS2345 to include missing-index-signature elaboration, got: {ts2345:?}"
    );
}

#[test]
fn test_ts2345_array_element_mismatch_includes_related_detail() {
    let source = r#"
        declare function takes(value: number[]): void;
        const arg: string[] = [""];
        takes(arg);
    "#;

    let diagnostics = diagnostics_for_source(source);
    let ts2345 = diagnostics
        .iter()
        .find(|diag| {
            diag.code == diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE
        })
        .expect("expected TS2345 for array-element mismatch");

    assert!(
        ts2345.related_information.iter().any(|info| {
            info.message_text
                .contains("Array element type 'string' is not assignable to 'number'.")
        }),
        "Expected TS2345 to include array-element elaboration, got: {ts2345:?}"
    );
    assert!(
        ts2345.related_information.iter().any(|info| {
            info.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                && info
                    .message_text
                    .contains("Type 'string' is not assignable to type 'number'.")
        }),
        "Expected TS2345 to include nested type mismatch under array-element elaboration, got: {ts2345:?}"
    );
}

#[test]
fn test_ts2322_no_error_for_any_to_number_assignment() {
    let source = r"
        let inferredAny: any;
        let x: number = inferredAny;
    ";

    assert!(
        !has_error_with_code(source, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected no TS2322 when assigning `any` to `number`, got diagnostics: {:?}",
        get_all_diagnostics(source)
    );
}

#[test]
fn test_ts2322_check_js_true_reports_annotation_union_mismatch() {
    let source = r"
        // @ts-check
        /** @type {number | string} */
        const value = { };
    ";

    let diagnostics = compile_with_options(
        source,
        "test.js",
        CheckerOptions {
            check_js: true,
            strict: true,
            ..Default::default()
        },
    );
    let has_2322 = diagnostics
        .iter()
        .any(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE);
    assert!(
        has_2322,
        "Expected TS2322 when assigning `{{}}` to `number | string` in JS mode, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts2322_check_js_false_does_not_enforce_nested_annotation_types() {
    // No @ts-check: nested JSDoc @type should NOT be enforced when checkJs is false.
    let source = r#"
        /** @type {{ a: number, b: string }} */
        const value = { a: "x", b: 1 };
    "#;

    let diagnostics = compile_with_options(
        source,
        "test.js",
        CheckerOptions {
            check_js: false,
            ..Default::default()
        },
    );
    assert!(
        !diagnostics
            .iter()
            .any(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected TS2322 to be suppressed when checkJs is false, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts2322_check_jsx_true_reports_javascript_annotation_mismatch() {
    let source = r#"
        /** @type {number} */
        const value = "bad";
    "#;

    let diagnostics = compile_with_options(
        source,
        "test.jsx",
        CheckerOptions {
            check_js: true,
            ..Default::default()
        },
    );
    assert!(
        diagnostics
            .iter()
            .any(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected TS2322 for .jsx JSDoc mismatch when checkJs is enabled, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts2322_check_jsx_false_does_not_enforce_annotation_type() {
    let source = r#"
        /** @type {number} */
        const value = "bad";
    "#;

    let diagnostics = compile_with_options(
        source,
        "test.jsx",
        CheckerOptions {
            check_js: false,
            ..Default::default()
        },
    );
    assert!(
        !diagnostics
            .iter()
            .any(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected no TS2322 for .jsx when checkJs is disabled, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts2322_check_jsx_strict_nullability_effect() {
    let source = r"
        // @ts-check
        /** @type {number} */
        const maybeNumber = null;
    ";

    let loose = compile_with_options(
        source,
        "test.jsx",
        CheckerOptions {
            check_js: true,
            strict: false,
            strict_null_checks: false,
            ..Default::default()
        },
    );
    let strict = compile_with_options(
        source,
        "test.jsx",
        CheckerOptions {
            check_js: true,
            strict: true,
            ..Default::default()
        },
    );

    let strict_has_2322 = strict
        .iter()
        .any(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE);
    assert!(
        strict_has_2322,
        "Expected strict+checkJs to emit TS2322 for .jsx nullability mismatch, got: {strict:?}"
    );
    assert!(
        strict.len() > loose.len(),
        "Expected strict mode to increase diagnostics for .jsx nullability in checkJs source"
    );
}

#[test]
fn test_ts2322_assignable_through_generic_identity_in_jsdoc_mode_jsx() {
    let source = r#"
        // @ts-check
        /** @returns {number} */
        function id(value) {
            return "string";
        }
    "#;

    let diagnostics = compile_with_options(
        source,
        "test.jsx",
        CheckerOptions {
            check_js: true,
            ..Default::default()
        },
    );
    assert!(
        diagnostics
            .iter()
            .any(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected TS2322 for 'string' not assignable to 'number' in @returns (.jsx), got: {diagnostics:?}"
    );
}

#[test]
fn test_ts2322_assignable_through_generic_identity_in_jsdoc_mode() {
    // In @ts-check JS files, @returns {number} annotations ARE checked by tsc.
    // Returning "string" from a @returns {number} function should emit TS2322.
    let source = r#"
        // @ts-check
        /** @returns {number} */
        function id(value) {
            return "string";
        }
    "#;

    let diagnostics = compile_with_options(
        source,
        "test.js",
        CheckerOptions {
            check_js: true,
            ..Default::default()
        },
    );
    assert!(
        diagnostics
            .iter()
            .any(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected TS2322 for 'string' not assignable to 'number' in @returns, got: {diagnostics:?}"
    );
}

