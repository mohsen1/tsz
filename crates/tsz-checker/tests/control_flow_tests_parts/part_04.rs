/// Test that try-catch-finally paths are captured.
#[test]
fn test_flow_graph_captures_try_catch_finally() {
    let source = r"
let x: number;
try {
    x = 1;
} catch (e) {
    x = 2;
} finally {
    x = 3;
}
x;
";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let arena = parser.get_arena();
    let root_node = arena.get(root).expect("root node");
    let source_file = arena.get_source_file(root_node).expect("source file");

    // The try statement should have flow recorded
    let try_stmt_idx = *source_file.statements.nodes.get(1).expect("try statement");
    let flow_at_try = binder.get_node_flow(try_stmt_idx);
    assert!(
        flow_at_try.is_some(),
        "Flow should be recorded at try statement"
    );
}

/// Test that loop control flow with break/continue is captured.
#[test]
fn test_flow_graph_captures_loop_break_continue() {
    let source = r"
let x: number;
for (let i = 0; i < 10; i++) {
    if (i === 5) break;
    if (i % 2 === 0) continue;
    x = i;
}
x;
";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let arena = parser.get_arena();
    let root_node = arena.get(root).expect("root node");
    let source_file = arena.get_source_file(root_node).expect("source file");

    // The for loop should have flow recorded
    let for_stmt_idx = *source_file.statements.nodes.get(1).expect("for statement");
    let flow_at_for = binder.get_node_flow(for_stmt_idx);
    assert!(flow_at_for.is_some(), "Flow should be recorded at for loop");
}

/// Test that nested control structures have correct flow.
#[test]
fn test_flow_graph_captures_nested_structures() {
    let source = r"
let x: number;
if (Math.random() > 0.5) {
    while (Math.random() > 0.1) {
        try {
            x = 1;
            break;
        } catch {
            x = 2;
        }
    }
} else {
    for (let i = 0; i < 5; i++) {
        switch (i) {
            case 0:
                x = 10;
                break;
            default:
                x = 20;
        }
    }
}
x;
";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let arena = parser.get_arena();
    let root_node = arena.get(root).expect("root node");
    let source_file = arena.get_source_file(root_node).expect("source file");

    // Verify final expression has flow
    let final_expr_idx = *source_file
        .statements
        .nodes
        .get(2)
        .expect("final expression");
    let flow_at_final = binder.get_node_flow(final_expr_idx);
    assert!(
        flow_at_final.is_some(),
        "Flow should be recorded at final expression after nested structures"
    );
}

/// Test that class constructor flow is tracked.
#[test]
fn test_flow_graph_captures_class_constructor() {
    let source = r"
class Foo {
    value: number;

    constructor(init: boolean) {
        if (init) {
            this.value = 1;
        } else {
            this.value = 2;
        }
    }
}
";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let arena = parser.get_arena();
    let root_node = arena.get(root).expect("root node");
    let source_file = arena.get_source_file(root_node).expect("source file");

    // Class should have flow recorded
    let class_idx = *source_file.statements.nodes.first().expect("class");
    let flow_at_class = binder.get_node_flow(class_idx);
    assert!(
        flow_at_class.is_some(),
        "Flow should be recorded at class declaration"
    );
}

/// Test that TS2454 is emitted when a variable is used before being assigned.
/// This verifies the definite assignment checking is working.
#[test]
fn test_ts2454_variable_used_before_assigned() {
    use crate::CheckerState;
    use tsz_binder::BinderState;

    use tsz_parser::parser::ParserState;

    let source = r"
function test() {
    let x: string;
    return x;  // Error: x is used before being assigned
}
";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let types = tsz_solver::TypeInterner::new();
    // TS2454 requires strictNullChecks (matches tsc behavior)
    let opts = crate::context::CheckerOptions {
        strict_null_checks: true,
        ..Default::default()
    };
    let mut checker = CheckerState::new(arena, &binder, &types, "test.ts".to_string(), opts);
    checker.check_source_file(root);

    // Should have TS2454 error
    let has_ts2454 = checker.ctx.diagnostics.iter().any(|d| d.code == 2454);
    assert!(
        has_ts2454,
        "Should have TS2454 error for variable used before assignment"
    );
}

#[test]
fn test_optional_chain_element_assignment_is_not_definite_for_later_use() {
    let source = r#"
declare const o: undefined | {
    [key: string]: any;
};

let b: number;
o?.x[b = 1];
b.toFixed();
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let types = TypeInterner::new();
    let opts = crate::context::CheckerOptions {
        strict_null_checks: true,
        ..Default::default()
    };
    let mut checker = CheckerState::new(arena, &binder, &types, "test.ts".to_string(), opts);
    checker.check_source_file(root);

    let b_ref = get_method_call_receiver_identifier(arena, root, 3);
    let flow_at_use = binder.get_node_flow(b_ref).expect("flow for b use");
    let analyzer = FlowAnalyzer::with_node_types(arena, &binder, &types, &checker.ctx.node_types);
    assert!(
        !analyzer.is_definitely_assigned(b_ref, flow_at_use),
        "b should not be definitely assigned at the later use"
    );
}

