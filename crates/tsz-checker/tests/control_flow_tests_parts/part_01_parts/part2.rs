#[test]
fn test_static_condition_branch_does_not_report_unreachable_exhaustive_switch() {
    use crate::CheckerState;
    use tsz_binder::BinderState;

    let source = r#"
function f1(x: 1 | 2): string {
    if (!!true) {
        switch (x) {
            case 1: return "a";
            case 2: return "b";
        }
        x;  // Unreachable
    }
    else {
        throw 0;
    }
}

enum E { A, B }

function g(e: E): number {
    if (!true)
        return -1;
    else
        switch (e) {
            case E.A: return 0;
            case E.B: return 1;
        }
}
"#;

    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let types = tsz_solver::construction::TypeInterner::new();
    let opts = crate::context::CheckerOptions {
        strict_null_checks: true,
        allow_unreachable_code: Some(false),
        ..Default::default()
    };
    let mut checker = CheckerState::new(arena, &binder, &types, "test.ts".to_string(), opts);
    checker.check_source_file(root);

    let ts7027: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 7027)
        .collect();
    let expected_start = source.find("x;  // Unreachable").expect("expected x tail") as u32;
    assert_eq!(
        ts7027.len(),
        1,
        "only the reachable post-switch tail should report TS7027; diagnostics: {:?}",
        checker.ctx.diagnostics
    );
    assert_eq!(
        ts7027[0].start, expected_start,
        "TS7027 should anchor at the post-switch tail"
    );
}

/// Issue #6823: an exhaustive numeric-enum switch must narrow the discriminant
/// to `never` in the `default` clause. The standard exhaustiveness pattern
/// (`const _: never = op`) must type-check without TS2322.
#[test]
fn test_ts2322_not_emitted_for_exhaustive_enum_switch_default_clause() {
    use crate::CheckerState;
    use tsz_binder::BinderState;

    let source = r#"
enum Operation {
    Add,
    Subtract,
    Multiply
}
function calculate(op: Operation, a: number, b: number): number {
    switch (op) {
        case Operation.Add: return a + b;
        case Operation.Subtract: return a - b;
        case Operation.Multiply: return a * b;
        default:
            const _exhaustive: never = op;
            return _exhaustive;
    }
}
"#;

    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let types = tsz_solver::construction::TypeInterner::new();
    let opts = crate::context::CheckerOptions {
        strict_null_checks: true,
        ..Default::default()
    };
    let mut checker = CheckerState::new(arena, &binder, &types, "test.ts".to_string(), opts);
    checker.check_source_file(root);

    let ts2322: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .collect();
    assert!(
        ts2322.is_empty(),
        "Exhaustive enum switch default must narrow to never; got TS2322: {ts2322:?}",
    );
}

/// Issue #6823 adjacent: renamed enum / numeric initialisers must behave the
/// same. The structural rule depends on enum nominal identity, not on
/// the spelling of member names.
#[test]
fn test_ts2322_not_emitted_for_exhaustive_renamed_enum_switch_default() {
    use crate::CheckerState;
    use tsz_binder::BinderState;

    let source = r#"
enum Direction {
    Up = 1, Down = 2, Left = 3, Right = 4
}
function handle(dir: Direction): string {
    switch (dir) {
        case Direction.Up: return "up";
        case Direction.Down: return "down";
        case Direction.Left: return "left";
        case Direction.Right: return "right";
        default:
            const exhaustive: never = dir;
            return exhaustive;
    }
}
"#;

    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let types = tsz_solver::construction::TypeInterner::new();
    let opts = crate::context::CheckerOptions {
        strict_null_checks: true,
        ..Default::default()
    };
    let mut checker = CheckerState::new(arena, &binder, &types, "test.ts".to_string(), opts);
    checker.check_source_file(root);

    let ts2322: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .collect();
    assert!(
        ts2322.is_empty(),
        "Renamed enum exhaustive switch default must narrow to never; got TS2322: {ts2322:?}",
    );
}

/// Issue #6823 adjacent: string-enum variant.
#[test]
fn test_ts2322_not_emitted_for_exhaustive_string_enum_switch_default() {
    use crate::CheckerState;
    use tsz_binder::BinderState;

    let source = r#"
enum Color {
    Red = "red",
    Green = "green",
    Blue = "blue"
}
function describe(c: Color): string {
    switch (c) {
        case Color.Red: return "r";
        case Color.Green: return "g";
        case Color.Blue: return "b";
        default:
            const exhaustive: never = c;
            return exhaustive;
    }
}
"#;

    let (parser, root) = parse_test_source(source);
    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let types = tsz_solver::construction::TypeInterner::new();
    let opts = crate::context::CheckerOptions {
        strict_null_checks: true,
        ..Default::default()
    };
    let mut checker = CheckerState::new(arena, &binder, &types, "test.ts".to_string(), opts);
    checker.check_source_file(root);

    let ts2322: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == 2322)
        .collect();
    assert!(
        ts2322.is_empty(),
        "String enum exhaustive switch default must narrow to never; got TS2322: {ts2322:?}",
    );
}
