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
    let has_2322 = diagnostics
        .iter()
        .any(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE);
    assert!(
        has_2322,
        "Expected TS2322 for generic helper return mismatched with number annotation in JS, got: {diagnostics:?}"
    );
}

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

    let has_2322 = diagnostics
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .count();

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

    let has_2322 = diagnostics
        .iter()
        .any(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE);
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

    assert!(
        ts2345.related_information.iter().any(|info| {
            info.message_text
                .contains("Return type 'string' is not assignable to 'number'.")
        }),
        "Expected TS2345 to include return-type elaboration, got: {ts2345:?}"
    );
    assert!(
        ts2345.related_information.iter().any(|info| {
            info.code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE
                && info
                    .message_text
                    .contains("Type 'string' is not assignable to type 'number'.")
        }),
        "Expected TS2345 to include nested type mismatch under the return-type detail, got: {ts2345:?}"
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