/// Test that TS2454 is NOT emitted when a variable has an initializer.
#[test]
fn test_ts2454_no_error_with_initializer() {
    use crate::CheckerState;
    use tsz_binder::BinderState;
    use tsz_parser::parser::ParserState;

    let source = r#"
function test() {
    let x: string = "hello";
    return x;  // OK: x is initialized
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let types = tsz_solver::TypeInterner::new();
    let mut checker = CheckerState::new(
        arena,
        &binder,
        &types,
        "test.ts".to_string(),
        crate::context::CheckerOptions::default(),
    );
    checker.check_source_file(root);

    // Should NOT have TS2454 error
    let has_ts2454 = checker.ctx.diagnostics.iter().any(|d| d.code == 2454);
    assert!(
        !has_ts2454,
        "Should NOT have TS2454 error when variable has initializer"
    );
}

#[test]
fn test_assignment_then_instanceof_merge_keeps_assigned_set_type() {
    use crate::CheckerState;
    use crate::diagnostics::diagnostic_codes;
    use tsz_binder::BinderState;
    use tsz_parser::parser::ParserState;

    let source = r#"
function f1(s: Set<string> | Set<number>) {
    s = new Set<number>();
    s;
    if (s instanceof Set) {
        s;
    }
    s;
    s.add(42);
}

function f2(s: Set<string> | Set<number>) {
    s = new Set<number>();
    s;
    if (s instanceof Promise) {
        s;
    }
    s;
    s.add(42);
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let types = tsz_solver::TypeInterner::new();
    let opts = crate::context::CheckerOptions {
        strict_null_checks: true,
        ..Default::default()
    };
    let mut checker = CheckerState::new(arena, &binder, &types, "test.ts".to_string(), opts);
    checker.check_source_file(root);

    let ts2339: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE)
        .map(|d| d.message_text.clone())
        .collect();

    assert!(
        ts2339.is_empty(),
        "instanceof merges should collapse back to the assigned Set<number> type, got: {ts2339:?}"
    );
}

#[test]
fn test_instanceof_accepts_annotated_union_after_function_augmentation() {
    use crate::CheckerState;
    use crate::diagnostics::diagnostic_codes;
    use tsz_binder::BinderState;
    use tsz_parser::parser::ParserState;

    let source = r#"
declare global {
    interface Function {
        now(): string;
    }
}

Function.prototype.now = function () {
    return "now";
};

class X {
    static now() {
        return {};
    }

    why() {}
}

export const x: X | number = Math.random() > 0.5 ? new X() : 1;

if (x instanceof X) {
    x.why();
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let types = tsz_solver::TypeInterner::new();
    let opts = crate::context::CheckerOptions {
        strict_null_checks: true,
        ..Default::default()
    };
    let mut checker = CheckerState::new(arena, &binder, &types, "test.ts".to_string(), opts);
    checker.check_source_file(root);

    let ts2358: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| {
            d.code
                == diagnostic_codes::THE_LEFT_HAND_SIDE_OF_AN_INSTANCEOF_EXPRESSION_MUST_BE_OF_TYPE_ANY_AN_OBJECT_TYP
        })
        .collect();
    let ts2339: Vec<_> = checker
        .ctx
        .diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::PROPERTY_DOES_NOT_EXIST_ON_TYPE)
        .collect();

    assert!(
        ts2358.is_empty(),
        "annotated union lhs should stay valid for instanceof, got: {ts2358:?}"
    );
    assert!(
        ts2339.is_empty(),
        "instanceof narrowing should preserve X in the true branch, got: {ts2339:?}"
    );
}

/// Exhaustive switch without default should satisfy return-path checking.
#[test]
fn test_ts2366_not_emitted_for_exhaustive_switch_without_default() {
    use crate::CheckerState;
    use tsz_binder::BinderState;
    use tsz_parser::parser::ParserState;

    let source = r#"
function f(v: 0 | 1): number {
    switch (v) {
        case 0:
            return 1;
        case 1:
            return 2;
    }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let types = tsz_solver::TypeInterner::new();
    let opts = crate::context::CheckerOptions {
        strict_null_checks: true,
        ..Default::default()
    };
    let mut checker = CheckerState::new(arena, &binder, &types, "test.ts".to_string(), opts);
    checker.check_source_file(root);

    let has_ts2366 = checker.ctx.diagnostics.iter().any(|d| d.code == 2366);
    assert!(
        !has_ts2366,
        "Exhaustive switch should not fall through; got diagnostics: {:?}",
        checker.ctx.diagnostics
    );
}

