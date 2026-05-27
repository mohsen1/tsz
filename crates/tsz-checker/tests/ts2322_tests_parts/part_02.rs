#[test]
fn test_ts2322_arrow_expression_body_jsdoc_cast_reports_template_return_mismatch() {
    let source = r#"
        /** @template T
         * @param {T|undefined} value value or not
         * @returns {T} result value
         */
        const foo1 = value => /** @type {string} */({ ...value });

        /** @template T
         * @param {T|undefined} value value or not
         * @returns {T} result value
         */
        const foo2 = value => /** @type {string} */(/** @type {T} */({ ...value }));
    "#;

    let diagnostics = compile_with_options(
        source,
        "mytest.js",
        CheckerOptions {
            check_js: true,
            strict: true,
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    let has_2322 = diagnostic_count(
        &diagnostics,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
    );

    assert_eq!(
        has_2322, 2,
        "Expected two TS2322 errors from both inline cast arrow bodies, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts2322_namespace_export_assignment_optional_to_required() {
    let source = r#"
        // @target: es2015
        namespace __test1__ {
            export interface interfaceWithPublicAndOptional<T,U> { one: T; two?: U; };  var obj4: interfaceWithPublicAndOptional<number,string> = { one: 1 };;
            export var __val__obj4 = obj4;
        }
        namespace __test2__ {
            export var obj = {two: 1};
            export var __val__obj = obj;
        }
        __test2__.__val__obj = __test1__.__val__obj4
    "#;

    let diagnostics = compile_with_options(
        source,
        "test.ts",
        CheckerOptions {
            target: ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    let has_2322 = has_diagnostic_code(
        &diagnostics,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
    );
    assert!(
        has_2322,
        "Expected TS2322 for assigning optional property type to required property target, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts2322_optional_property_required_includes_related_missing_property_detail() {
    let source = r#"
        let source: { one?: number } = {};
        let target: { one: number } = source;
    "#;

    let diagnostics = diagnostics_for_source(source);
    let ts2322 = diagnostics
        .iter()
        .find(|diag| diag.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .expect("expected TS2322 for optional-to-required property assignment");

    assert!(
        ts2322.related_information.iter().any(|info| {
            info.code == diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE
                && info
                    .message_text
                    .contains("Property 'one' is missing in type")
        }),
        "Expected TS2322 to include missing-property elaboration as related information, got: {ts2322:?}"
    );
}

#[test]
fn test_ts2322_property_type_mismatch_includes_related_property_detail() {
    let source = r#"
        let source: { one: string } = { one: "" };
        let target: { one: number } = source;
    "#;

    let diagnostics = diagnostics_for_source(source);
    let ts2322 = diagnostics
        .iter()
        .find(|diag| diag.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .expect("expected TS2322 for property type mismatch assignment");

    assert!(
        ts2322.related_information.iter().any(|info| {
            info.message_text
                .contains("Types of property 'one' are incompatible.")
        }),
        "Expected TS2322 to include property incompatibility elaboration, got: {ts2322:?}"
    );
}

#[test]
fn test_ts2345_property_type_mismatch_includes_related_property_detail() {
    let source = r#"
        declare function takes(value: { one: number }): void;
        const arg: { one: string } = { one: "" };
        takes(arg);
    "#;

    let diagnostics = diagnostics_for_source(source);
    let ts2345 = diagnostics
        .iter()
        .find(|diag| {
            diag.code == diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE
        })
        .expect("expected TS2345 for argument property type mismatch");

    assert!(
        ts2345.related_information.iter().any(|info| {
            info.code == diagnostic_codes::TYPES_OF_PROPERTY_ARE_INCOMPATIBLE
                && info
                    .message_text
                    .contains("Types of property 'one' are incompatible.")
        }),
        "Expected TS2345 to include property incompatibility elaboration, got: {ts2345:?}"
    );
    assert!(
        ts2345.related_information.iter().any(|info| {
            info.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                && info
                    .message_text
                    .contains("Type 'string' is not assignable to type 'number'.")
        }),
        "Expected TS2345 to include nested type mismatch elaboration, got: {ts2345:?}"
    );
}

#[test]
fn test_ts2345_missing_many_properties_formats_related_detail_once() {
    let source = r#"
        declare function takes(value: { a: number; b: number; c: number; d: number; e: number }): void;
        const arg = {};
        takes(arg);
    "#;

    let diagnostics = diagnostics_for_source(source);
    let ts2345 = diagnostics
        .iter()
        .find(|diag| {
            diag.code == diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE
        })
        .expect("expected TS2345 for missing-properties argument mismatch");

    let related = ts2345
        .related_information
        .iter()
        .find(|info| {
            info.code
                == diagnostic_codes::TYPE_IS_MISSING_THE_FOLLOWING_PROPERTIES_FROM_TYPE_AND_MORE
        })
        .expect("expected TS2740 related detail under TS2345");

    assert!(
        related.message_text.contains("a, b, c, d, and 1 more."),
        "Expected TS2345 related detail to format the extra-property suffix once, got: {related:?}"
    );
    assert!(
        !related.message_text.contains("and 1 more., and 1 more."),
        "Expected TS2345 related detail to avoid duplicating the extra-property suffix, got: {related:?}"
    );
}

#[test]
fn test_ts2345_optional_property_required_includes_related_missing_property_detail() {
    let source = r#"
        declare function takes(value: { one: number }): void;
        const arg: { one?: number } = {};
        takes(arg);
    "#;

    let diagnostics = diagnostics_for_source(source);
    let ts2345 = diagnostics
        .iter()
        .find(|diag| {
            diag.code == diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE
        })
        .expect("expected TS2345 for optional-to-required argument mismatch");

    assert!(
        ts2345.related_information.iter().any(|info| {
            info.code == diagnostic_codes::PROPERTY_IS_MISSING_IN_TYPE_BUT_REQUIRED_IN_TYPE
                && info
                    .message_text
                    .contains("Property 'one' is missing in type")
        }),
        "Expected TS2345 to include missing-property elaboration for optional-to-required mismatch, got: {ts2345:?}"
    );
}

#[test]
fn test_ts2345_function_return_mismatch_includes_related_return_detail() {
    let source = r#"
        declare function takes(cb: () => number): void;
        const cb: () => string = () => "";
        takes(cb);
    "#;

    let diagnostics = diagnostics_for_source(source);
    let ts2345 = diagnostics
        .iter()
        .find(|diag| {
            diag.code == diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE
        })
        .expect("expected TS2345 for function return type mismatch");

    // tsc emits the inner mismatch as a single "Type 'X' is not assignable
    // to type 'Y'." line — no intermediate "Return type ..." prefix
    // (verified: zero matches across all tsc baselines). The TS2345 path
    // formerly emitted both lines; the fingerprint-policy fix collapses to
    // just the direct mismatch line, matching tsc.
    assert!(
        ts2345.related_information.iter().any(|info| {
            info.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                && info
                    .message_text
                    .contains("Type 'string' is not assignable to type 'number'.")
        }),
        "Expected TS2345 to include direct inner type mismatch line, got: {ts2345:?}"
    );
    assert!(
        ts2345.related_information.iter().all(|info| {
            !info
                .message_text
                .contains("Return type 'string' is not assignable to 'number'.")
        }),
        "Should NOT emit \"Return type ...\" framing — tsc omits it: {ts2345:?}"
    );
}

#[test]
fn ts2322_function_return_mismatch_does_not_double_elaborate_with_outer_source() {
    // Regression: render_return_type_mismatch was emitting two related
    // information lines for the same gap:
    //
    //   1. "Return type 'Object' is not assignable to 'string'." (the fallback
    //      label from the depth=0 branch)
    //   2. "Type '(x: Object) => Object' is not assignable to type 'string'."
    //      from the recursive nested render — and the source side was
    //      WRONGLY rendered as the OUTER function type because
    //      `format_nested_assignment_source_type_for_diagnostic` re-derived
    //      the source from the anchor's expression (which is the outer
    //      assignment value), ignoring the passed nested `source` (the
    //      inner return type).
    //
    // tsc emits a single nested line:
    //   "Type 'Object' is not assignable to type 'string'."
    //
    // Two assertions:
    //  - The bogus "Type '(x: Object) => Object' is not assignable to type
    //    'string'." line is NOT emitted (anchor-derived re-render fix).
    //  - The "Return type ..." fallback line is NOT emitted when the nested
    //    reason already carries the inner mismatch (avoids double elaboration).
    let source = r#"
        declare let f1: (x: Object) => string;
        declare let f3: (x: Object) => Object;
        f1 = f3;
    "#;

    let diagnostics = diagnostics_for_source(source);
    let ts2322 = diagnostics
        .iter()
        .find(|diag| diag.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .expect("expected TS2322 for function return type mismatch");

    let related_messages: Vec<&str> = ts2322
        .related_information
        .iter()
        .map(|info| info.message_text.as_str())
        .collect();

    let bogus_outer_assignment = related_messages.iter().any(|msg| {
        msg.contains("'(x: Object) => Object'") && msg.contains("not assignable to type 'string'")
    });
    assert!(
        !bogus_outer_assignment,
        "Should not claim the outer function type is not assignable to the inner return type, got: {related_messages:?}"
    );

    let return_type_label = related_messages
        .iter()
        .any(|msg| msg.contains("Return type 'Object' is not assignable to 'string'."));
    let direct_inner_mismatch = related_messages
        .iter()
        .any(|msg| msg.contains("Type 'Object' is not assignable to type 'string'."));
    assert!(
        direct_inner_mismatch,
        "Expected the direct inner mismatch line, got: {related_messages:?}"
    );
    assert!(
        !return_type_label,
        "Should not double-elaborate with both 'Return type ...' and the nested type mismatch, got: {related_messages:?}"
    );
}

#[test]
fn ts2322_function_return_mismatch_param_name_independent() {
    // Same rule as the test above, but with different binding names —
    // locks the rule as structural per the anti-hardcoding directive
    // in CLAUDE.md §25.
    let source = r#"
        declare let alpha: (input: Object) => number;
        declare let beta: (input: Object) => Object;
        alpha = beta;
    "#;

    let diagnostics = diagnostics_for_source(source);
    let ts2322 = diagnostics
        .iter()
        .find(|diag| diag.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .expect("expected TS2322 for function return type mismatch");

    let related_messages: Vec<&str> = ts2322
        .related_information
        .iter()
        .map(|info| info.message_text.as_str())
        .collect();

    assert!(
        !related_messages.iter().any(|msg| {
            msg.contains("'(input: Object) => Object'")
                && msg.contains("not assignable to type 'number'")
        }),
        "Outer function source must not be re-rendered against inner return target, got: {related_messages:?}"
    );
    assert!(
        related_messages
            .iter()
            .any(|msg| msg.contains("Type 'Object' is not assignable to type 'number'.")),
        "Expected the direct inner mismatch line, got: {related_messages:?}"
    );
}

#[test]
fn test_ts2345_function_return_mismatch_related_detail_qualifies_same_named_returns() {
    let source = r#"
        declare namespace N { export interface Token { kind: "n"; } }
        declare namespace M { export interface Token { kind: "m"; } }
        declare function takes(cb: () => M.Token): void;
        declare const cb: () => N.Token;
        takes(cb);
    "#;

    let diagnostics = diagnostics_for_source(source);
    let ts2345 = diagnostics
        .iter()
        .find(|diag| {
            diag.code == diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE
        })
        .expect("expected TS2345 for function return type mismatch");

    // tsc qualifies same-named types in the inner mismatch line itself
    // ("Type 'N.Token' is not assignable to type 'M.Token'.") rather than
    // through a separate "Return type ..." framing. The qualification still
    // surfaces — just on the direct mismatch line.
    assert!(
        ts2345.related_information.iter().any(|info| {
            info.message_text
                .contains("Type 'N.Token' is not assignable to type 'M.Token'.")
        }),
        "Expected TS2345 inner mismatch to qualify same-named return types, got: {ts2345:?}"
    );
}

#[test]
fn test_ts2345_index_signature_mismatch_includes_related_detail() {
    let source = r#"
        declare function takes(value: { [key: string]: number }): void;
        const arg: { [key: string]: string } = { a: "" };
        takes(arg);
    "#;

    let diagnostics = diagnostics_for_source(source);
    let ts2345 = diagnostics
        .iter()
        .find(|diag| {
            diag.code == diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE
        })
        .expect("expected TS2345 for index-signature mismatch");

    assert!(
        ts2345.related_information.iter().any(|info| {
            info.message_text.contains(
                "string index signature is incompatible: 'string' is not assignable to 'number'.",
            )
        }),
        "Expected TS2345 to include index-signature elaboration, got: {ts2345:?}"
    );
    assert!(
        ts2345.related_information.iter().any(|info| {
            info.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                && info
                    .message_text
                    .contains("Type 'string' is not assignable to type 'number'.")
        }),
        "Expected TS2345 to include nested type mismatch under index-signature elaboration, got: {ts2345:?}"
    );
}

#[test]
fn test_ts2345_index_signature_mismatch_related_detail_qualifies_same_named_values() {
    let source = r#"
        declare namespace N { export interface Token { kind: "n"; } }
        declare namespace M { export interface Token { kind: "m"; } }
        declare function takes(value: { [key: string]: M.Token }): void;
        declare const arg: { [key: string]: N.Token };
        takes(arg);
    "#;

    let diagnostics = diagnostics_for_source(source);
    let ts2345 = diagnostics
        .iter()
        .find(|diag| {
            diag.code == diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE
        })
        .expect("expected TS2345 for index-signature mismatch");

    assert!(
        ts2345.related_information.iter().any(|info| {
            info.message_text.contains(
                "string index signature is incompatible: 'N.Token' is not assignable to 'M.Token'.",
            )
        }),
        "Expected TS2345 related info to qualify same-named index value types, got: {ts2345:?}"
    );
}

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
fn test_ts2345_array_element_mismatch_related_detail_qualifies_same_named_elements() {
    let source = r#"
        declare namespace N { export interface Token { kind: "n"; } }
        declare namespace M { export interface Token { kind: "m"; } }
        declare function takes(value: M.Token[]): void;
        declare const arg: N.Token[];
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
                .contains("Array element type 'N.Token' is not assignable to 'M.Token'.")
        }),
        "Expected TS2345 related info to qualify same-named element types, got: {ts2345:?}"
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
    let has_2322 = has_diagnostic_code(
        &diagnostics,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
    );
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
        !has_diagnostic_code(
            &diagnostics,
            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
        ),
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
        has_diagnostic_code(
            &diagnostics,
            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
        ),
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
        !has_diagnostic_code(
            &diagnostics,
            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
        ),
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

    let strict_has_2322 =
        has_diagnostic_code(&strict, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE);
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
        has_diagnostic_code(
            &diagnostics,
            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
        ),
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
        has_diagnostic_code(
            &diagnostics,
            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
        ),
        "Expected TS2322 for 'string' not assignable to 'number' in @returns, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts2322_assignable_through_generic_identity_in_jsdoc_mode_mjs() {
    let source = r#"
        // @ts-check
        /** @returns {number} */
        function id(value) {
            return "string";
        }
    "#;

    let diagnostics = compile_with_options(
        source,
        "test.mjs",
        CheckerOptions {
            check_js: true,
            ..Default::default()
        },
    );
    assert!(
        has_diagnostic_code(
            &diagnostics,
            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
        ),
        "Expected TS2322 for 'string' not assignable to 'number' in @returns (.mjs), got: {diagnostics:?}"
    );
}

#[test]
fn test_ts2322_for_of_uses_declared_type_for_predeclared_identifier() {
    let source = r"
        let obj: number[];
        let x: string | number | boolean | RegExp;

        function a() {
            x = true;
            for (x of obj) {
                x = x.toExponential();
            }
            x;
        }
    ";

    let diagnostics = get_all_diagnostics(source);
    assert!(
        !has_diagnostic_code(
            &diagnostics,
            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
        ),
        "Expected no TS2322 in for-of assignment flow for predeclared identifier, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts2322_for_of_array_destructuring_assignment_no_false_positive() {
    // for ([k, v] of map) should not produce TS2322 when types match.
    // The iteration element type flows through the destructuring pattern
    // element-by-element, not as a whole-type assignability check.
    let source = r"
        var k: string, v: number;
        var arr: [string, number][] = [['a', 1]];
        for ([k, v] of arr) {
            k;
            v;
        }
    ";

    let diagnostics = get_all_diagnostics(source);
    assert!(
        !has_diagnostic_code(
            &diagnostics,
            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
        ),
        "Expected no TS2322 for array destructuring in for-of with matching types, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts2322_for_of_array_destructuring_wrong_default_still_errors() {
    // for ([k = false] of arr) where k is string should still produce TS2322
    // because the default value `false` is not assignable to `string`.
    let source = r"
        var k: string;
        var arr: [string][] = [['a']];
        for ([k = false] of arr) {
            k;
        }
    ";

    let diagnostics = get_all_diagnostics(source);
    assert!(
        has_diagnostic_code(
            &diagnostics,
            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
        ),
        "Expected TS2322 for wrong default value type in array destructuring for-of"
    );
}

#[test]
fn test_ts2322_object_destructuring_default_not_checked_for_required_property() {
    let source = r#"
        const data = { param: "value" };
        const { param = (() => { throw new Error("param is not defined") })() } = data;
    "#;

    let diagnostics = get_all_diagnostics(source);
    assert!(
        !has_diagnostic_code(
            &diagnostics,
            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
        ),
        "Expected no TS2322 for required-property object destructuring default initializer, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts2322_assignment_destructuring_defaults_report_undefined_mismatches() {
    let source = r#"
        const a: { x?: number; y?: number } = {};
        let x: number;

        ({ x = undefined } = a);
        ({ x: x = undefined } = a);
        ({ y: x = undefined } = a);
    "#;

    let diagnostics = get_all_diagnostics(source);
    let ts2322_messages: Vec<&str> = diagnostics
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .map(|(_, message)| message.as_str())
        .collect();

    // tsc reports the shorthand assignment's optional property read plus each
    // `undefined` default initializer.
    assert_eq!(
        ts2322_messages.len(),
        4,
        "Expected TS2322 for the shorthand property read and each undefined default, got: {diagnostics:?}"
    );
    assert!(
        ts2322_messages
            .iter()
            .any(|message| message.contains("Type 'undefined' is not assignable to type 'number'.")),
        "Expected at least one 'undefined' source display, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts2322_nested_assignment_destructuring_default_is_not_whole_pattern_checked() {
    let source = r#"
        let a: 0 | 1 = 0;
        let b: 0 | 1 | 9;
        [{ [(a = 1)]: b } = [9, a] as const] = [];
        const bb: 0 = b;
    "#;

    let diagnostics = get_all_diagnostics(source);
    assert!(
        !has_diagnostic_code(
            &diagnostics,
            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
        ),
        "Expected no whole-pattern TS2322 for nested assignment destructuring default, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts2322_type_query_in_type_assertion_uses_flow_narrowed_property_type() {
    let source = r#"
        interface I<T> {
            p: T;
        }
        function e(x: I<"A" | "B">) {
            if (x.p === "A") {
                let a: "A" = (null as unknown as typeof x.p);
            }
        }
    "#;

    let diagnostics = get_all_diagnostics(source);
    assert!(
        !has_diagnostic_code(
            &diagnostics,
            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
        ),
        "Expected no TS2322 for flow-narrowed typeof property type in assertion, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts2322_class_or_null_assignable_to_object_or_null() {
    let source = r#"
        class Foo {
            x: string = "";
        }

        declare function getFooOrNull(): Foo | null;

        function f3() {
            let obj: Object | null;
            if ((obj = getFooOrNull()) instanceof Foo) {
                obj;
            }
        }
    "#;

    let diagnostics = get_all_diagnostics(source);
    assert!(
        !has_diagnostic_code(
            &diagnostics,
            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
        ),
        "Expected no TS2322 for `Foo | null` assignment to `Object | null`, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts2322_noimplicitany_nullish_initializer_mutation_is_not_assignability_error() {
    let source = r#"
        declare let cond: boolean;
        function f() {
            let x = undefined;
            if (cond) {
                x = 1;
            }
            if (cond) {
                x = "hello";
            }
        }
    "#;

    let diagnostics = with_lib_contexts(
        source,
        "test.ts",
        CheckerOptions {
            no_implicit_any: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
    );
    assert!(
        !has_diagnostic_code(
            &diagnostics,
            diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
        ),
        "Expected no TS2322 for mutable noImplicitAny variable with undefined initializer, got: {diagnostics:?}"
    );
}

// ── Mapped type key constraint inside conditional types (inferTypes1 parity) ──

#[test]
fn test_ts2322_mapped_type_key_in_conditional_unconstrained_t() {
    // `string extends T ? { [P in T]: void } : T` — T is NOT narrowed in the
    // true branch (check type is `string`, not a type parameter), so T is still
    // unconstrained and `[P in T]` is invalid. tsc emits TS2322 here.
    let source = r"
        type B<T> = string extends T ? { [P in T]: void; } : T;
    ";
    assert!(
        has_error_with_code(source, 2322),
        "Expected TS2322 for unconstrained T in mapped type key inside conditional (string extends T)"
    );
}

#[test]
fn test_ts2322_no_false_positive_mapped_type_key_narrowed_by_conditional() {
    // `T extends string ? { [P in T]: void } : T` — T IS narrowed to `T & string`
    // in the true branch, so `[P in T]` is valid (T is string-like). No TS2322.
    let source = r"
        type A<T> = T extends string ? { [P in T]: void; } : T;
    ";
    let errors = get_all_diagnostics(source);
    assert!(
        !has_diagnostic_code(&errors, 2322),
        "Expected no TS2322 for narrowed T in mapped type key (T extends string). Got: {errors:?}"
    );
}

#[test]
fn test_ts2322_conditional_extends_distinguishes_optional_and_optional_undefined() {
    let source = r#"
        export let a: <T>() => T extends {a?: string} ? 0 : 1 = null!;
        export let b: <T>() => T extends {a?: string | undefined} ? 0 : 1 = a;
    "#;

    let diagnostics = get_all_diagnostics(source);
    let ts2322: Vec<&(u32, String)> = diagnostics_with_code(
        &diagnostics,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
    );

    assert_eq!(
        ts2322.len(),
        1,
        "Expected one TS2322 for conditional extends optional-property identity. Actual diagnostics: {diagnostics:?}"
    );
    assert!(
        ts2322[0].1.contains(
            "is not assignable to type '<T>() => T extends { a?: string | undefined; } ? 0 : 1'"
        ),
        "Expected TS2322 for differing optional-property conditional signatures. Actual diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_ts2322_constructor_default_value_diagnostics_do_not_timeout() {
    let source = r#"
class C {
    constructor(x);
    constructor(public x: string = 1) {
        var y = x;
    }
}

class D<T, U> {
    constructor(x: T, y: U);
    constructor(x: T = 1, public y: U = x) {
        var z = x;
    }
}

class E<T extends Date> {
    constructor(x);
    constructor(x: T = new Date()) {
        var y = x;
    }
}
"#;

    let diagnostics = compile_with_libs_for_ts(
        source,
        "test.ts",
        CheckerOptions {
            strict: false,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    let ts2322: Vec<_> = diagnostics_with_code(
        &diagnostics,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
    );

    assert_eq!(
        ts2322.len(),
        4,
        "Expected four TS2322 diagnostics for constructor parameter defaults, got: {diagnostics:?}"
    );
    assert!(
        ts2322
            .iter()
            .any(|(_, msg)| msg.contains("Type 'number' is not assignable to type 'string'")),
        "Expected string default initializer TS2322, got: {diagnostics:?}"
    );
    assert!(
        ts2322
            .iter()
            .any(|(_, msg)| msg.contains("Type 'number' is not assignable to type 'T'")),
        "Expected generic T default initializer TS2322, got: {diagnostics:?}"
    );
    assert!(
        ts2322
            .iter()
            .any(|(_, msg)| msg.contains("Type 'T' is not assignable to type 'U'")),
        "Expected generic parameter-property TS2322, got: {diagnostics:?}"
    );
    assert!(
        ts2322.iter().any(|(_, msg)| {
            msg.ends_with("is not assignable to type 'T'.")
                && !msg.contains("Type 'number' is not assignable to type 'T'.")
        }),
        "Expected constrained default initializer TS2322 for T, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts2322_new_date_assignment_uses_nominal_date_display() {
    let source = r#"
function foo4<T extends U, U extends V, V extends Date>(t: T, u: U, v: V) {
    t = new Date();
    u = new Date();
    v = new Date();
}
"#;

    let diagnostics = compile_with_libs_for_ts(
        source,
        "test.ts",
        CheckerOptions {
            strict: false,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    let ts2322: Vec<_> = diagnostics_with_code(
        &diagnostics,
        diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE,
    );

    assert_eq!(
        ts2322.len(),
        3,
        "Expected three TS2322 diagnostics for Date-constrained generic assignments, got: {diagnostics:?}"
    );
    assert!(
        ts2322
            .iter()
            .any(|(_, msg)| msg.contains("Type 'Date' is not assignable to type 'T'.")),
        "Expected nominal Date display for T assignment, got: {diagnostics:?}"
    );
    assert!(
        ts2322
            .iter()
            .any(|(_, msg)| msg.contains("Type 'Date' is not assignable to type 'U'.")),
        "Expected nominal Date display for U assignment, got: {diagnostics:?}"
    );
    assert!(
        ts2322
            .iter()
            .any(|(_, msg)| msg.contains("Type 'Date' is not assignable to type 'V'.")),
        "Expected nominal Date display for V assignment, got: {diagnostics:?}"
    );
    assert!(
        ts2322.iter().all(|(_, msg)| !msg.contains("getVarDate")),
        "Did not expect structural Date expansion in TS2322 diagnostics, got: {diagnostics:?}"
    );
}

#[test]
fn indexed_access_on_intersection_preserves_deferred_constraints() {
    // Repro from TypeScript#14723 / conformance test compiler/indexedAccessRelation.ts.
    //
    // Fixed: when evaluating (S & State<T>)["a"] in the mapped type
    // template for Pick<S & State<T>, K>, the solver now preserves deferred
    // IndexAccess types for unconstrained type parameters.
    // This ensures S["a"] is included in the result (S["a"] & (T | undefined)),
    // making T not assignable and TS2322 correctly emitted.
    //
    // tsc keeps (S & State<T>)["a"] as a deferred indexed access type,
    // which correctly rejects T as not assignable to the full expression.
    //
    // Fix requires changes to either:
    // 1. Mapped type evaluation to preserve deferred indexed access for
    //    non-homomorphic mapped types (but Application eval caching
    //    prevents the fix from taking effect), OR
    // 2. The indexed access intersection distribution to include deferred
    //    results (but this causes false positives in homomorphic mapped
    //    types like Readonly<TType & { name: string }>).
    let source = r#"
class Component<S> {
    setState<K extends keyof S>(state: Pick<S, K>) {}
}

export interface State<T> {
    a?: T;
}

class Foo {}

class Comp<T extends Foo, S> extends Component<S & State<T>>
{
    foo(a: T) {
        this.setState({ a: a });
    }
}
"#;
    let diagnostics = get_all_diagnostics(source);
    let ts2322 = diagnostics
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect::<Vec<_>>();
    assert!(
        !ts2322.is_empty(),
        "Expected TS2322 for indexed access on intersection with unconstrained type parameter. Actual diagnostics: {diagnostics:?}"
    );
    assert!(
        ts2322.iter().any(|(_, msg)| msg
            .contains("Type 'T' is not assignable to type '(S & State<T>)[\"a\"] | undefined'.")),
        "Expected top-level TS2322 to preserve the contextual indexed-access target surface, got: {diagnostics:?}"
    );
}

#[test]
fn indexed_access_on_intersection_real_pick_preserves_deferred_constraints() {
    // Same conformance case as above, but using the real library `Pick`.
    // The no-lib reduced test can pass while the lib mapped type loses the
    // deferred `(S & State<T>)["a"]` constraint and incorrectly accepts `T`.
    let source = r#"
class Component<S> {
    setState<K extends keyof S>(state: Pick<S, K>) {}
}

export interface State<T> {
    a?: T;
}

class Foo {}

class Comp<T extends Foo, S> extends Component<S & State<T>>
{
    foo(a: T) {
        this.setState({ a: a });
    }
}
"#;
    let libs = load_lib_files_for_test();
    let diagnostics = check_source_with_libs(
        source,
        "indexedAccessRelation.ts",
        CheckerOptions::default(),
        &libs,
    );
    let ts2322 = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect::<Vec<_>>();
    assert!(
        !ts2322.is_empty(),
        "Expected TS2322 for real Pick indexed access on intersection with unconstrained type parameter. Actual diagnostics: {diagnostics:?}"
    );
}

/// Regression test: arrays should NOT be assignable to interfaces that extend
/// ReadonlyArray/Array but have additional required properties.
///
/// In TypeScript, `TemplateStringsArray` extends `ReadonlyArray<string>` with
/// `readonly raw: readonly string[]`. An empty array `[]` (type `never[]`) lacks
/// the `raw` property, so `var x: TemplateStringsArray = []` should produce TS2322.
///
/// This was previously incorrectly accepted because the array-to-interface subtype
/// shortcut (`check_array_interface_subtype`) checked only `Array<T> <: target`
/// without verifying the target's extra declared properties.
#[test]
fn test_ts2322_array_not_assignable_to_interface_extending_array_with_extra_props() {
    let source = r#"
        interface ArrayWithExtra extends ReadonlyArray<string> {
            readonly raw: readonly string[];
        }
        var x: string[] = [];
        var y: ArrayWithExtra = x;
    "#;

    let diagnostics = diagnostics_for_source(source);
    let assignability_errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| {
            d.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE  // TS2322
                || d.code == 2741  // TS2741: Property 'X' is missing
                || d.code == 2739 // TS2739: Type 'X' is missing properties
        })
        .collect();
    assert!(
        !assignability_errors.is_empty(),
        "Expected TS2322/TS2741/TS2739 when assigning string[] to interface extending ReadonlyArray with extra properties. All diagnostics: {:?}",
        project_diagnostic_codes(&diagnostics)
    );
}

#[test]
fn nested_weak_type_in_intersection_target_emits_ts2322() {
    // When assigning to an intersection target where nested properties are weak types,
    // the weak type check must still apply to the inner property comparison.
    // `in_intersection_member_check` should only suppress weak type checks at the
    // direct intersection member level, not for nested property types.
    // See: nestedExcessPropertyChecking.ts
    let source = r#"
        type A1 = { x: { a?: string } };
        type B1 = { x: { b?: string } };
        type C1 = { x: { c: string } };
        const ab1: A1 & B1 = {} as C1;
    "#;

    let diagnostics = get_all_diagnostics(source);
    let has_ts2322 = has_diagnostic_code(&diagnostics, 2322);
    let has_ts2559 = has_diagnostic_code(&diagnostics, 2559);
    assert!(
        has_ts2322 || has_ts2559,
        "Expected TS2322 or TS2559 for nested weak type mismatch in intersection target. Got: {diagnostics:?}"
    );
}

#[test]
fn flat_weak_type_in_intersection_target_emits_ts2559() {
    // For flat (non-nested) weak types in an intersection, TS2559 should be emitted.
    let source = r#"
        type A2 = { a?: string };
        type B2 = { b?: string };
        type C2 = { c: string };
        const ab2: A2 & B2 = {} as C2;
    "#;

    let diagnostics = get_all_diagnostics(source);
    let has_ts2559 = has_diagnostic_code(&diagnostics, 2559);
    assert!(
        has_ts2559,
        "Expected TS2559 for flat weak type mismatch in intersection target. Got: {diagnostics:?}"
    );
}

#[test]
fn direct_object_literal_excess_prop_against_weak_intersection_emits_ts2353() {
    // tsc emits TS2353 (excess property check) — NOT TS2559 — when a fresh object
    // literal with an unrecognized property is assigned to a weak intersection target.
    // EPC runs first for fresh literals; TS2559 applies to non-fresh sources only.
    let source = r#"
        interface A { x?: number }
        interface B { y?: string }
        const v: A & B = { z: 1 };
    "#;

    let diagnostics = get_all_diagnostics(source);
    let has_ts2353 = has_diagnostic_code(&diagnostics, 2353);
    let has_ts2559 = has_diagnostic_code(&diagnostics, 2559);
    assert!(
        has_ts2353,
        "Expected TS2353 (excess property) for fresh literal with unrecognized property against weak intersection. Got: {diagnostics:?}"
    );
    assert!(
        !has_ts2559,
        "Expected no TS2559 for a fresh object literal (EPC fires first). Got: {diagnostics:?}"
    );
}

#[test]
fn direct_object_literal_prop_matching_second_weak_intersection_member_no_ts2559() {
    // `y` is a property of `Second` and therefore of the intersection `First & Second`.
    // Having any property in common with ANY member is sufficient — no TS2559.
    // This verifies that `violates_weak_type` collects ALL members' properties.
    let source = r#"
        interface First { x?: number }
        interface Second { y?: string }
        const v: First & Second = { y: "hello" };
    "#;

    let diagnostics = get_all_diagnostics(source);
    let has_ts2559 = has_diagnostic_code(&diagnostics, 2559);
    assert!(
        !has_ts2559,
        "Source with property matching second intersection member should NOT emit TS2559. Got: {diagnostics:?}"
    );
}

#[test]
fn direct_object_literal_excess_prop_against_single_weak_type_emits_ts2353() {
    // tsc emits TS2353 (excess property check) when a fresh object literal with an
    // unrecognized property is assigned to a single weak type. EPC fires first for
    // fresh literals; TS2559 applies only to non-fresh sources.
    let source = r#"
        interface Opts { timeout?: number; retries?: number }
        const v: Opts = { url: "x" };
    "#;
    let diagnostics = get_all_diagnostics(source);
    let has_ts2353 = has_diagnostic_code(&diagnostics, 2353);
    let has_ts2559 = has_diagnostic_code(&diagnostics, 2559);
    assert!(
        has_ts2353,
        "Expected TS2353 (excess property) for fresh literal with unrecognized property against single weak type. Got: {diagnostics:?}"
    );
    assert!(
        !has_ts2559,
        "Expected no TS2559 for a fresh object literal (EPC fires first). Got: {diagnostics:?}"
    );
}

#[test]
fn intersection_member_weak_type_suppression_still_works() {
    // When the source has properties that overlap with one intersection member
    // but not with a weak-type member, the assignment should still pass.
    // The weak type suppression during intersection member checking should work
    // at the DIRECT level but not for nested property types.
    let source = r#"
        interface ITreeItem {
            Parent?: ITreeItem;
        }
        interface IDecl {
            Id?: number;
        }
        const x: ITreeItem & IDecl = {} as ITreeItem;
    "#;

    let diagnostics = get_all_diagnostics(source);
    let has_ts2322 = has_diagnostic_code(&diagnostics, 2322);
    let has_ts2559 = has_diagnostic_code(&diagnostics, 2559);
    assert!(
        !has_ts2322 && !has_ts2559,
        "ITreeItem should be assignable to ITreeItem & IDecl without error. Got: {diagnostics:?}"
    );
}

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
    let has_ts2559 = has_diagnostic_code(&diagnostics, 2559);
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
    let has_ts2559 = has_diagnostic_code(&diagnostics, 2559);
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
    let has_ts2559 = has_diagnostic_code(&diagnostics, 2559);
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
    let has_ts2559 = has_diagnostic_code(&diagnostics, 2559);
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
    let has_ts2559 = has_diagnostic_code(&diagnostics, 2559);
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

