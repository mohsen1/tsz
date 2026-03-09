use crate::CheckerState;
use crate::context::CheckerOptions;
use crate::diagnostics::diagnostic_codes;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn diagnostics_with_options(source: &str, options: CheckerOptions) -> Vec<(u32, String)> {
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
    checker
        .ctx
        .diagnostics
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
        "TS2454 requires strictNullChecks, got: {diags:?}"
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
        o?.(b = 1);
        b.toFixed();

        let c: number;
        o?.x(c = 1);
        c.toFixed();
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
        ) >= 3,
        "Expected TS2454 for assignments guarded by optional chaining, got: {diags:?}"
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
            diagnostic_codes::BLOCK_SCOPED_VARIABLE_USED_BEFORE_ITS_DECLARATION
        ),
        2,
        "Expected both destructuring TDZ sites to emit TS2448, got: {diags:?}"
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
fn test_ts2454_emitted_for_source_file_global_inside_module_function() {
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
    assert!(
        count_code(
            &diags,
            diagnostic_codes::VARIABLE_IS_USED_BEFORE_BEING_ASSIGNED
        ) >= 1,
        "Module-scoped globals should still emit TS2454 inside nested functions, got: {diags:?}"
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
