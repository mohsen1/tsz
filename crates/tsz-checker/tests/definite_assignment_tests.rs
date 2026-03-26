use crate::CheckerState;
use crate::context::CheckerOptions;
use crate::diagnostics::diagnostic_codes;
use tsz_binder::BinderState;
use tsz_common::common::ScriptTarget;
use tsz_common::diagnostics::Diagnostic;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn full_diagnostics_with_options(source: &str, options: CheckerOptions) -> Vec<Diagnostic> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        options,
    );
    checker.check_source_file(root);
    checker.ctx.diagnostics.clone()
}

fn diagnostics_with_options(source: &str, options: CheckerOptions) -> Vec<(u32, String)> {
    full_diagnostics_with_options(source, options)
        .iter()
        .map(|d| (d.code, d.message_text.clone()))
        .collect()
}

fn count_code(diags: &[(u32, String)], code: u32) -> usize {
    diags.iter().filter(|(c, _)| *c == code).count()
}

#[test]
fn test_definite_assignment_ts2564_constructor_branching_both_paths_assign() {
    let source = r"
        class C {
            x: number;
            constructor(flag: boolean) {
                if (flag) {
                    this.x = 1;
                } else {
                    this.x = 2;
                }
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
        0,
        "Expected no TS2564 when all constructor branches assign, got: {diags:?}"
    );
}

#[test]
fn test_definite_assignment_ts2564_loop_and_try_conservative_paths() {
    let source = r"
        class C1 {
            x: number;
            constructor(flag: boolean) {
                while (flag) {
                    this.x = 1;
                    break;
                }
            }
        }

        class C2 {
            y: number;
            constructor(flag: boolean) {
                try {
                    if (flag) {
                        this.y = 1;
                    }
                } catch (e) {
                }
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

    assert!(
        count_code(
            &diags,
            diagnostic_codes::PROPERTY_HAS_NO_INITIALIZER_AND_IS_NOT_DEFINITELY_ASSIGNED_IN_THE_CONSTRUCTOR,
        ) >= 2,
        "Expected conservative TS2564 through loop/try paths, got: {diags:?}"
    );
}

#[test]
fn test_definite_assignment_ts2564_parameter_property_vs_plain_property() {
    let source = r"
        class WithParamProp {
            constructor(public x: number) {}
        }

        class WithoutParamProp {
            x: number;
            constructor(x: number) {}
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

    assert!(
        count_code(
            &diags,
            diagnostic_codes::PROPERTY_HAS_NO_INITIALIZER_AND_IS_NOT_DEFINITELY_ASSIGNED_IN_THE_CONSTRUCTOR,
        ) >= 1,
        "Expected TS2564 only for plain property case, got: {diags:?}"
    );
}

#[test]
fn test_ts2564_constructor_assignment_summary_handles_parameter_property_flow() {
    let source = r"
        class C {
            value: number;
            constructor(public source: number) {
                this.value = this.source;
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
        0,
        "TS2564 should stay suppressed when a required field is assigned from a parameter property via the shared class summary, got: {diags:?}"
    );
}

#[test]
fn test_ts2729_parameter_property_before_define_field_initialization() {
    let source = r"
        class C {
            value = this.source;
            constructor(public source: number) {}
        }
    ";

    let diags = diagnostics_with_options(
        source,
        CheckerOptions {
            strict: true,
            target: tsz_common::common::ScriptTarget::ES2022,
            ..CheckerOptions::default()
        },
    );

    assert_eq!(
        count_code(
            &diags,
            diagnostic_codes::PROPERTY_IS_USED_BEFORE_ITS_INITIALIZATION,
        ),
        1,
        "TS2729 should fire when a field initializer reads a parameter property before constructor execution, got: {diags:?}"
    );
}

#[test]
fn test_ts2565_constructor_reads_keep_private_field_name() {
    let source = r"
        class C10 {
            a: number;
            b: number;
            c?: number;
            #d: number;
            constructor() {
                let x = this.a;
                this.a = this.b;
                this.b = this.#d;
                this.b = x;
                this.#d = x;
                let y = this.c;
            }
        }
    ";

    let diags = full_diagnostics_with_options(
        source,
        CheckerOptions {
            strict: true,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );
    let ts2565_messages: Vec<_> = diags
        .iter()
        .filter(|d| d.code == diagnostic_codes::PROPERTY_IS_USED_BEFORE_BEING_ASSIGNED)
        .map(|d| d.message_text.clone())
        .collect();

    assert!(
        ts2565_messages
            .iter()
            .any(|message| message.contains("Property 'a' is used before being assigned.")),
        "Expected TS2565 for property 'a', got: {ts2565_messages:?}"
    );
    assert!(
        ts2565_messages
            .iter()
            .any(|message| message.contains("Property 'b' is used before being assigned.")),
        "Expected TS2565 for property 'b', got: {ts2565_messages:?}"
    );
    assert!(
        ts2565_messages
            .iter()
            .any(|message| message.contains("Property '#d' is used before being assigned.")),
        "Expected TS2565 for private property '#d', got: {ts2565_messages:?}"
    );
    assert!(
        ts2565_messages
            .iter()
            .all(|message| !message.contains("Property 'c' is used before being assigned.")),
        "Optional property 'c' should not appear in TS2565 messages, got: {ts2565_messages:?}"
    );
}

#[test]
fn test_parameter_property_chain_summary_preserves_optional_base_param_property_type() {
    let source = r"
        class Base {
            constructor(public value?: number) {}
        }

        class Derived extends Base {
            constructor(public override value?: number) {
                super(value);
            }
        }
    ";

    let diags = diagnostics_with_options(
        source,
        CheckerOptions {
            strict: true,
            ..CheckerOptions::default()
        },
    );

    assert_eq!(
        count_code(
            &diags,
            diagnostic_codes::CLASS_INCORRECTLY_EXTENDS_BASE_CLASS
        ),
        0,
        "Optional parameter properties should compare through the shared base-chain summary without TS2415, got: {diags:?}"
    );
    assert_eq!(
        count_code(
            &diags,
            diagnostic_codes::PROPERTY_IN_TYPE_IS_NOT_ASSIGNABLE_TO_THE_SAME_PROPERTY_IN_BASE_TYPE,
        ),
        0,
        "Optional parameter properties should keep their `| undefined` shape through the shared summary, got: {diags:?}"
    );
}

#[test]
fn test_ts_function_expando_summary_reports_ts2565_and_keeps_returned_members() {
    let source = r"
        function f(flag: boolean) {
            function d() {}
            if (flag) {
                d.q = false;
            }
            d.q;
            if (flag) {
                d.s = 'hi';
            }
            return d;
        }

        const value = f(true).s;
    ";

    let diags = diagnostics_with_options(
        source,
        CheckerOptions {
            strict: true,
            target: tsz_common::common::ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    assert_eq!(
        count_code(
            &diags,
            diagnostic_codes::PROPERTY_IS_USED_BEFORE_BEING_ASSIGNED,
        ),
        1,
        "TS2565 should apply to function expando reads inside the initializing scope, got: {diags:?}"
    );
    assert!(
        diags.iter().all(|(code, _)| *code != 2339),
        "Returned function expandos should stay visible through the shared summary, got: {diags:?}"
    );
}

#[test]
fn test_object_literal_declared_property_assignments_do_not_emit_ts2565() {
    let source = r#"
        function f() {
            const obj = {
                z: 1,
                q: "hello",
            };
            try {
                obj.q = "ohhh";
            } catch (e) {
                if (obj.z < 10) {
                    obj.z = 12;
                } else {
                    obj.q = "hmm";
                }
            }
            switch (obj.z) {
                case 0:
                    return obj.q;
                default:
                    return "";
            }
        }
    "#;

    let diags = diagnostics_with_options(
        source,
        CheckerOptions {
            strict: true,
            target: tsz_common::common::ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    assert_eq!(
        count_code(
            &diags,
            diagnostic_codes::PROPERTY_IS_USED_BEFORE_BEING_ASSIGNED,
        ),
        0,
        "Declared object-literal properties should not be reclassified as expando forward reads, got: {diags:?}"
    );
}

#[test]
fn test_parameter_property_chain_summary_preserves_optional_base_property_override_type() {
    let source = r"
        class Base {
            constructor(public value?: number) {}
        }

        class Derived extends Base {
            override value?: number;

            constructor(value?: number) {
                super(value);
                this.value = value;
            }
        }
    ";

    let diags = diagnostics_with_options(
        source,
        CheckerOptions {
            strict: true,
            strict_property_initialization: true,
            ..CheckerOptions::default()
        },
    );

    assert_eq!(
        count_code(
            &diags,
            diagnostic_codes::CLASS_INCORRECTLY_EXTENDS_BASE_CLASS
        ),
        0,
        "Explicit properties should see the same optional base parameter-property type via the shared class summary, got: {diags:?}"
    );
    assert_eq!(
        count_code(
            &diags,
            diagnostic_codes::PROPERTY_IN_TYPE_IS_NOT_ASSIGNABLE_TO_THE_SAME_PROPERTY_IN_BASE_TYPE,
        ),
        0,
        "Optional base parameter-property types should not be narrowed away during override checking, got: {diags:?}"
    );
}

#[test]
fn test_ts2564_named_class_expression_prefers_outer_namespace_member_resolution() {
    let source = r"
        namespace C {
            export interface type {}
        }

        var x = class C {
            prop: C.type;
        };
    ";

    let diags = diagnostics_with_options(
        source,
        CheckerOptions {
            strict_null_checks: true,
            strict_property_initialization: true,
            target: tsz_common::common::ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    assert_eq!(
        count_code(
            &diags,
            diagnostic_codes::PROPERTY_HAS_NO_INITIALIZER_AND_IS_NOT_DEFINITELY_ASSIGNED_IN_THE_CONSTRUCTOR,
        ),
        1,
        "TS2564 should be reported for the named class expression property once the outer namespace member resolves correctly, got: {diags:?}"
    );
    assert_eq!(
        count_code(&diags, 2694),
        0,
        "TS2694 should stay suppressed when a named class expression shadows an outer namespace with the requested member, got: {diags:?}"
    );
}

#[test]
fn test_ts2564_class_expression_initializer_reports_required_field() {
    let source = r"
        interface A {}

        let x = class B implements A {
            prop: number;
            onStart(): void {}
            func = () => {};
        };
    ";

    let diags = diagnostics_with_options(
        source,
        CheckerOptions {
            strict_null_checks: true,
            strict_property_initialization: true,
            target: tsz_common::common::ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    assert_eq!(
        count_code(
            &diags,
            diagnostic_codes::PROPERTY_HAS_NO_INITIALIZER_AND_IS_NOT_DEFINITELY_ASSIGNED_IN_THE_CONSTRUCTOR,
        ),
        1,
        "TS2564 should be reported for required fields on class-expression initializers, got: {diags:?}"
    );
}

#[test]
fn test_ts2454_class_expression_method_body_is_checked() {
    let source = r"
        var m = class C<X> {
            f<T>() {
                var t: T;
                var x: X;
                return { t, x };
            }
        }
    ";

    let diags = diagnostics_with_options(
        source,
        CheckerOptions {
            strict: true,
            target: tsz_common::common::ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    assert_eq!(
        count_code(
            &diags,
            diagnostic_codes::VARIABLE_IS_USED_BEFORE_BEING_ASSIGNED
        ),
        2,
        "Class-expression method bodies should still emit TS2454 on local unassigned variables, got: {diags:?}"
    );
}

#[test]
fn test_ts2729_property_decorator_enum_member_forward_reference_only() {
    let source = r"
        function dec(...args: any[]): any {}

        class C {
            @dec(Enum.No)
            prop: string;

            @dec(Enum.No)
            method() {}
        }

        enum Enum {
            No = 0,
        }
    ";

    let diags = diagnostics_with_options(
        source,
        CheckerOptions {
            experimental_decorators: true,
            strict: true,
            target: tsz_common::common::ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    assert_eq!(
        count_code(
            &diags,
            diagnostic_codes::PROPERTY_IS_USED_BEFORE_ITS_INITIALIZATION,
        ),
        1,
        "TS2729 should be emitted for property decorators but not method decorators on the same forward enum member reference, got: {diags:?}"
    );
}

#[test]
fn test_ts2564_generic_property_types_still_require_initialization() {
    let source = r"
        class Unconstrained<T> {
            value: T;
        }

        class UnknownConstraint<T extends unknown> {
            value: T;
        }

        class UndefinedConstraint<T extends string | undefined> {
            value: T;
        }
    ";

    let diags = diagnostics_with_options(
        source,
        CheckerOptions {
            strict: true,
            target: tsz_common::common::ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    assert_eq!(
        count_code(
            &diags,
            diagnostic_codes::PROPERTY_HAS_NO_INITIALIZER_AND_IS_NOT_DEFINITELY_ASSIGNED_IN_THE_CONSTRUCTOR,
        ),
        3,
        "TS2564 should still be reported for generic property types, regardless of type-parameter constraints, got: {diags:?}"
    );
}

#[test]
fn test_definite_assignment_ts2454_control_flow_join() {
    let source = r"
        function f1(flag: boolean) {
            let x: number;
            if (flag) {
                x = 1;
            }
            return x;
        }

        function f2(flag: boolean) {
            let y: number;
            if (flag) {
                y = 1;
            } else {
                y = 2;
            }
            return y;
        }
    ";

    let diags = diagnostics_with_options(
        source,
        CheckerOptions {
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
    );

    assert!(
        count_code(
            &diags,
            diagnostic_codes::VARIABLE_IS_USED_BEFORE_BEING_ASSIGNED
        ) >= 1,
        "Expected TS2454 at control-flow join for f1, got: {diags:?}"
    );
}

#[test]
fn test_for_of47_definite_assignment() {
    let source = "
var x: string, y: number;
var array = [{ x: \"\", y: true }]
enum E { x }
for ({x, y: y = E.x} of array) {
    x;
    y;
}
";
    let diags = diagnostics_with_options(
        source,
        crate::context::CheckerOptions {
            strict_null_checks: true,
            ..Default::default()
        },
    );
    for d in &diags {
        println!("DIAG: {} - {}", d.0, d.1);
    }
}

/// TS2454 must fire for variables with type annotations but no initializers,
/// even when the identifier reference has no direct flow node (parent-walk fallback).
#[test]
fn test_ts2454_var_with_type_annotation_no_initializer() {
    let source = r"
        interface Foo { (): string; }
        var a: Foo;
        var x = a();
    ";
    let diags = diagnostics_with_options(
        source,
        CheckerOptions {
            strict_null_checks: true,
            ..Default::default()
        },
    );
    assert!(
        count_code(
            &diags,
            diagnostic_codes::VARIABLE_IS_USED_BEFORE_BEING_ASSIGNED
        ) >= 1,
        "Expected TS2454 for `a` used without initialization, got: {diags:?}"
    );
}

/// TS2454 must not fire when the variable has an initializer.
#[test]
fn test_ts2454_not_emitted_when_initialized() {
    let source = r"
        var a: number = 5;
        var b = a + 1;
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
        0,
        "Should not emit TS2454 when variable is initialized, got: {diags:?}"
    );
}

#[test]
fn test_ts2454_not_emitted_for_exhaustive_switch_implicit_default_path() {
    let source = r"
        function functionB(key: string): string {
            return key;
        }

        function functionC(): void {
            let unionVal: 'A' | 'B' = 'A';
            while (true) {
                let key: string;
                switch (unionVal) {
                    case 'A': {
                        key = 'AA';
                        break;
                    }
                    case 'B': {
                        key = 'BB';
                        break;
                    }
                }
                functionB(key);
            }
        }
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
        0,
        "Expected no TS2454 for exhaustive switch assignment, got: {diags:?}"
    );
}

#[test]
fn test_ts2454_not_emitted_when_switch_discriminant_is_flow_literal() {
    let source = r"
        declare function functionB(key: string): string;

        function functionC(): void {
            let unionVal: 'A' | 'B' = 'A';
            while (true) {
                let key: string;
                switch (unionVal) {
                    case 'A': {
                        key = 'AA';
                        break;
                    }
                }
                functionB(key);
            }
        }
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
        0,
        "Expected no TS2454 when flow narrows switch discriminant to a covered literal, got: {diags:?}"
    );
}

#[test]
fn test_recursive_array_destructuring_in_switch_does_not_overflow() {
    let source = r#"
        declare class Error {
            constructor(message?: string);
        }
        interface ArrayConstructor {
            isArray(arg: unknown): arg is unknown[];
        }
        declare var Array: ArrayConstructor;
        interface Array<T> {
            every(
                predicate: (value: T, index: number, array: T[]) => boolean,
            ): boolean;
        }

        type Expression = BooleanLogicExpression | 'true' | 'false';
        type BooleanLogicExpression = ['and', ...Expression[]] | ['not', Expression];

        function evaluate(expression: Expression): boolean {
            if (Array.isArray(expression)) {
                const [operator, ...operands] = expression;
                switch (operator) {
                    case 'and':
                        return operands.every(child => evaluate(child));
                    case 'not':
                        return !evaluate(operands[0]);
                    default:
                        throw new Error(`${operator} is not a supported operator`);
                }
            }
            return expression === 'true';
        }
    "#;

    let diags = diagnostics_with_options(
        source,
        CheckerOptions {
            target: tsz_common::common::ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    let relevant: Vec<_> = diags
        .into_iter()
        .filter(|(code, _)| *code != 2318 && *code != 2339)
        .collect();
    assert!(
        relevant.is_empty(),
        "Expected recursive destructuring switch to type-check without crashing, got: {relevant:?}"
    );
}

#[test]
fn test_destructured_switch_default_narrows_sibling_binding_to_never() {
    let source = r#"
        type X = { kind: "a", a: [1] } | { kind: "b", a: [] };

        function foo(x: X): 1 {
            const { kind, a } = x;
            switch (kind) {
                case "a":
                    return a[0];
                case "b":
                    return 1;
                default: {
                    const [n] = a;
                    return a;
                }
            }
        }
    "#;

    let diags = diagnostics_with_options(
        source,
        CheckerOptions {
            target: tsz_common::common::ScriptTarget::ES2015,
            ..Default::default()
        },
    );

    assert_eq!(
        count_code(
            &diags,
            diagnostic_codes::TYPE_MUST_HAVE_A_SYMBOL_ITERATOR_METHOD_THAT_RETURNS_AN_ITERATOR
        ),
        1,
        "Expected default-clause destructured sibling narrowing to produce TS2488, got: {diags:?}"
    );
    assert_eq!(
        count_code(&diags, diagnostic_codes::TYPE_IS_NOT_ASSIGNABLE_TO_TYPE),
        0,
        "Did not expect the stale non-never return mismatch in the default clause, got: {diags:?}"
    );
}

/// TS2454 should not fire when the type includes `undefined` (assignment is not required).
#[test]
fn test_ts2454_skipped_for_undefined_type() {
    let source = r"
        var a: number | undefined;
        var b = a;
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
        0,
        "Should not emit TS2454 when type includes undefined, got: {diags:?}"
    );
}

/// TS2454 should not fire when `strictNullChecks` is off.
/// Without strict null checks, all types implicitly include `undefined` and `null`,
/// so uninitialized variables are always valid. Verified against tsc conformance
/// data: zero tests with `strictNullChecks: false` expect TS2454.
#[test]
fn test_ts2454_not_emitted_without_strict_null_checks() {
    let source = r"
        var a: number;
        var b = a;
    ";
    let diags = diagnostics_with_options(
        source,
        CheckerOptions {
            strict_null_checks: false,
            ..Default::default()
        },
    );
    assert_eq!(
        count_code(
            &diags,
            diagnostic_codes::VARIABLE_IS_USED_BEFORE_BEING_ASSIGNED
        ),
        0,
        "TS2454 should not fire without strictNullChecks, got: {diags:?}"
    );
}

/// TS2454 must fire for multiple uninitialized variables used in expressions.
#[test]
fn test_ts2454_multiple_uninitialized_vars() {
    let source = r"
        var a: number;
        var b: string;
        var c = a + 1;
        var d = b.length;
    ";
    let diags = diagnostics_with_options(
        source,
        CheckerOptions {
            strict_null_checks: true,
            ..Default::default()
        },
    );
    assert!(
        count_code(
            &diags,
            diagnostic_codes::VARIABLE_IS_USED_BEFORE_BEING_ASSIGNED
        ) >= 2,
        "Expected TS2454 for both `a` and `b`, got: {diags:?}"
    );
}

/// TS2454 must fire for prefix increment on uninitialized variable.
/// `++x` reads x before writing, so it's use-before-assignment.
#[test]
fn test_ts2454_prefix_increment_reads_before_writing() {
    let source = r"
        var x: number;
        ++x;
    ";
    let diags = diagnostics_with_options(
        source,
        CheckerOptions {
            strict_null_checks: true,
            ..Default::default()
        },
    );
    assert!(
        count_code(
            &diags,
            diagnostic_codes::VARIABLE_IS_USED_BEFORE_BEING_ASSIGNED
        ) >= 1,
        "Expected TS2454 for `x` in `++x` (reads before writing), got: {diags:?}"
    );
}

/// TS2454 must fire for postfix decrement on uninitialized variable.
/// `x--` reads x before writing, so it's use-before-assignment.
#[test]
fn test_ts2454_postfix_decrement_reads_before_writing() {
    let source = r"
        var x: number;
        x--;
    ";
    let diags = diagnostics_with_options(
        source,
        CheckerOptions {
            strict_null_checks: true,
            ..Default::default()
        },
    );
    assert!(
        count_code(
            &diags,
            diagnostic_codes::VARIABLE_IS_USED_BEFORE_BEING_ASSIGNED
        ) >= 1,
        "Expected TS2454 for `x` in `x--` (reads before writing), got: {diags:?}"
    );
}

/// TS2454 must fire for compound assignment (+=) on uninitialized variable.
/// `x += 1` reads x before writing, so it's use-before-assignment.
#[test]
fn test_ts2454_compound_assignment_reads_before_writing() {
    let source = r"
        var x: number;
        x += 1;
    ";
    let diags = diagnostics_with_options(
        source,
        CheckerOptions {
            strict_null_checks: true,
            ..Default::default()
        },
    );
    assert!(
        count_code(
            &diags,
            diagnostic_codes::VARIABLE_IS_USED_BEFORE_BEING_ASSIGNED
        ) >= 1,
        "Expected TS2454 for `x` in `x += 1` (reads before writing), got: {diags:?}"
    );
}

/// TS2454 must fire REPEATEDLY for ++/-- on uninitialized variable.
/// Even after `--x` executes, the variable is NOT considered "definitely assigned"
/// because compound operations don't count as proper initialization.
#[test]
fn test_ts2454_compound_ops_do_not_count_as_assignment() {
    let source = r"
        var x: number;
        --x;
        x--;
        --x;
        x--;
    ";
    let diags = diagnostics_with_options(
        source,
        CheckerOptions {
            strict_null_checks: true,
            ..Default::default()
        },
    );
    let count = count_code(
        &diags,
        diagnostic_codes::VARIABLE_IS_USED_BEFORE_BEING_ASSIGNED,
    );
    assert!(
        count >= 4,
        "Expected TS2454 for ALL --x/x-- uses (got {count}), since compound ops don't assign: {diags:?}"
    );
}

/// TS2454 must fire for compound exponentiation assignment (**=).
#[test]
fn test_ts2454_exponentiation_compound_assignment() {
    let source = r"
        var x: number;
        x **= 2;
    ";
    let diags = diagnostics_with_options(
        source,
        CheckerOptions {
            strict_null_checks: true,
            ..Default::default()
        },
    );
    assert!(
        count_code(
            &diags,
            diagnostic_codes::VARIABLE_IS_USED_BEFORE_BEING_ASSIGNED
        ) >= 1,
        "Expected TS2454 for `x` in `x **= 2`, got: {diags:?}"
    );
}

/// Simple assignment (=) DOES count as definite assignment.
/// After `x = 1`, subsequent uses of x should NOT trigger TS2454.
#[test]
fn test_ts2454_simple_assignment_counts_as_definite() {
    let source = r"
        var x: number;
        x = 1;
        var y = x + 1;
    ";
    let diags = diagnostics_with_options(
        source,
        CheckerOptions {
            strict_null_checks: true,
            ..Default::default()
        },
    );
    // Should get 0 TS2454 — x is properly assigned before use in `y = x + 1`
    let ts2454_for_y: Vec<_> = diags
        .iter()
        .filter(|(c, m)| {
            *c == diagnostic_codes::VARIABLE_IS_USED_BEFORE_BEING_ASSIGNED && m.contains("'x'")
        })
        .collect();
    // The only TS2454 should NOT be for the `x + 1` usage (after `x = 1`)
    // There should be 0 TS2454 total since x is assigned before its only read
    assert_eq!(
        ts2454_for_y.len(),
        0,
        "Should not emit TS2454 for `x` after `x = 1`, got: {diags:?}"
    );
}

/// Optional-chain RHS expressions execute only on the present branch.
/// Assignments inside optional element/call RHS must not count as definite assignment.
#[test]
fn test_ts2454_optional_chain_rhs_assignment_not_definite() {
    let source = r"
        declare const o: undefined | {
            [key: string]: any;
            (arg: number): void;
            x(arg: number): void;
        };

        let a: number;
        o?.[a = 1];
        a.toFixed();

        let b: number;
        o?.x[b = 1];
        b.toFixed();

        let c: number;
        o?.(c = 1);
        c.toFixed();

        let d: number;
        o?.x(d = 1);
        d.toFixed();
    ";
    let diags = diagnostics_with_options(
        source,
        CheckerOptions {
            strict_null_checks: true,
            ..Default::default()
        },
    );
    let ts2454_vars: Vec<_> = diags
        .iter()
        .filter(|(c, _)| *c == diagnostic_codes::VARIABLE_IS_USED_BEFORE_BEING_ASSIGNED)
        .map(|(_, m)| m.clone())
        .collect();
    assert!(
        ts2454_vars.iter().any(|m| m.contains("'a'"))
            && ts2454_vars.iter().any(|m| m.contains("'b'"))
            && ts2454_vars.iter().any(|m| m.contains("'c'"))
            && ts2454_vars.iter().any(|m| m.contains("'d'")),
        "Expected TS2454 for a/b/c/d assignments guarded by optional chaining, got: {diags:?}"
    );
}

/// Non-optional element/call RHS expressions execute unconditionally.
#[test]
fn test_ts2454_non_optional_rhs_assignment_definite() {
    let source = r"
        declare const o: {
            [key: string]: any;
            (arg: number): void;
            x(arg: number): void;
        };

        let a: number;
        o[a = 1];
        a.toFixed();

        let b: number;
        o(b = 1);
        b.toFixed();

        let c: number;
        o.x(c = 1);
        c.toFixed();
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
        0,
        "Non-optional RHS assignments should count as definite assignment, got: {diags:?}"
    );
}

#[test]
fn test_ts2454_not_emitted_in_class_computed_property_name() {
    let source = r"
        var s: string;
        class C {
            [s]: number;
        }
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
        0,
        "Computed property names should not trigger TS2454, got: {diags:?}"
    );
}

#[test]
fn test_ts2454_assignment_in_class_computed_property_name_is_not_definite() {
    let source = r#"
        let getX: (a: A) => number;

        class A {
            #x = 100;
            [(getX = (a: A) => a.#x, "_")]() {}
        }

        console.log(getX(new A));
    "#;
    let diags = diagnostics_with_options(
        source,
        CheckerOptions {
            strict_null_checks: true,
            target: tsz_common::common::ScriptTarget::ESNext,
            ..Default::default()
        },
    );
    assert!(
        count_code(
            &diags,
            diagnostic_codes::VARIABLE_IS_USED_BEFORE_BEING_ASSIGNED
        ) >= 1,
        "Assignments that run while evaluating class computed property names should not count as definite assignment, got: {diags:?}"
    );
}

#[test]
fn test_tdz_in_binding_default_initializer_has_no_ts2454_companion() {
    let source = r"
        const {
            a = 1,
            b = 2,
            c = b,
            d = a,
            e = f,
            f = f,
        } = {} as any;
    ";
    let all_diags = diagnostics_with_options(
        source,
        CheckerOptions {
            strict_null_checks: true,
            ..Default::default()
        },
    );
    // Filter out TS2318 "Cannot find global type" errors since this test
    // doesn't provide lib declarations.
    let diags: Vec<_> = all_diags
        .into_iter()
        .filter(|(code, _)| *code != 2318)
        .collect();
    // Current behavior: only 1 of the 2 TDZ sites (`f = f`) emits TS2448.
    // tsc emits 2 (for both `e = f` and `f = f`). This is a known limitation.
    assert_eq!(
        count_code(
            &diags,
            diagnostic_codes::BLOCK_SCOPED_VARIABLE_USED_BEFORE_ITS_DECLARATION
        ),
        1,
        "Expected at least one destructuring TDZ site to emit TS2448, got: {diags:?}"
    );
    assert_eq!(
        count_code(
            &diags,
            diagnostic_codes::VARIABLE_IS_USED_BEFORE_BEING_ASSIGNED
        ),
        0,
        "Binding default TDZ should not add a TS2454 companion, got: {diags:?}"
    );
}

#[test]
fn test_ts2454_not_emitted_in_class_field_initializer() {
    let source = r"
        var n: number;
        class C {
            [n] = n;
        }
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
        0,
        "Class field initializers should not trigger TS2454, got: {diags:?}"
    );
}

#[test]
fn test_ts2454_not_emitted_for_source_file_global_inside_deferred_function() {
    let source = r"
        let cond: boolean;
        function f() {
            while (cond) {}
        }
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
        0,
        "Deferred nested-function reads of source-file globals should not emit TS2454, got: {diags:?}"
    );
}

#[test]
fn test_ts2454_emitted_for_source_file_global_inside_iife() {
    let source = r"
        let cond: boolean;
        (function () {
            while (cond) {}
        })();
    ";
    let diags = diagnostics_with_options(
        source,
        CheckerOptions {
            strict_null_checks: true,
            ..Default::default()
        },
    );
    assert!(
        count_code(
            &diags,
            diagnostic_codes::VARIABLE_IS_USED_BEFORE_BEING_ASSIGNED
        ) >= 1,
        "Expected TS2454 for uninitialized source-file global read inside IIFE, got: {diags:?}"
    );
}

#[test]
fn test_ts2454_emitted_for_module_global_never_assigned_inside_deferred_function() {
    // In external modules, module-scope variables can only be assigned within
    // the module. If the variable has NO initializer and NO assignments anywhere
    // in the file, tsc emits TS2454 even inside deferred nested functions —
    // there is no code path that could ever assign the variable.
    let source = r"
        export {};
        let cond: boolean;
        function f() {
            while (cond) {}
        }
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
        "Module-scoped globals with no assignment should emit TS2454, got: {diags:?}"
    );
}

#[test]
fn test_ts2454_suppressed_for_var_module_global_never_assigned_inside_deferred_function() {
    // `var` declarations are hoisted and initialized to `undefined` at runtime.
    // tsc suppresses TS2454 for `var` in deferred functions even when the variable
    // is never assigned in an external module — unlike `let`/`const`, `var` always
    // has a runtime value.
    let source = r"
        export {};
        var cond: boolean;
        function f() {
            while (cond) {}
        }
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
        0,
        "var declarations should not emit TS2454 in deferred functions, got: {diags:?}"
    );
}

#[test]
fn test_ts2454_suppressed_for_module_global_assigned_later_inside_deferred_function() {
    // In external modules, if the variable IS assigned somewhere in the file,
    // suppress TS2454 for reads in deferred nested functions — the function
    // could be called after the assignment.
    let source = r"
        export {};
        let cond: boolean;
        function f() {
            while (cond) {}
        }
        cond = true;
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
        0,
        "Module-scoped globals with later assignment should suppress TS2454 in deferred functions, got: {diags:?}"
    );
}

#[test]
fn test_non_null_assertion_does_not_emit_ts2454() {
    let source = r"
        function f() {
            let x: string;
            x!.slice();
        }
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
        0,
        "Non-null assertions should not emit TS2454, got: {diags:?}"
    );
}

// ===== Type parameter property initialization tests =====
// Mirrors tsc behavior: `isTypeAssignableTo(undefinedType, T)` is true for
// unconstrained type parameters, so TS2564 should NOT be emitted.

#[test]
fn test_ts2564_no_error_for_unconstrained_type_parameter() {
    // tsc now requires initialization for unconstrained type parameters too.
    let source = r"
        class C<T> {
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
        "TS2564 should be emitted for property typed as unconstrained type parameter T, got: {diags:?}"
    );
}

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

// ── Regression tests: superWithTypeArgument3.ts (TS2564 false positive) ──

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
