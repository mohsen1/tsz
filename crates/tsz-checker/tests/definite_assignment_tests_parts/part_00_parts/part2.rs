#[test]
fn test_ts2564_error_for_constrained_type_parameter_excluding_undefined() {
    // T extends string → undefined NOT assignable to string → TS2564 required
    let source = r"
        class C<T extends string> {
            foo: T;
        }
    ";
    let diags = diagnostics_with_options(
        source,
        CheckerOptions {
            strict_null_checks: true,
            strict_property_initialization: true,
            ..CheckerOptions::default()
        },
    );
    assert_eq!(
        count_code(
            &diags,
            diagnostic_codes::PROPERTY_HAS_NO_INITIALIZER_AND_IS_NOT_DEFINITELY_ASSIGNED_IN_THE_CONSTRUCTOR,
        ),
        1,
        "TS2564 should be emitted for property typed as T extends string, got: {diags:?}"
    );
}

#[test]
fn test_ts2564_no_error_for_type_parameter_constrained_to_undefined() {
    // tsc still requires initialization for T extends string | undefined.
    let source = r"
        class C<T extends string | undefined> {
            foo: T;
        }
    ";
    let diags = diagnostics_with_options(
        source,
        CheckerOptions {
            strict_null_checks: true,
            strict_property_initialization: true,
            ..CheckerOptions::default()
        },
    );
    assert_eq!(
        count_code(
            &diags,
            diagnostic_codes::PROPERTY_HAS_NO_INITIALIZER_AND_IS_NOT_DEFINITELY_ASSIGNED_IN_THE_CONSTRUCTOR,
        ),
        1,
        "TS2564 should be emitted for T extends string | undefined, got: {diags:?}"
    );
}

#[test]
fn test_ts2564_still_emitted_for_concrete_types() {
    // Ensure the fix doesn't break concrete type TS2564
    let source = r"
        class C {
            x: number;
            y: string;
        }
    ";
    let diags = diagnostics_with_options(
        source,
        CheckerOptions {
            strict_null_checks: true,
            strict_property_initialization: true,
            ..CheckerOptions::default()
        },
    );
    assert_eq!(
        count_code(
            &diags,
            diagnostic_codes::PROPERTY_HAS_NO_INITIALIZER_AND_IS_NOT_DEFINITELY_ASSIGNED_IN_THE_CONSTRUCTOR,
        ),
        2,
        "TS2564 should still be emitted for concrete types number and string, got: {diags:?}"
    );
}

#[test]
fn test_ts2564_no_error_without_strict_mode() {
    // Mirrors superWithTypeArgument3.ts: no @strict → no TS2564
    let source = r"
        class C<T> {
            foo: T;
            bar(): void {}
        }
        class D<T> extends C<T> {
            constructor() {
                super();
            }
        }
    ";
    let diags = diagnostics_with_options(
        source,
        CheckerOptions {
            strict_null_checks: false,
            strict_property_initialization: false,
            ..CheckerOptions::default()
        },
    );
    assert_eq!(
        count_code(
            &diags,
            diagnostic_codes::PROPERTY_HAS_NO_INITIALIZER_AND_IS_NOT_DEFINITELY_ASSIGNED_IN_THE_CONSTRUCTOR,
        ),
        0,
        "TS2564 should not be emitted when strict mode is off, got: {diags:?}"
    );
}

#[test]
fn test_ts2564_no_false_positive_generic_class_with_base() {
    // Matches current superWithTypeArgument3-style behavior: the base class field
    // still requires initialization even when it is typed as unconstrained T.
    let source = r"
        class C<T> {
            foo: T;
            bar<U>(x: U) { }
        }

        class D<T> extends C<T> {
            constructor() {
                super();
            }
            bar() {
                super.bar(null);
            }
        }
    ";
    let diags = diagnostics_with_options(
        source,
        CheckerOptions {
            strict_null_checks: true,
            strict_property_initialization: true,
            ..CheckerOptions::default()
        },
    );
    assert_eq!(
        count_code(
            &diags,
            diagnostic_codes::PROPERTY_HAS_NO_INITIALIZER_AND_IS_NOT_DEFINITELY_ASSIGNED_IN_THE_CONSTRUCTOR,
        ),
        1,
        "TS2564 should fire once for the generic base-class field in the superWithTypeArgument3-style case, got: {diags:?}"
    );
}

#[test]
fn test_super_with_type_argument3_no_ts2564_without_strict_property_initialization() {
    let source = r"
        class C<T> {
            foo: T;
            bar<U>(x: U) { }
        }

        class D<T> extends C<T> {
            constructor() {
                super<T>();
            }
            bar() {
                super.bar<T>(null);
            }
        }
    ";
    let diags = diagnostics_with_options(
        source,
        CheckerOptions {
            strict: false,
            no_implicit_any: false,
            strict_null_checks: false,
            strict_property_initialization: false,
            no_implicit_this: false,
            strict_function_types: false,
            use_unknown_in_catch_variables: false,
            strict_bind_call_apply: false,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );
    assert_eq!(
        count_code(
            &diags,
            diagnostic_codes::PROPERTY_HAS_NO_INITIALIZER_AND_IS_NOT_DEFINITELY_ASSIGNED_IN_THE_CONSTRUCTOR,
        ),
        0,
        "TS2564 should stay off when strict property initialization is disabled, got: {diags:?}"
    );
}
