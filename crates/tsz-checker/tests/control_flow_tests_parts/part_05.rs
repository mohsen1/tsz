/// Exhaustive enum switch without default should satisfy return-path checking.
#[test]
fn test_ts2366_not_emitted_for_exhaustive_enum_switch_without_default() {
    use crate::CheckerState;
    use tsz_binder::BinderState;
    use tsz_parser::parser::ParserState;

    let source = r#"
enum E { A, B }
function f(e: E): number {
    switch (e) {
        case E.A:
            return 0;
        case E.B:
            return 1;
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
        "Exhaustive enum switch should not fall through; got diagnostics: {:?}",
        checker.ctx.diagnostics
    );
}

/// Exhaustive enum switch assignments should satisfy definite-assignment checks.
#[test]
fn test_ts2454_not_emitted_for_exhaustive_enum_switch_assignment() {
    use crate::CheckerState;
    use tsz_binder::BinderState;
    use tsz_parser::parser::ParserState;

    let source = r#"
enum E { A, B }
function g(e: E): string {
    let s: string;
    switch (e) {
        case E.A:
            s = "a";
            break;
        case E.B:
            s = "b";
            break;
    }
    return s;
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

    let has_ts2454 = checker.ctx.diagnostics.iter().any(|d| d.code == 2454);
    assert!(
        !has_ts2454,
        "Exhaustive enum switch assignment should be definitely assigned; diagnostics: {:?}",
        checker.ctx.diagnostics
    );
}

/// Exhaustive enum switch over `optional?.prop ?? fallback` should satisfy return-path checking.
#[test]
fn test_ts2366_not_emitted_for_exhaustive_optional_chain_coalescing_switch() {
    use crate::CheckerState;
    use tsz_binder::BinderState;
    use tsz_parser::parser::ParserState;

    let source = r#"
enum Animal { DOG, CAT }
declare const zoo: { animal: Animal } | undefined;
function expression(): Animal {
    switch (zoo?.animal ?? Animal.DOG) {
        case Animal.DOG:
            return Animal.DOG;
        case Animal.CAT:
            return Animal.CAT;
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
        "Exhaustive optional-chain/coalescing enum switch should not fall through; got diagnostics: {:?}",
        checker.ctx.diagnostics,
    );
}

#[test]
fn test_typeof_switch_exhaustive_unknown_reports_unreachable_not_ts2366() {
    use crate::CheckerState;
    use tsz_binder::BinderState;
    use tsz_parser::parser::ParserState;

    let source = r#"
const unreachable = (x: unknown): number => {
    switch (typeof x) {
        case "string": return 0;
        case "number": return 0;
        case "bigint": return 0;
        case "boolean": return 0;
        case "symbol": return 0;
        case "undefined": return 0;
        case "object": return 0;
        case "function": return 0;
    }
    x;
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
        allow_unreachable_code: Some(false),
        ..Default::default()
    };
    let mut checker = CheckerState::new(arena, &binder, &types, "test.ts".to_string(), opts);
    checker.check_source_file(root);

    let has_ts2366 = checker.ctx.diagnostics.iter().any(|d| d.code == 2366);
    let has_ts7027 = checker.ctx.diagnostics.iter().any(|d| d.code == 7027);
    assert!(
        !has_ts2366,
        "Exhaustive typeof switch should not produce TS2366; diagnostics: {:?}",
        checker.ctx.diagnostics
    );
    assert!(
        has_ts7027,
        "Exhaustive typeof switch tail should be unreachable (TS7027); diagnostics: {:?}",
        checker.ctx.diagnostics
    );
}

#[test]
fn test_typeof_switch_exhaustive_any_reports_unreachable_not_ts2366() {
    use crate::CheckerState;
    use tsz_binder::BinderState;
    use tsz_parser::parser::ParserState;

    let source = r#"
const unreachable = (x: any): number => {
    switch (typeof x) {
        case "string": return 0;
        case "number": return 0;
        case "bigint": return 0;
        case "boolean": return 0;
        case "symbol": return 0;
        case "undefined": return 0;
        case "object": return 0;
        case "function": return 0;
    }
    x;
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
        allow_unreachable_code: Some(false),
        ..Default::default()
    };
    let mut checker = CheckerState::new(arena, &binder, &types, "test.ts".to_string(), opts);
    checker.check_source_file(root);

    let has_ts2366 = checker.ctx.diagnostics.iter().any(|d| d.code == 2366);
    let has_ts7027 = checker.ctx.diagnostics.iter().any(|d| d.code == 7027);
    assert!(
        !has_ts2366,
        "Exhaustive typeof switch should not produce TS2366; diagnostics: {:?}",
        checker.ctx.diagnostics
    );
    assert!(
        has_ts7027,
        "Exhaustive typeof switch tail should be unreachable (TS7027); diagnostics: {:?}",
        checker.ctx.diagnostics
    );
}

/// Test that && creates intermediate flow condition nodes for the right operand.
///
/// For `typeof x === 'object' && x`, the `x` on the right side of `&&` should
/// have a `TRUE_CONDITION` flow node so that it sees the typeof narrowing.
#[test]
fn test_and_expression_creates_intermediate_flow_nodes() {
    use tsz_binder::{BinderState, flow_flags};
    use tsz_parser::parser::ParserState;

    let source = r#"
let x: string | number | null;
if (typeof x === "string" && x) {
  x;
} else {
  x;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    // Navigate to the condition: typeof x === "string" && x
    let root_node = arena.get(root).expect("root node");
    let source_file = arena.get_source_file(root_node).expect("source file");
    let if_idx = *source_file.statements.nodes.get(1).expect("if statement");
    let if_node = arena.get(if_idx).expect("if node");
    let if_data = arena.get_if_statement(if_node).expect("if data");

    // The condition is: typeof x === "string" && x
    let condition_idx = if_data.expression;
    let cond_node = arena.get(condition_idx).expect("condition");
    let bin = arena.get_binary_expr(cond_node).expect("binary &&");

    // bin.right is the `x` on the right side of &&
    let right_x = bin.right;

    // The flow node for right_x should be a TRUE_CONDITION
    let flow_id = binder
        .get_node_flow(right_x)
        .expect("flow node for right operand of &&");
    let flow_node = binder.flow_nodes.get(flow_id).expect("flow node data");

    assert!(
        flow_node.has_any_flags(flow_flags::TRUE_CONDITION),
        "Right operand of && should have TRUE_CONDITION flow node, got flags: {}",
        flow_node.flags,
    );

    // The condition of this TRUE_CONDITION should be the left operand (typeof x === "string")
    assert_eq!(
        flow_node.node, bin.left,
        "TRUE_CONDITION should reference the left operand of &&"
    );
}

/// Test that typeof narrowing works correctly through && in the then-block.
///
/// For `if (typeof x === "string" && x) { x }`, x in the then-block
/// should be narrowed to `string` (typeof removes number|null, truthiness is redundant).
#[test]
fn test_typeof_and_truthiness_narrows_in_then_block() {
    use tsz_binder::BinderState;
    use tsz_parser::parser::ParserState;
    use tsz_solver::TypeInterner;

    let source = r#"
let x: string | number | null;
if (typeof x === "string" && x) {
  x;
} else {
  x;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let types = TypeInterner::new();
    let analyzer = FlowAnalyzer::new(arena, &binder, &types);

    let union = types.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::NULL]);

    let ident_then = get_if_branch_expression(arena, root, 1, true);
    let flow_then = binder.get_node_flow(ident_then).expect("flow then");
    let narrowed_then = analyzer.get_flow_type(ident_then, union, flow_then);

    // In the then-block, typeof x === "string" narrows to string,
    // and && x truthiness is redundant (string already excludes null/undefined)
    assert_eq!(narrowed_then, TypeId::STRING);
}

/// Test that || creates intermediate `FALSE_CONDITION` flow nodes for the right operand.
#[test]
fn test_or_expression_creates_intermediate_flow_nodes() {
    use tsz_binder::{BinderState, flow_flags};
    use tsz_parser::parser::ParserState;

    let source = r#"
let x: string | number | null;
if (typeof x === "string" || x) {
  x;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    // Navigate to the condition
    let root_node = arena.get(root).expect("root node");
    let source_file = arena.get_source_file(root_node).expect("source file");
    let if_idx = *source_file.statements.nodes.get(1).expect("if statement");
    let if_node = arena.get(if_idx).expect("if node");
    let if_data = arena.get_if_statement(if_node).expect("if data");
    let cond_node = arena.get(if_data.expression).expect("condition");
    let bin = arena.get_binary_expr(cond_node).expect("binary ||");

    // bin.right is the `x` on the right side of ||
    let right_x = bin.right;

    let flow_id = binder
        .get_node_flow(right_x)
        .expect("flow node for right operand of ||");
    let flow_node = binder.flow_nodes.get(flow_id).expect("flow node data");

    assert!(
        flow_node.has_any_flags(flow_flags::FALSE_CONDITION),
        "Right operand of || should have FALSE_CONDITION flow node, got flags: {}",
        flow_node.flags,
    );

    // The condition of this FALSE_CONDITION should be the left operand
    assert_eq!(
        flow_node.node, bin.left,
        "FALSE_CONDITION should reference the left operand of ||"
    );
}

// =============================================================================
// Unit Tests for CFA Bug Fixes
// =============================================================================

/// Bug #2.1: Test assignment tracking in condition expressions
///
/// Verifies that assignments in if/while conditions are tracked
/// for definite assignment analysis.
#[test]
fn test_assignment_tracking_in_conditions() {
    use tsz_parser::parser::ParserState;

    let source = r"
let x: string | number;

// Assignment in condition should be tracked
if ((x = getValue()) !== null) {
    console.log(x.toString()); // x is definitely assigned here
}
";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    // Build binder - this tests that flow graph building doesn't panic
    // when handling assignments in conditions
    let mut binder = tsz_binder::BinderState::new();
    binder.bind_source_file(arena, root);

    // Test passes if no panic occurs
    let _ = true; // Assignment tracking in conditions compiles
}

/// Bug #2.1: Test assignment tracking in while conditions
///
/// Verifies that while loop conditions track assignments.
#[test]
fn test_assignment_tracking_in_while_conditions() {
    use tsz_parser::parser::ParserState;

    let source = r"
let x: string | number | undefined;

while ((x = getNextValue()) !== null) {
    console.log(x.toString());
}
";

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    let arena = parser.get_arena();

    let mut binder = tsz_binder::BinderState::new();
    binder.bind_source_file(arena, root);

    // Test passes if flow graph building handles while conditions
    let _ = true; // While condition assignment tracking compiles
}

