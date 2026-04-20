#[test]
fn test_ts2322_no_false_positive_nested_conditional() {
    // Nested conditional expressions should also work
    let source = r#"
        function pick<T, K extends keyof T>(obj: T, key: K): T[K] {
            return obj[key];
        }

        type Point = { x: number; y: number; z: number };

        function test(p: Point, a: boolean, b: boolean) {
            // Nested ternary should produce "x" | "y" | "z"
            let value = pick(p, a ? "x" : (b ? "y" : "z"));
        }
    "#;

    let errors = get_all_diagnostics(source);
    let ts2322_errors: Vec<_> = errors
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();
    assert!(
        ts2322_errors.is_empty(),
        "Expected no TS2322 for nested conditional expression, got: {ts2322_errors:?}"
    );
}

#[test]
fn test_ts2322_generic_indexed_write_preserves_type_parameter_display() {
    let source = r#"
        type Item = { a: string; b: number };

        function setValue<T extends Item, K extends keyof T>(obj: T, key: K) {
            obj[key] = 123;
        }
    "#;

    let ts2322_errors: Vec<_> = get_all_diagnostics(source)
        .into_iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();

    assert!(
        ts2322_errors
            .iter()
            .any(|(_, message)| message.contains("Type 'number' is not assignable to type 'T[K]'")),
        "Expected generic indexed-write TS2322 to preserve T[K] display, got: {ts2322_errors:?}"
    );
}

#[test]
fn test_ts2322_accessor_incompatible_getter_setter() {
    // TS 5.1+: when BOTH getter and setter have explicit type annotations,
    // unrelated types are allowed (no error).
    let source_both_explicit = r#"
        class C {
            get x(): string { return "s"; }
            set x(value: number) {}
        }
    "#;

    let diagnostics = get_all_diagnostics(source_both_explicit);
    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();
    assert!(
        ts2322.is_empty(),
        "TS 5.1+ allows unrelated types when both annotated; got: {ts2322:?}"
    );

    // But when getter has NO explicit return annotation (inferred type),
    // the inferred type must be compatible with the setter's explicit param type.
    let source_inferred_getter = r#"
        class C {
            get bar() { return 0; }
            set bar(n: string) {}
        }
    "#;

    let diagnostics = get_all_diagnostics(source_inferred_getter);
    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();
    assert!(
        !ts2322.is_empty(),
        "Inferred getter type (number) conflicts with explicit setter type (string) → TS2322"
    );
}

#[test]
fn test_ts2322_accessor_compatible_divergent_types() {
    // When getter return IS assignable to setter param, no error.
    let source = r#"
        class C {
            get x(): string { return "hello"; }
            set x(value: string | number) {}
        }
    "#;

    let diagnostics = get_all_diagnostics(source);
    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();

    assert!(
        ts2322.is_empty(),
        "Getter return type (string) is assignable to setter param (string|number), no TS2322; got: {ts2322:?}"
    );
}

#[test]
fn test_ts2322_annotated_getter_contextually_types_unannotated_setter_parameter() {
    let source = r#"
        class C {
            get x(): string { return ""; }
            set x(value) { value = 0; }
        }
    "#;

    let diagnostics = get_all_diagnostics(source);
    let ts2322: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE)
        .collect();
    let ts7006: Vec<_> = diagnostics
        .iter()
        .filter(|(code, _)| *code == diagnostic_codes::PARAMETER_IMPLICITLY_HAS_AN_TYPE)
        .collect();

    assert_eq!(
        ts2322.len(),
        1,
        "expected setter body assignment to be checked against getter type: {diagnostics:?}"
    );
    assert!(
        ts7006.is_empty(),
        "paired getter should contextually type the setter parameter: {diagnostics:?}"
    );
}

#[test]
fn test_ts2322_js_accessor_jsdoc_does_not_force_inferred_getter_mismatch() {
    let source = r#"
        export class Foo {
            /**
             * @type {null | string}
             */
            _bar = null;

            get bar() {
                return this._bar;
            }
            /**
             * @type {string}
             */
            set bar(value) {
                this._bar = value;
            }
        }
    "#;

    let diagnostics = compile_with_options(
        source,
        "test.js",
        CheckerOptions {
            check_js: true,
            allow_js: true,
            strict: true,
            ..CheckerOptions::default()
        },
    );

    assert!(
        !diagnostics
            .iter()
            .any(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected JS accessor JSDoc pair to avoid TS2322 getter/setter mismatch. Actual diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_ts2322_for_of_annotation_mismatch() {
    let source = r"
        for (const x: string of [1, 2, 3]) {}
    ";

    assert!(
        has_error_with_code(source, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        "Expected TS2322 for for-of annotation mismatch"
    );
}

#[test]
fn test_ts2322_check_js_true_reports_javascript_annotation_mismatch() {
    let source = r#"
        // @ts-check
        /** @type {number} */
        const value = "bad";
    "#;

    let diagnostics = compile_with_options(
        source,
        "test.js",
        CheckerOptions {
            check_js: true,
            ..CheckerOptions::default()
        },
    );
    let has_2322 = diagnostics
        .iter()
        .any(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE);
    assert!(
        has_2322,
        "Expected TS2322 when checkJs checks mismatched JS annotation, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts2322_check_mjs_true_reports_javascript_annotation_mismatch() {
    let source = r#"
        /** @type {number} */
        const value = "bad";
    "#;

    let diagnostics = compile_with_options(
        source,
        "test.mjs",
        CheckerOptions {
            check_js: true,
            ..CheckerOptions::default()
        },
    );
    let has_2322 = diagnostics
        .iter()
        .any(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE);
    assert!(
        has_2322,
        "Expected TS2322 for .mjs jsdoc mismatch when checkJs is enabled, got: {diagnostics:?}"
    );
}

#[test]
fn test_ts2322_check_js_false_does_not_enforce_annotation_type() {
    // No @ts-check: JSDoc types should NOT be enforced when checkJs is false.
    let source = r#"
        /** @type {number} */
        const value = "bad";
    "#;

    let diagnostics = compile_with_options(
        source,
        "test.js",
        CheckerOptions {
            check_js: false,
            ..CheckerOptions::default()
        },
    );
    let has_2322 = diagnostics
        .iter()
        .any(|(code, _)| *code == diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE);
    assert!(
        !has_2322,
        "Expected no TS2322 when checkJs is disabled, got: {diagnostics:?}"
    );
}

