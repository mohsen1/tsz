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
        diagnostics
            .iter()
            .any(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
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
        !diagnostics
            .iter()
            .any(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
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
        !diagnostics
            .iter()
            .any(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
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
        diagnostics
            .iter()
            .any(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
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
        !diagnostics
            .iter()
            .any(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
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

    // TSC emits TWO TS2322 errors for shorthand defaults (combined type + default alone)
    // and ONE per non-shorthand default (default alone only — tsc does NOT check the full
    // source property type for non-shorthand). With 1 shorthand and 2 non-shorthand
    // assignments, we expect 2 + 1 + 1 = 4 errors.
    assert_eq!(
        ts2322_messages.len(),
        4,
        "Expected TS2322 for each undefined default in assignment destructuring (shorthand gets both combined and default-alone; non-shorthand gets default-alone only), got: {diagnostics:?}"
    );
    assert!(
        ts2322_messages
            .iter()
            .any(|message| message.contains("Type 'undefined' is not assignable to type 'number'.")),
        "Expected at least one 'undefined' source display, got: {diagnostics:?}"
    );
    assert!(
        ts2322_messages.iter().any(|message| message
            .contains("Type 'number | undefined' is not assignable to type 'number'.")),
        "Expected at least one 'number | undefined' source display, got: {diagnostics:?}"
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
        !diagnostics
            .iter()
            .any(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
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
        !diagnostics
            .iter()
            .any(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
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
        !diagnostics
            .iter()
            .any(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
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
        !diagnostics
            .iter()
            .any(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected no TS2322 for mutable noImplicitAny variable with undefined initializer, got: {diagnostics:?}"
    );
}

// ── Mapped type key constraint inside conditional types (inferTypes1 parity) ──

