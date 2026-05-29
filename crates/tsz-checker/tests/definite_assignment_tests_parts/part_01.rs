#[test]
fn test_super_with_type_argument3_reports_ts2564_under_strict_property_initialization() {
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
            strict: true,
            strict_null_checks: true,
            strict_property_initialization: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );
    assert_eq!(
        count_code(
            &diags,
            diagnostic_codes::PROPERTY_HAS_NO_INITIALIZER_AND_IS_NOT_DEFINITELY_ASSIGNED_IN_THE_CONSTRUCTOR,
        ),
        1,
        "The exact superWithTypeArgument3 shape should still emit TS2564 under strict property initialization, got: {diags:?}"
    );
}

#[test]
fn test_ts2564_multiple_type_parameters_mixed_constraints() {
    // tsc now requires initialization for both constrained and unconstrained type params.
    let source = r"
        class Store<K extends string, V> {
            key: K;
            value: V;
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
    // key: K (extends string) → TS2564
    // value: V (unconstrained) → TS2564 as well
    assert_eq!(
        count_code(
            &diags,
            diagnostic_codes::PROPERTY_HAS_NO_INITIALIZER_AND_IS_NOT_DEFINITELY_ASSIGNED_IN_THE_CONSTRUCTOR,
        ),
        2,
        "Both constrained K and unconstrained V should get TS2564, got: {diags:?}"
    );
}

/// When a declaration has `!` with an initializer but no type annotation,
/// tsc emits only TS1263 (initializer conflicts with `!`), NOT TS1264
/// (needs type annotation). TS1264 is suppressed because TS1263 takes priority.
#[test]
fn test_ts1264_suppressed_when_initializer_present_class_property() {
    let source = r"
        class C {
            a! = 1;
        }
    ";
    let diags = diagnostics_with_options(source, CheckerOptions::default());
    assert_eq!(
        count_code(
            &diags,
            diagnostic_codes::DECLARATIONS_WITH_INITIALIZERS_CANNOT_ALSO_HAVE_DEFINITE_ASSIGNMENT_ASSERTIONS,
        ),
        1,
        "TS1263 should fire for `a! = 1`, got: {diags:?}"
    );
    assert_eq!(
        count_code(
            &diags,
            diagnostic_codes::DECLARATIONS_WITH_DEFINITE_ASSIGNMENT_ASSERTIONS_MUST_ALSO_HAVE_TYPE_ANNOTATIONS,
        ),
        0,
        "TS1264 should be suppressed when initializer is present, got: {diags:?}"
    );
}

#[test]
fn test_ts1264_suppressed_when_initializer_present_variable() {
    let source = r"
        let b! = 1;
    ";
    let diags = diagnostics_with_options(source, CheckerOptions::default());
    assert_eq!(
        count_code(
            &diags,
            diagnostic_codes::DECLARATIONS_WITH_INITIALIZERS_CANNOT_ALSO_HAVE_DEFINITE_ASSIGNMENT_ASSERTIONS,
        ),
        1,
        "TS1263 should fire for `let b! = 1`, got: {diags:?}"
    );
    assert_eq!(
        count_code(
            &diags,
            diagnostic_codes::DECLARATIONS_WITH_DEFINITE_ASSIGNMENT_ASSERTIONS_MUST_ALSO_HAVE_TYPE_ANNOTATIONS,
        ),
        0,
        "TS1264 should be suppressed when initializer is present, got: {diags:?}"
    );
}

#[test]
fn test_ts1264_emitted_when_no_initializer_and_no_type() {
    let source = r"
        class C {
            d!;
        }
    ";
    let diags = diagnostics_with_options(source, CheckerOptions::default());
    assert_eq!(
        count_code(
            &diags,
            diagnostic_codes::DECLARATIONS_WITH_DEFINITE_ASSIGNMENT_ASSERTIONS_MUST_ALSO_HAVE_TYPE_ANNOTATIONS,
        ),
        1,
        "TS1264 should fire for `d!` (no initializer, no type), got: {diags:?}"
    );
    assert_eq!(
        count_code(
            &diags,
            diagnostic_codes::DECLARATIONS_WITH_INITIALIZERS_CANNOT_ALSO_HAVE_DEFINITE_ASSIGNMENT_ASSERTIONS,
        ),
        0,
        "TS1263 should NOT fire (no initializer), got: {diags:?}"
    );
}

/// Regression: when a TDZ-violating use precedes a `const` declaration whose
/// type annotation is a self-referential `typeof` (e.g. `const fn: typeof fn`),
/// tsc emits TS2448 + TS2502 only — it does NOT emit TS2454 as a TDZ companion
/// because the variable's declared type cannot be evaluated. The companion
/// suppression here mirrors that behaviour and is exercised by the
/// `controlFlowFunctionLikeCircular_3.ts` block of the upstream
/// `controlFlowFunctionLikeCircular1` conformance fixture.
#[test]
fn test_ts2454_suppressed_for_tdz_with_self_circular_typeof_annotation() {
    let source = r"
        function test(arg: () => string) {
          fn();
          const fn: typeof fn = arg;
        }
    ";
    let diags = diagnostics_with_options(
        source,
        CheckerOptions {
            strict_null_checks: true,
            ..Default::default()
        },
    );
    assert!(
        diags.iter().any(|(c, _)| *c
            == diagnostic_codes::BLOCK_SCOPED_VARIABLE_USED_BEFORE_ITS_DECLARATION),
        "Expected TS2448 (block-scoped used before declaration), got: {diags:?}"
    );
    assert_eq!(
        count_code(
            &diags,
            diagnostic_codes::VARIABLE_IS_USED_BEFORE_BEING_ASSIGNED
        ),
        0,
        "TS2454 should be suppressed when declared type self-references via typeof, got: {diags:?}"
    );
}

/// Sanity guard: a regular TDZ use of a `const` with no self-circular `typeof`
/// in its annotation must still emit the TS2454 companion alongside TS2448.
/// This confirms the new self-circular suppression is narrowly targeted and
/// does not regress the standard companion path.
#[test]
fn test_ts2454_still_emitted_for_tdz_with_non_circular_annotation() {
    let source = r"
        function test() {
          fn();
          const fn: () => void = () => {};
        }
    ";
    let diags = diagnostics_with_options(
        source,
        CheckerOptions {
            strict_null_checks: true,
            ..Default::default()
        },
    );
    assert!(
        diags.iter().any(|(c, _)| *c
            == diagnostic_codes::BLOCK_SCOPED_VARIABLE_USED_BEFORE_ITS_DECLARATION),
        "Expected TS2448 (block-scoped used before declaration), got: {diags:?}"
    );
    assert_eq!(
        count_code(
            &diags,
            diagnostic_codes::VARIABLE_IS_USED_BEFORE_BEING_ASSIGNED
        ),
        1,
        "TS2454 must still fire for normal TDZ companion, got: {diags:?}"
    );
}

/// Regression for `unionAndIntersectionInference1.ts`. When a property access
/// receiver is a composite expression (e.g. a `CallExpression`) whose
/// sub-expression is an uninitialized variable that fires TS2454, tsc
/// suppresses TS2532 on the outer property access. The cascade is meaningless
/// once the underlying variable has been reported as un-assigned.
///
/// Before the fix, the checker only recognised the receiver as DAA-flagged
/// when the receiver node itself was the failing identifier, or when a
/// TS2454 diagnostic started exactly at the receiver's start position. For
/// `get(foo).toUpperCase()`, the receiver is `get(foo)` which starts before
/// `foo`, so the heuristic missed it and TS2532 leaked through.
#[test]
fn test_ts2532_suppressed_when_ts2454_fires_in_call_receiver() {
    let source = r"
        type Maybe<T> = T | void;
        function get<U>(x: U | void): U {
            return null as any;
        }
        let foo: Maybe<string>;
        get(foo).toUpperCase();
    ";
    let diags = diagnostics_with_options(
        source,
        CheckerOptions {
            strict_null_checks: true,
            ..Default::default()
        },
    );
    assert_eq!(
        count_code(
            &diags,
            diagnostic_codes::VARIABLE_IS_USED_BEFORE_BEING_ASSIGNED
        ),
        1,
        "TS2454 must fire on uninitialized `foo`, got: {diags:?}"
    );
    assert_eq!(
        count_code(&diags, diagnostic_codes::OBJECT_IS_POSSIBLY_UNDEFINED),
        0,
        "TS2532 must be suppressed when TS2454 fires inside the receiver \
         expression of a property access; tsc only reports the underlying \
         use-before-assigned. Got: {diags:?}"
    );
}

fn es_decorator_strict_options() -> CheckerOptions {
    CheckerOptions {
        strict: true,
        target: ScriptTarget::ES2022,
        ..CheckerOptions::default()
    }
}

// A minimal ES stage-3 field decorator used across the tests below.
const DEC_FN: &str =
    "function dec(_t: undefined, _c: ClassFieldDecoratorContext): undefined { return undefined; }";

#[test]
fn test_ts2565_es_decorated_field_used_before_assigned_in_constructor() {
    let source = format!(
        r#"
        {DEC_FN}
        class A {{
            @dec
            x: number;
            constructor() {{
                this.x = this.x;
            }}
        }}
        class B {{
            @dec
            y: number;
            z: number;
            constructor() {{
                let v = this.y;
                this.y = 1;
                this.z = v;
            }}
        }}
    "#
    );

    let ts2565_messages: Vec<_> =
        full_diagnostics_with_options(&source, es_decorator_strict_options())
            .into_iter()
            .filter(|d| d.code == diagnostic_codes::PROPERTY_IS_USED_BEFORE_BEING_ASSIGNED)
            .map(|d| d.message_text)
            .collect();

    assert!(
        ts2565_messages
            .iter()
            .any(|m| m.contains("'x' is used before being assigned")),
        "Expected TS2565 for decorated field 'x', got: {ts2565_messages:?}"
    );
    assert!(
        ts2565_messages
            .iter()
            .any(|m| m.contains("'y' is used before being assigned")),
        "Expected TS2565 for decorated field 'y', got: {ts2565_messages:?}"
    );
}

#[test]
fn test_ts2565_es_decorated_field_with_initializer_no_error() {
    let source = format!(
        r#"
        {DEC_FN}
        class C {{
            @dec
            x: number = 0;
            constructor() {{
                let v = this.x;
                this.x = v + 1;
            }}
        }}
    "#
    );

    let ts2565_count = full_diagnostics_with_options(&source, es_decorator_strict_options())
        .iter()
        .filter(|d| d.code == diagnostic_codes::PROPERTY_IS_USED_BEFORE_BEING_ASSIGNED)
        .count();

    assert_eq!(
        ts2565_count, 0,
        "Decorated field with initializer must not produce TS2565"
    );
}

#[test]
fn test_ts2564_es_decorated_field_no_initializer_no_ts2564() {
    let source = format!(
        r#"
        {DEC_FN}
        class D {{
            @dec
            x: number;
            constructor() {{
                this.x = 42;
            }}
        }}
    "#
    );

    let ts2564_count = diagnostics_with_options(&source, es_decorator_strict_options())
        .iter()
        .filter(|d| d.0 == diagnostic_codes::PROPERTY_HAS_NO_INITIALIZER_AND_IS_NOT_DEFINITELY_ASSIGNED_IN_THE_CONSTRUCTOR)
        .count();

    assert_eq!(
        ts2564_count, 0,
        "ES-decorated field must not produce TS2564 — decorator may supply initialization"
    );
}

#[test]
fn test_ts2564_computed_unique_symbol_property_no_initializer() {
    let source = r#"
declare const s: unique symbol;
declare namespace N {
    export const s: unique symbol;
}
class C {
    [s]: number;
    [N.s]: string;
}
"#;
    let diags = diagnostics_with_options(
        source,
        CheckerOptions {
            strict_null_checks: true,
            strict_property_initialization: true,
            ..CheckerOptions::default()
        },
    );
    let codes: Vec<_> = diags.iter().map(|(code, _)| *code).collect();
    assert_eq!(
        codes,
        vec![
            diagnostic_codes::PROPERTY_HAS_NO_INITIALIZER_AND_IS_NOT_DEFINITELY_ASSIGNED_IN_THE_CONSTRUCTOR,
            diagnostic_codes::PROPERTY_HAS_NO_INITIALIZER_AND_IS_NOT_DEFINITELY_ASSIGNED_IN_THE_CONSTRUCTOR,
        ],
        "Expected exactly TS2564 for [s] and [N.s] computed unique symbol properties, got: {diags:?}"
    );
}

#[test]
fn test_ts2564_computed_unique_symbol_property_suppressed_by_constructor_assign() {
    let source = r#"
declare const s: unique symbol;
declare namespace N {
    export const s: unique symbol;
}
class C {
    [s]: number;
    [N.s]: string;
    constructor() {
        this[s] = 42;
        this[N.s] = "hello";
    }
}
"#;
    let diags = diagnostics_with_options(
        source,
        CheckerOptions {
            strict_null_checks: true,
            strict_property_initialization: true,
            ..CheckerOptions::default()
        },
    );
    let codes: Vec<_> = diags.iter().map(|(code, _)| *code).collect();
    assert_eq!(
        codes,
        Vec::<u32>::new(),
        "Expected no diagnostics when constructor assigns [s] and [N.s], got: {diags:?}"
    );
}
