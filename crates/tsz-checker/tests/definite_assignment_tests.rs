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
