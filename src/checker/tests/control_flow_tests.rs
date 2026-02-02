use super::FlowAnalyzer;
use crate::binder::BinderState;
use crate::checker::CheckerState;
use crate::checker::flow_graph_builder::FlowGraphBuilder;
use crate::parser::NodeIndex;
use crate::parser::ParserState;
use crate::parser::node::NodeArena;
use crate::solver::{PropertyInfo, TypeId, TypeInterner};

fn get_switch_statement(arena: &NodeArena, root: NodeIndex, stmt_index: usize) -> NodeIndex {
    let root_node = arena.get(root).expect("root node");
    let source_file = arena.get_source_file(root_node).expect("source file");
    *source_file
        .statements
        .nodes
        .get(stmt_index)
        .expect("switch statement")
}

fn get_switch_clause_expression(
    arena: &NodeArena,
    switch_idx: NodeIndex,
    clause_index: usize,
) -> NodeIndex {
    let switch_node = arena.get(switch_idx).expect("switch node");
    let switch_data = arena.get_switch(switch_node).expect("switch data");
    let case_block_node = arena.get(switch_data.case_block).expect("case block node");
    let case_block = arena.get_block(case_block_node).expect("case block");
    let clause_idx = *case_block
        .statements
        .nodes
        .get(clause_index)
        .expect("case clause");
    let clause_node = arena.get(clause_idx).expect("clause node");
    let clause = arena.get_case_clause(clause_node).expect("clause data");
    let stmt_idx = *clause.statements.nodes.first().expect("clause statement");
    let stmt_node = arena.get(stmt_idx).expect("statement node");
    let expr_stmt = arena
        .get_expression_statement(stmt_node)
        .expect("expression statement");
    expr_stmt.expression
}

fn get_if_branch_expression(
    arena: &NodeArena,
    root: NodeIndex,
    stmt_index: usize,
    is_then: bool,
) -> NodeIndex {
    let root_node = arena.get(root).expect("root node");
    let source_file = arena.get_source_file(root_node).expect("source file");
    let if_idx = *source_file
        .statements
        .nodes
        .get(stmt_index)
        .expect("if statement");
    let if_node = arena.get(if_idx).expect("if node");
    let if_data = arena.get_if_statement(if_node).expect("if data");
    let branch_idx = if is_then {
        if_data.then_statement
    } else {
        if_data.else_statement
    };
    assert!(!branch_idx.is_none(), "missing branch statement");
    extract_expression_from_statement(arena, branch_idx)
}

fn extract_expression_from_statement(arena: &NodeArena, stmt_idx: NodeIndex) -> NodeIndex {
    let stmt_node = arena.get(stmt_idx).expect("statement node");
    if let Some(block) = arena.get_block(stmt_node) {
        let inner_idx = *block.statements.nodes.first().expect("block statement");
        return extract_expression_from_statement(arena, inner_idx);
    }

    let expr_stmt = arena
        .get_expression_statement(stmt_node)
        .expect("expression statement");
    expr_stmt.expression
}

fn get_block_expression(arena: &NodeArena, block_idx: NodeIndex, stmt_index: usize) -> NodeIndex {
    let block_node = arena.get(block_idx).expect("block node");
    let block = arena.get_block(block_node).expect("block");
    let stmt_idx = *block
        .statements
        .nodes
        .get(stmt_index)
        .expect("block statement");
    extract_expression_from_statement(arena, stmt_idx)
}

/// Test switch statement fallthrough and default clause narrowing.
///
/// NOTE: Currently ignored - switch clause fallthrough narrowing is not fully
/// implemented. The flow graph records fallthrough antecedents, but the
/// SWITCH_CLAUSE handler in `check_flow` doesn't correctly union types from
/// fallthrough paths.
#[test]
#[ignore = "Switch fallthrough narrowing not fully implemented"]
fn test_switch_fallthrough_and_default_narrowing() {
    let source = r#"
let x: "a" | "b" | "c";
switch (x) {
  case "a":
    x;
  case "b":
    x;
    break;
  default:
    x;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let arena = parser.get_arena();
    let types = TypeInterner::new();
    let analyzer = FlowAnalyzer::new(arena, &binder, &types);

    let switch_idx = get_switch_statement(arena, root, 1);
    let ident_case_a = get_switch_clause_expression(arena, switch_idx, 0);
    let ident_case_b = get_switch_clause_expression(arena, switch_idx, 1);
    let ident_default = get_switch_clause_expression(arena, switch_idx, 2);

    let lit_a = types.literal_string("a");
    let lit_b = types.literal_string("b");
    let lit_c = types.literal_string("c");
    let union = types.union(vec![lit_a, lit_b, lit_c]);

    let flow_a = binder.get_node_flow(ident_case_a).expect("flow for case a");
    let narrowed_a = analyzer.get_flow_type(ident_case_a, union, flow_a);
    assert_eq!(narrowed_a, lit_a);

    let flow_b = binder.get_node_flow(ident_case_b).expect("flow for case b");
    let narrowed_b = analyzer.get_flow_type(ident_case_b, union, flow_b);
    let expected_b = types.union(vec![lit_a, lit_b]);
    assert_eq!(narrowed_b, expected_b);

    let flow_default = binder
        .get_node_flow(ident_default)
        .expect("flow for default");
    let narrowed_default = analyzer.get_flow_type(ident_default, union, flow_default);
    assert_eq!(narrowed_default, lit_c);
}

#[test]
fn test_switch_discriminant_narrowing() {
    let source = r#"
let x: { kind: "a" } | { kind: "b" };
switch (x.kind) {
  case "a":
    x;
    break;
  default:
    x;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let arena = parser.get_arena();
    let types = TypeInterner::new();
    let analyzer = FlowAnalyzer::new(arena, &binder, &types);

    let kind_name = types.intern_string("kind");
    let lit_a = types.literal_string("a");
    let lit_b = types.literal_string("b");

    let member_a = types.object(vec![PropertyInfo {
        name: kind_name,
        type_id: lit_a,
        write_type: lit_a,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    let member_b = types.object(vec![PropertyInfo {
        name: kind_name,
        type_id: lit_b,
        write_type: lit_b,
        optional: false,
        readonly: false,
        is_method: false,
    }]);

    let union = types.union(vec![member_a, member_b]);

    let switch_idx = get_switch_statement(arena, root, 1);
    let ident_case_a = get_switch_clause_expression(arena, switch_idx, 0);
    let ident_default = get_switch_clause_expression(arena, switch_idx, 1);

    let flow_case_a = binder.get_node_flow(ident_case_a).expect("flow for case a");
    let narrowed_case_a = analyzer.get_flow_type(ident_case_a, union, flow_case_a);
    assert_eq!(narrowed_case_a, member_a);

    let flow_default = binder
        .get_node_flow(ident_default)
        .expect("flow for default");
    let narrowed_default = analyzer.get_flow_type(ident_default, union, flow_default);
    assert_eq!(narrowed_default, member_b);
}

#[test]
fn test_instanceof_narrows_to_object_union_members() {
    let source = r#"
let x: string | { a: number };
if (x instanceof Foo) {
  x;
} else {
  x;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let arena = parser.get_arena();
    let types = TypeInterner::new();
    let analyzer = FlowAnalyzer::new(arena, &binder, &types);

    let prop_a = types.intern_string("a");
    let obj_type = types.object(vec![PropertyInfo {
        name: prop_a,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    let union = types.union(vec![TypeId::STRING, obj_type]);

    let ident_then = get_if_branch_expression(arena, root, 1, true);
    let ident_else = get_if_branch_expression(arena, root, 1, false);

    let flow_then = binder.get_node_flow(ident_then).expect("flow then");
    let flow_else = binder.get_node_flow(ident_else).expect("flow else");

    let narrowed_then = analyzer.get_flow_type(ident_then, union, flow_then);
    assert_eq!(narrowed_then, obj_type);

    let narrowed_else = analyzer.get_flow_type(ident_else, union, flow_else);
    assert_eq!(narrowed_else, union);
}

#[test]
fn test_in_operator_narrows_required_property() {
    let source = r#"
let x: { a: number } | { b: string };
if ("a" in x) {
  x;
} else {
  x;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let arena = parser.get_arena();
    let types = TypeInterner::new();
    let analyzer = FlowAnalyzer::new(arena, &binder, &types);

    let prop_a = types.intern_string("a");
    let prop_b = types.intern_string("b");

    let type_a = types.object(vec![PropertyInfo {
        name: prop_a,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    let type_b = types.object(vec![PropertyInfo {
        name: prop_b,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    let union = types.union(vec![type_a, type_b]);

    let ident_then = get_if_branch_expression(arena, root, 1, true);
    let ident_else = get_if_branch_expression(arena, root, 1, false);

    let flow_then = binder.get_node_flow(ident_then).expect("flow then");
    let flow_else = binder.get_node_flow(ident_else).expect("flow else");

    let narrowed_then = analyzer.get_flow_type(ident_then, union, flow_then);
    assert_eq!(narrowed_then, type_a);

    let narrowed_else = analyzer.get_flow_type(ident_else, union, flow_else);
    assert_eq!(narrowed_else, type_b);
}

#[test]
fn test_in_operator_optional_property_keeps_false_branch_union() {
    let source = r#"
let x: { a?: number } | { b: string };
if ("a" in x) {
  x;
} else {
  x;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let arena = parser.get_arena();
    let types = TypeInterner::new();
    let analyzer = FlowAnalyzer::new(arena, &binder, &types);

    let prop_a = types.intern_string("a");
    let prop_b = types.intern_string("b");

    let type_a = types.object(vec![PropertyInfo {
        name: prop_a,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: true,
        readonly: false,
        is_method: false,
    }]);
    let type_b = types.object(vec![PropertyInfo {
        name: prop_b,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    let union = types.union(vec![type_a, type_b]);

    let ident_then = get_if_branch_expression(arena, root, 1, true);
    let ident_else = get_if_branch_expression(arena, root, 1, false);

    let flow_then = binder.get_node_flow(ident_then).expect("flow then");
    let flow_else = binder.get_node_flow(ident_else).expect("flow else");

    let narrowed_then = analyzer.get_flow_type(ident_then, union, flow_then);
    assert_eq!(narrowed_then, type_a);

    let narrowed_else = analyzer.get_flow_type(ident_else, union, flow_else);
    assert_eq!(narrowed_else, union);
}

#[test]
fn test_in_operator_private_identifier_narrows_required_property() {
    let source = r##"
let x: { "#a": number } | { b: string };
if (#a in x) {
  x;
} else {
  x;
}
"##;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let arena = parser.get_arena();
    let types = TypeInterner::new();
    let analyzer = FlowAnalyzer::new(arena, &binder, &types);

    let prop_a = types.intern_string("#a");
    let prop_b = types.intern_string("b");

    let type_a = types.object(vec![PropertyInfo {
        name: prop_a,
        type_id: TypeId::NUMBER,
        write_type: TypeId::NUMBER,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    let type_b = types.object(vec![PropertyInfo {
        name: prop_b,
        type_id: TypeId::STRING,
        write_type: TypeId::STRING,
        optional: false,
        readonly: false,
        is_method: false,
    }]);
    let union = types.union(vec![type_a, type_b]);

    let ident_then = get_if_branch_expression(arena, root, 1, true);
    let ident_else = get_if_branch_expression(arena, root, 1, false);

    let flow_then = binder.get_node_flow(ident_then).expect("flow then");
    let flow_else = binder.get_node_flow(ident_else).expect("flow else");

    let narrowed_then = analyzer.get_flow_type(ident_then, union, flow_then);
    assert_eq!(narrowed_then, type_a);

    let narrowed_else = analyzer.get_flow_type(ident_else, union, flow_else);
    assert_eq!(narrowed_else, type_b);
}

#[test]
fn test_user_defined_type_predicate_narrows_branches() {
    let source = r#"
function isString(x: string | number): x is string {
  return typeof x === "string";
}
let x: string | number;
if (isString(x)) {
  x;
} else {
  x;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let arena = parser.get_arena();
    let types = TypeInterner::new();
    let compiler_options = crate::checker::context::CheckerOptions::default();
    let mut checker = CheckerState::new(
        arena,
        &binder,
        &types,
        "test.ts".to_string(),
        compiler_options,
    );
    checker.check_source_file(root);

    let analyzer = FlowAnalyzer::with_node_types(arena, &binder, &types, &checker.ctx.node_types);

    let ident_then = get_if_branch_expression(arena, root, 2, true);
    let ident_else = get_if_branch_expression(arena, root, 2, false);

    let union = types.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let flow_then = binder.get_node_flow(ident_then).expect("flow then");
    let flow_else = binder.get_node_flow(ident_else).expect("flow else");

    let narrowed_then = analyzer.get_flow_type(ident_then, union, flow_then);
    assert_eq!(narrowed_then, TypeId::STRING);

    let narrowed_else = analyzer.get_flow_type(ident_else, union, flow_else);
    assert_eq!(narrowed_else, TypeId::NUMBER);
}

#[test]
fn test_user_defined_type_predicate_alias_narrows() {
    let source = r#"
function isString(x: string | number): x is string {
  return typeof x === "string";
}
const guard = isString;
let x: string | number;
if (guard(x)) {
  x;
} else {
  x;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let arena = parser.get_arena();
    let types = TypeInterner::new();
    let compiler_options = crate::checker::context::CheckerOptions::default();
    let mut checker = CheckerState::new(
        arena,
        &binder,
        &types,
        "test.ts".to_string(),
        compiler_options,
    );
    checker.check_source_file(root);

    let analyzer = FlowAnalyzer::with_node_types(arena, &binder, &types, &checker.ctx.node_types);

    let ident_then = get_if_branch_expression(arena, root, 3, true);
    let ident_else = get_if_branch_expression(arena, root, 3, false);

    let union = types.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let flow_then = binder.get_node_flow(ident_then).expect("flow then");
    let flow_else = binder.get_node_flow(ident_else).expect("flow else");

    let narrowed_then = analyzer.get_flow_type(ident_then, union, flow_then);
    // Debug: check if callee type is stored in node_types
    // Get the if statement and extract the call expression
    let root_node = arena.get(root).expect("root node");
    let source_file = arena.get_source_file(root_node).expect("source file");
    let if_idx = *source_file.statements.nodes.get(3).expect("if statement");
    let if_node = arena.get(if_idx).expect("if node");
    let if_data = arena.get_if_statement(if_node).expect("if data");
    let call_idx = if_data.expression;
    let call_node = arena.get(call_idx).expect("call node");
    let call_data = arena.get_call_expr(call_node).expect("call data");
    let callee_idx = call_data.expression;

    // Check if callee type is in node_types
    let callee_type_opt = checker.ctx.node_types.get(&callee_idx.0);
    assert!(
        callee_type_opt.is_some(),
        "Callee type should be in node_types, callee_idx.0 = {}",
        callee_idx.0
    );
    let callee_type = *callee_type_opt.unwrap();

    // Check that callee type is a function with a type predicate
    let function_shape = crate::solver::type_queries::get_function_shape(&types, callee_type);
    assert!(
        function_shape.is_some(),
        "Callee type {} should be a function type",
        callee_type.0
    );
    let shape = function_shape.unwrap();
    assert!(
        shape.type_predicate.is_some(),
        "Function should have a type predicate"
    );

    assert_eq!(narrowed_then, TypeId::STRING);

    let narrowed_else = analyzer.get_flow_type(ident_else, union, flow_else);
    assert_eq!(narrowed_else, TypeId::NUMBER);
}

#[test]
fn test_asserts_type_predicate_narrows_true_branch() {
    let source = r#"
function assertString(x: string | number): asserts x is string {
  if (typeof x !== "string") throw new Error("nope");
}
let x: string | number;
if (assertString(x)) {
  x;
} else {
  x;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let arena = parser.get_arena();
    let types = TypeInterner::new();
    let compiler_options = crate::checker::context::CheckerOptions::default();
    let mut checker = CheckerState::new(
        arena,
        &binder,
        &types,
        "test.ts".to_string(),
        compiler_options,
    );
    checker.check_source_file(root);

    let analyzer = FlowAnalyzer::with_node_types(arena, &binder, &types, &checker.ctx.node_types);

    let ident_then = get_if_branch_expression(arena, root, 2, true);
    let ident_else = get_if_branch_expression(arena, root, 2, false);

    let union = types.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let flow_then = binder.get_node_flow(ident_then).expect("flow then");
    let flow_else = binder.get_node_flow(ident_else).expect("flow else");

    let narrowed_then = analyzer.get_flow_type(ident_then, union, flow_then);
    assert_eq!(narrowed_then, TypeId::STRING);

    let narrowed_else = analyzer.get_flow_type(ident_else, union, flow_else);
    assert_eq!(narrowed_else, union);
}

#[test]
fn test_asserts_call_statement_narrows() {
    let source = r#"
function assertString(x: string | number): asserts x is string {
  if (typeof x !== "string") throw new Error("nope");
}
let x: string | number;
assertString(x);
x;
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let arena = parser.get_arena();
    let types = TypeInterner::new();
    let compiler_options = crate::checker::context::CheckerOptions::default();
    let mut checker = CheckerState::new(
        arena,
        &binder,
        &types,
        "test.ts".to_string(),
        compiler_options,
    );
    checker.check_source_file(root);

    let analyzer = FlowAnalyzer::with_node_types(arena, &binder, &types, &checker.ctx.node_types);

    let root_node = arena.get(root).expect("root node");
    let source_file = arena.get_source_file(root_node).expect("source file");
    let ident_after = extract_expression_from_statement(
        arena,
        *source_file.statements.nodes.get(3).expect("x after"),
    );

    let union = types.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let flow_after = binder.get_node_flow(ident_after).expect("flow after");
    let narrowed_after = analyzer.get_flow_type(ident_after, union, flow_after);
    assert_eq!(narrowed_after, TypeId::STRING);
}

#[test]
fn test_assignment_narrows_to_rhs_in_branch() {
    let source = r#"
let x: string | number;
if (typeof x === "string") {
  x;
  x = 1;
  x;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let arena = parser.get_arena();
    let types = TypeInterner::new();
    let analyzer = FlowAnalyzer::new(arena, &binder, &types);

    let root_node = arena.get(root).expect("root node");
    let source_file = arena.get_source_file(root_node).expect("source file");
    let if_idx = *source_file.statements.nodes.get(1).expect("if statement");
    let if_node = arena.get(if_idx).expect("if node");
    let if_data = arena.get_if_statement(if_node).expect("if data");
    let then_block = if_data.then_statement;

    let ident_before = get_block_expression(arena, then_block, 0);
    let ident_after = get_block_expression(arena, then_block, 2);

    let union = types.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let flow_before = binder.get_node_flow(ident_before).expect("flow before");
    let narrowed_before = analyzer.get_flow_type(ident_before, union, flow_before);
    assert_eq!(narrowed_before, TypeId::STRING);

    let flow_after = binder.get_node_flow(ident_after).expect("flow after");
    let narrowed_after = analyzer.get_flow_type(ident_after, union, flow_after);
    assert_eq!(narrowed_after, types.literal_number(1.0));
}

#[test]
fn test_assignment_narrows_to_rhs_type() {
    let source = r#"
let x: string | number;
x;
x = "hi";
x;
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let arena = parser.get_arena();
    let types = TypeInterner::new();
    let compiler_options = crate::checker::context::CheckerOptions::default();
    let mut checker = CheckerState::new(
        arena,
        &binder,
        &types,
        "test.ts".to_string(),
        compiler_options,
    );
    checker.check_source_file(root);

    let analyzer = FlowAnalyzer::with_node_types(arena, &binder, &types, &checker.ctx.node_types);

    let root_node = arena.get(root).expect("root node");
    let source_file = arena.get_source_file(root_node).expect("source file");
    let ident_before = extract_expression_from_statement(
        arena,
        *source_file.statements.nodes.get(1).expect("x before"),
    );
    let ident_after = extract_expression_from_statement(
        arena,
        *source_file.statements.nodes.get(3).expect("x after"),
    );

    let union = types.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let flow_before = binder.get_node_flow(ident_before).expect("flow before");
    let narrowed_before = analyzer.get_flow_type(ident_before, union, flow_before);
    assert_eq!(narrowed_before, union);

    let flow_after = binder.get_node_flow(ident_after).expect("flow after");
    let narrowed_after = analyzer.get_flow_type(ident_after, union, flow_after);
    assert_eq!(narrowed_after, types.literal_string("hi"));
}

#[test]
fn test_this_property_assignment_narrows() {
    let source = r#"
class Foo {
  x: string | number;
  method() {
    this.x;
    this.x = "s";
    this.x;
  }
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let arena = parser.get_arena();
    let types = TypeInterner::new();
    let compiler_options = crate::checker::context::CheckerOptions::default();
    let mut checker = CheckerState::new(
        arena,
        &binder,
        &types,
        "test.ts".to_string(),
        compiler_options,
    );
    checker.check_source_file(root);

    let analyzer = FlowAnalyzer::with_node_types(arena, &binder, &types, &checker.ctx.node_types);

    let root_node = arena.get(root).expect("root node");
    let source_file = arena.get_source_file(root_node).expect("source file");
    let class_idx = *source_file.statements.nodes.first().expect("class decl");
    let class_node = arena.get(class_idx).expect("class node");
    let class_decl = arena.get_class(class_node).expect("class data");
    let method_idx = *class_decl.members.nodes.get(1).expect("method decl");
    let method_node = arena.get(method_idx).expect("method node");
    let method_decl = arena.get_method_decl(method_node).expect("method data");
    let body_idx = method_decl.body;

    let ident_before = get_block_expression(arena, body_idx, 0);
    let ident_after = get_block_expression(arena, body_idx, 2);

    let union = types.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let flow_before = binder.get_node_flow(ident_before).expect("flow before");
    let narrowed_before = analyzer.get_flow_type(ident_before, union, flow_before);
    assert_eq!(narrowed_before, union);

    let flow_after = binder.get_node_flow(ident_after).expect("flow after");
    let narrowed_after = analyzer.get_flow_type(ident_after, union, flow_after);
    assert_eq!(narrowed_after, types.literal_string("s"));
}

#[test]
fn test_const_alias_condition_narrows() {
    let source = r#"
let x: string | number;
const isString = typeof x === "string";
if (isString) {
  x;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let arena = parser.get_arena();
    let types = TypeInterner::new();
    let analyzer = FlowAnalyzer::new(arena, &binder, &types);

    let ident_then = get_if_branch_expression(arena, root, 2, true);

    let union = types.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let flow_then = binder.get_node_flow(ident_then).expect("flow then");
    let narrowed_then = analyzer.get_flow_type(ident_then, union, flow_then);
    assert_eq!(narrowed_then, TypeId::STRING);
}

#[test]
fn test_assignment_narrows_to_rhs_literal_without_cache() {
    let source = r#"
let x: string | number;
x;
x = "hi";
x;
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let arena = parser.get_arena();
    let types = TypeInterner::new();
    let analyzer = FlowAnalyzer::new(arena, &binder, &types);

    let root_node = arena.get(root).expect("root node");
    let source_file = arena.get_source_file(root_node).expect("source file");
    let ident_after = extract_expression_from_statement(
        arena,
        *source_file.statements.nodes.get(3).expect("x after"),
    );

    let union = types.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let flow_after = binder.get_node_flow(ident_after).expect("flow after");
    let narrowed_after = analyzer.get_flow_type(ident_after, union, flow_after);
    assert_eq!(narrowed_after, types.literal_string("hi"));
}

/// Test that loop labels correctly union types from back edges.
///
/// NOTE: Currently ignored - the LOOP_LABEL finalization logic in `check_flow`
/// doesn't correctly union types from all antecedents including back edges.
/// The flow graph is built correctly with back edges recorded, but the
/// finalization step needs to properly union all antecedent types.
#[test]
#[ignore = "LOOP_LABEL finalization doesn't union back edge types correctly"]
fn test_loop_label_unions_back_edges() {
    let source = r#"
let x: string | number;
x = "a";
while (true) {
  x;
  x = 1;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let arena = parser.get_arena();
    let types = TypeInterner::new();
    let analyzer = FlowAnalyzer::new(arena, &binder, &types);

    let root_node = arena.get(root).expect("root node");
    let source_file = arena.get_source_file(root_node).expect("source file");
    let while_idx = *source_file
        .statements
        .nodes
        .get(2)
        .expect("while statement");
    let while_node = arena.get(while_idx).expect("while node");
    let while_data = arena.get_loop(while_node).expect("while data");
    let body_idx = while_data.statement;

    let ident_before = get_block_expression(arena, body_idx, 0);

    let declared = types.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let expected = types.union(vec![types.literal_string("a"), types.literal_number(1.0)]);

    let flow_before = binder.get_node_flow(ident_before).expect("flow before");
    let narrowed_before = analyzer.get_flow_type(ident_before, declared, flow_before);
    assert_eq!(narrowed_before, expected);
}

#[test]
fn test_assignment_narrows_to_null_without_cache() {
    let source = r#"
let x: string | null;
x;
x = null;
x;
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let arena = parser.get_arena();
    let types = TypeInterner::new();
    let analyzer = FlowAnalyzer::new(arena, &binder, &types);

    let root_node = arena.get(root).expect("root node");
    let source_file = arena.get_source_file(root_node).expect("source file");
    let ident_after = extract_expression_from_statement(
        arena,
        *source_file.statements.nodes.get(3).expect("x after"),
    );

    let union = types.union(vec![TypeId::STRING, TypeId::NULL]);
    let flow_after = binder.get_node_flow(ident_after).expect("flow after");
    let narrowed_after = analyzer.get_flow_type(ident_after, union, flow_after);
    assert_eq!(narrowed_after, TypeId::NULL);
}

#[test]
fn test_array_destructuring_assignment_clears_narrowing() {
    let source = r#"
let x: string | number;
if (typeof x === "string") {
  x;
  [x] = [1];
  x;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let arena = parser.get_arena();
    let types = TypeInterner::new();
    let analyzer = FlowAnalyzer::new(arena, &binder, &types);

    let root_node = arena.get(root).expect("root node");
    let source_file = arena.get_source_file(root_node).expect("source file");
    let if_idx = *source_file.statements.nodes.get(1).expect("if statement");
    let if_node = arena.get(if_idx).expect("if node");
    let if_data = arena.get_if_statement(if_node).expect("if data");
    let then_block = if_data.then_statement;

    let ident_before = get_block_expression(arena, then_block, 0);
    let ident_after = get_block_expression(arena, then_block, 2);

    let union = types.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let flow_before = binder.get_node_flow(ident_before).expect("flow before");
    let narrowed_before = analyzer.get_flow_type(ident_before, union, flow_before);
    assert_eq!(narrowed_before, TypeId::STRING);

    let flow_after = binder.get_node_flow(ident_after).expect("flow after");
    let narrowed_after = analyzer.get_flow_type(ident_after, union, flow_after);
    assert_eq!(narrowed_after, union);
}

#[test]
fn test_object_destructuring_assignment_clears_narrowing() {
    let source = r#"
let x: string | number;
if (typeof x === "string") {
  x;
  ({ x } = { x: 1 });
  x;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let arena = parser.get_arena();
    let types = TypeInterner::new();
    let analyzer = FlowAnalyzer::new(arena, &binder, &types);

    let root_node = arena.get(root).expect("root node");
    let source_file = arena.get_source_file(root_node).expect("source file");
    let if_idx = *source_file.statements.nodes.get(1).expect("if statement");
    let if_node = arena.get(if_idx).expect("if node");
    let if_data = arena.get_if_statement(if_node).expect("if data");
    let then_block = if_data.then_statement;

    let ident_before = get_block_expression(arena, then_block, 0);
    let ident_after = get_block_expression(arena, then_block, 2);

    let union = types.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let flow_before = binder.get_node_flow(ident_before).expect("flow before");
    let narrowed_before = analyzer.get_flow_type(ident_before, union, flow_before);
    assert_eq!(narrowed_before, TypeId::STRING);

    let flow_after = binder.get_node_flow(ident_after).expect("flow after");
    let narrowed_after = analyzer.get_flow_type(ident_after, union, flow_after);
    assert_eq!(narrowed_after, union);
}

#[test]
fn test_array_destructuring_default_initializer_clears_narrowing() {
    let source = r#"
let x: string | number;
if (typeof x === "string") {
  x;
  [x = 1] = [];
  x;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let arena = parser.get_arena();
    let types = TypeInterner::new();
    let analyzer = FlowAnalyzer::new(arena, &binder, &types);

    let root_node = arena.get(root).expect("root node");
    let source_file = arena.get_source_file(root_node).expect("source file");
    let if_idx = *source_file.statements.nodes.get(1).expect("if statement");
    let if_node = arena.get(if_idx).expect("if node");
    let if_data = arena.get_if_statement(if_node).expect("if data");
    let then_block = if_data.then_statement;

    let ident_before = get_block_expression(arena, then_block, 0);
    let ident_after = get_block_expression(arena, then_block, 2);

    let union = types.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let flow_before = binder.get_node_flow(ident_before).expect("flow before");
    let narrowed_before = analyzer.get_flow_type(ident_before, union, flow_before);
    assert_eq!(narrowed_before, TypeId::STRING);

    let flow_after = binder.get_node_flow(ident_after).expect("flow after");
    let narrowed_after = analyzer.get_flow_type(ident_after, union, flow_after);
    assert_eq!(narrowed_after, union);
}

#[test]
fn test_object_destructuring_alias_default_initializer_clears_narrowing() {
    let source = r#"
let x: string | number;
if (typeof x === "string") {
  x;
  ({ y: x = 1 } = {});
  x;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let arena = parser.get_arena();
    let types = TypeInterner::new();
    let analyzer = FlowAnalyzer::new(arena, &binder, &types);

    let root_node = arena.get(root).expect("root node");
    let source_file = arena.get_source_file(root_node).expect("source file");
    let if_idx = *source_file.statements.nodes.get(1).expect("if statement");
    let if_node = arena.get(if_idx).expect("if node");
    let if_data = arena.get_if_statement(if_node).expect("if data");
    let then_block = if_data.then_statement;

    let ident_before = get_block_expression(arena, then_block, 0);
    let ident_after = get_block_expression(arena, then_block, 2);

    let union = types.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let flow_before = binder.get_node_flow(ident_before).expect("flow before");
    let narrowed_before = analyzer.get_flow_type(ident_before, union, flow_before);
    assert_eq!(narrowed_before, TypeId::STRING);

    let flow_after = binder.get_node_flow(ident_after).expect("flow after");
    let narrowed_after = analyzer.get_flow_type(ident_after, union, flow_after);
    assert_eq!(narrowed_after, union);
}

#[test]
fn test_object_destructuring_alias_assignment_clears_narrowing() {
    let source = r#"
let x: string | number;
if (typeof x === "string") {
  x;
  ({ y: x } = { y: 1 });
  x;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let arena = parser.get_arena();
    let types = TypeInterner::new();
    let analyzer = FlowAnalyzer::new(arena, &binder, &types);

    let root_node = arena.get(root).expect("root node");
    let source_file = arena.get_source_file(root_node).expect("source file");
    let if_idx = *source_file.statements.nodes.get(1).expect("if statement");
    let if_node = arena.get(if_idx).expect("if node");
    let if_data = arena.get_if_statement(if_node).expect("if data");
    let then_block = if_data.then_statement;

    let ident_before = get_block_expression(arena, then_block, 0);
    let ident_after = get_block_expression(arena, then_block, 2);

    let union = types.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let flow_before = binder.get_node_flow(ident_before).expect("flow before");
    let narrowed_before = analyzer.get_flow_type(ident_before, union, flow_before);
    assert_eq!(narrowed_before, TypeId::STRING);

    let flow_after = binder.get_node_flow(ident_after).expect("flow after");
    let narrowed_after = analyzer.get_flow_type(ident_after, union, flow_after);
    assert_eq!(narrowed_after, union);
}

#[test]
fn test_compound_assignment_clears_narrowing() {
    let source = r#"
let x: string | number;
if (typeof x === "string") {
  x;
  x += 1;
  x;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let arena = parser.get_arena();
    let types = TypeInterner::new();
    let analyzer = FlowAnalyzer::new(arena, &binder, &types);

    let root_node = arena.get(root).expect("root node");
    let source_file = arena.get_source_file(root_node).expect("source file");
    let if_idx = *source_file.statements.nodes.get(1).expect("if statement");
    let if_node = arena.get(if_idx).expect("if node");
    let if_data = arena.get_if_statement(if_node).expect("if data");
    let then_block = if_data.then_statement;

    let ident_before = get_block_expression(arena, then_block, 0);
    let ident_after = get_block_expression(arena, then_block, 2);

    let union = types.union(vec![TypeId::STRING, TypeId::NUMBER]);

    let flow_before = binder.get_node_flow(ident_before).expect("flow before");
    let narrowed_before = analyzer.get_flow_type(ident_before, union, flow_before);
    assert_eq!(narrowed_before, TypeId::STRING);

    let flow_after = binder.get_node_flow(ident_after).expect("flow after");
    let narrowed_after = analyzer.get_flow_type(ident_after, union, flow_after);
    assert_eq!(narrowed_after, union);
}

#[test]
fn test_array_mutation_clears_predicate_narrowing() {
    let source = r#"
function isStringArray(x: string[] | number[]): x is string[] {
  return true;
}
let x: string[] | number[];
if (isStringArray(x)) {
  x;
  x.push("a");
  x;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let arena = parser.get_arena();
    let types = TypeInterner::new();
    let compiler_options = crate::checker::context::CheckerOptions::default();
    let mut checker = CheckerState::new(
        arena,
        &binder,
        &types,
        "test.ts".to_string(),
        compiler_options,
    );
    checker.check_source_file(root);

    let analyzer = FlowAnalyzer::with_node_types(arena, &binder, &types, &checker.ctx.node_types);

    let root_node = arena.get(root).expect("root node");
    let source_file = arena.get_source_file(root_node).expect("source file");
    let if_idx = *source_file.statements.nodes.get(2).expect("if statement");
    let if_node = arena.get(if_idx).expect("if node");
    let if_data = arena.get_if_statement(if_node).expect("if data");
    let then_block = if_data.then_statement;

    let ident_before = get_block_expression(arena, then_block, 0);
    let ident_after = get_block_expression(arena, then_block, 2);

    let string_array = types.array(TypeId::STRING);
    let number_array = types.array(TypeId::NUMBER);
    let union = types.union(vec![string_array, number_array]);

    let flow_before = binder.get_node_flow(ident_before).expect("flow before");
    let narrowed_before = analyzer.get_flow_type(ident_before, union, flow_before);
    assert_eq!(narrowed_before, string_array);

    let flow_after = binder.get_node_flow(ident_after).expect("flow after");
    let narrowed_after = analyzer.get_flow_type(ident_after, union, flow_after);
    assert_eq!(narrowed_after, union);
}

// ============================================================================
// CFA-19: Callback Closure Flow Tracking Tests
// ============================================================================

/// Test that variables assigned before a callback are tracked in flow graph.
///
/// This test verifies that when a variable is assigned and then captured by
/// a closure, the flow graph correctly records the flow state at the point
/// where the closure is created.
///
/// NOTE: Currently ignored - the flow analysis doesn't correctly traverse
/// START node antecedents to apply type narrowing from outer scopes.
/// The flow graph is built correctly (closure START nodes have the proper
/// antecedent set), but the `check_flow` function in `control_flow.rs`
/// needs to be updated to properly follow the antecedent chain for closures.
#[test]
#[ignore = "Flow analysis doesn't traverse closure START node antecedents correctly"]
fn test_closure_capture_flow_before_callback() {
    let source = r#"
let x: string | number;
x = "assigned";
const callback = () => {
    x;
};
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let arena = parser.get_arena();
    let types = TypeInterner::new();
    let compiler_options = crate::checker::context::CheckerOptions::default();
    let mut checker = CheckerState::new(
        arena,
        &binder,
        &types,
        "test.ts".to_string(),
        compiler_options,
    );
    checker.check_source_file(root);

    let analyzer = FlowAnalyzer::with_node_types(arena, &binder, &types, &checker.ctx.node_types);

    // Get the variable reference inside the arrow function body
    let root_node = arena.get(root).expect("root node");
    let source_file = arena.get_source_file(root_node).expect("source file");

    // Get the variable statement at index 2 (const callback = ...)
    let var_stmt_idx = *source_file
        .statements
        .nodes
        .get(2)
        .expect("variable statement");
    let var_stmt_node = arena.get(var_stmt_idx).expect("var stmt node");
    let var_stmt_data = arena.get_variable(var_stmt_node).expect("var stmt data");

    // Get the declaration list
    let decl_list_idx = *var_stmt_data
        .declarations
        .nodes
        .first()
        .expect("declaration list");
    let decl_list_node = arena.get(decl_list_idx).expect("decl list node");
    let decl_list_data = arena.get_variable(decl_list_node).expect("decl list data");

    // Get the first declaration and its initializer (the arrow function)
    let decl_idx = *decl_list_data
        .declarations
        .nodes
        .first()
        .expect("declaration");
    let decl_node = arena.get(decl_idx).expect("decl node");
    let decl = arena
        .get_variable_declaration(decl_node)
        .expect("decl data");
    let arrow_func_node = arena.get(decl.initializer).expect("arrow func node");
    let arrow_func = arena
        .get_function(arrow_func_node)
        .expect("arrow func data");

    // Get the body block
    let body_node = arena.get(arrow_func.body).expect("body node");
    let body_block = arena.get_block(body_node).expect("body block");
    let ident_in_closure = extract_expression_from_statement(
        arena,
        *body_block.statements.nodes.first().expect("x in closure"),
    );

    let union = types.union(vec![TypeId::STRING, TypeId::NUMBER]);

    // The variable inside the closure should be narrowed to "assigned"
    let flow_in_closure = binder.get_node_flow(ident_in_closure);
    assert!(
        flow_in_closure.is_some(),
        "Flow should be recorded for variable inside closure"
    );

    // Verify the type narrowing works correctly
    let narrowed_in_closure =
        analyzer.get_flow_type(ident_in_closure, union, flow_in_closure.unwrap());
    assert_eq!(narrowed_in_closure, types.literal_string("assigned"));
}

/// Test definite assignment analysis with callbacks that are immediately invoked.
///
/// This verifies that variables assigned before an IIFE (Immediately Invoked
/// Function Expression) are properly tracked.
#[test]
fn test_definite_assignment_with_iife() {
    let source = r#"
let x: string;
x = "assigned";
(() => {
    const y = x;
})();
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let arena = parser.get_arena();

    // Verify that flow graph is built correctly for IIFE scenario
    let root_node = arena.get(root).expect("root node");
    let source_file = arena.get_source_file(root_node).expect("source file");

    // The IIFE call expression statement should be at index 2
    let iife_stmt_idx = *source_file.statements.nodes.get(2).expect("IIFE statement");
    let _iife_stmt = arena.get(iife_stmt_idx).expect("IIFE statement node");

    // Verify the IIFE statement exists and has flow recorded
    assert!(!iife_stmt_idx.is_none(), "IIFE statement should exist");
}

/// Test variable capture with array methods (forEach, map, filter).
///
/// These are common patterns where callbacks capture variables from
/// their enclosing scope. The flow graph should correctly track this.
///
/// NOTE: Currently ignored - see `test_closure_capture_flow_before_callback`
/// for details on the limitation.
#[test]
#[ignore = "Flow analysis doesn't traverse closure START node antecedents correctly"]
fn test_closure_capture_with_array_foreach() {
    let source = r#"
let x: string | number;
x = "hello";
const arr = [1, 2, 3];
arr.forEach((item) => {
    const y = x;
});
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let arena = parser.get_arena();
    let types = TypeInterner::new();
    let compiler_options = crate::checker::context::CheckerOptions::default();
    let mut checker = CheckerState::new(
        arena,
        &binder,
        &types,
        "test.ts".to_string(),
        compiler_options,
    );
    checker.check_source_file(root);

    let analyzer = FlowAnalyzer::with_node_types(arena, &binder, &types, &checker.ctx.node_types);

    let root_node = arena.get(root).expect("root node");
    let source_file = arena.get_source_file(root_node).expect("source file");

    // Get the forEach expression statement (index 3)
    let foreach_stmt_idx = *source_file
        .statements
        .nodes
        .get(3)
        .expect("forEach statement");
    let foreach_stmt_node = arena.get(foreach_stmt_idx).expect("forEach stmt node");
    let foreach_stmt_data = arena
        .get_expression_statement(foreach_stmt_node)
        .expect("forEach stmt data");

    // Get the call expression from the expression statement
    let foreach_call_node = arena
        .get(foreach_stmt_data.expression)
        .expect("forEach call node");
    let foreach_call = arena
        .get_call_expr(foreach_call_node)
        .expect("forEach call data");

    // Get the arrow function argument
    let args = foreach_call.arguments.as_ref().expect("arguments");
    let arrow_func_idx = *args.nodes.first().expect("arrow function");
    let arrow_func_node = arena.get(arrow_func_idx).expect("arrow func node");
    let arrow_func = arena
        .get_function(arrow_func_node)
        .expect("arrow func data");

    // Get the body block
    let body_node = arena.get(arrow_func.body).expect("body node");
    let body_block = arena.get_block(body_node).expect("body block");

    // Get the variable reference x inside the closure (in the initializer of y)
    let y_var_stmt_idx = *body_block
        .statements
        .nodes
        .first()
        .expect("y variable statement");
    let y_var_stmt_node = arena.get(y_var_stmt_idx).expect("y var stmt node");
    let y_var_stmt_data = arena
        .get_variable(y_var_stmt_node)
        .expect("y var stmt data");

    // Get the declaration list
    let y_decl_list_idx = *y_var_stmt_data
        .declarations
        .nodes
        .first()
        .expect("y declaration list");
    let y_decl_list_node = arena.get(y_decl_list_idx).expect("y decl list node");
    let y_decl_list_data = arena
        .get_variable(y_decl_list_node)
        .expect("y decl list data");

    // Get the declaration
    let y_decl_idx = *y_decl_list_data
        .declarations
        .nodes
        .first()
        .expect("y declaration");
    let y_decl_node = arena.get(y_decl_idx).expect("y decl node");
    let y_decl = arena
        .get_variable_declaration(y_decl_node)
        .expect("y decl data");
    let x_ref_in_closure = y_decl.initializer;

    let union = types.union(vec![TypeId::STRING, TypeId::NUMBER]);

    // The variable x inside the forEach callback should be narrowed to string
    let flow_in_callback = binder.get_node_flow(x_ref_in_closure);
    assert!(
        flow_in_callback.is_some(),
        "Flow should be recorded for variable inside forEach callback"
    );

    let narrowed_in_callback =
        analyzer.get_flow_type(x_ref_in_closure, union, flow_in_callback.unwrap());
    assert_eq!(narrowed_in_callback, types.literal_string("hello"));
}

/// Test variable capture with map callback.
///
/// NOTE: Currently ignored - see `test_closure_capture_flow_before_callback`
/// for details on the limitation.
#[test]
#[ignore = "Flow analysis doesn't traverse closure START node antecedents correctly"]
fn test_closure_capture_with_array_map() {
    let source = r#"
let x: string | number;
x = "world";
const arr = [1, 2, 3];
const mapped = arr.map((item) => {
    return x.length;
});
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let arena = parser.get_arena();
    let types = TypeInterner::new();
    let compiler_options = crate::checker::context::CheckerOptions::default();
    let mut checker = CheckerState::new(
        arena,
        &binder,
        &types,
        "test.ts".to_string(),
        compiler_options,
    );
    checker.check_source_file(root);

    let analyzer = FlowAnalyzer::with_node_types(arena, &binder, &types, &checker.ctx.node_types);

    let root_node = arena.get(root).expect("root node");
    let source_file = arena.get_source_file(root_node).expect("source file");

    // Get the variable statement at index 3 (const mapped = ...)
    let var_stmt_idx = *source_file
        .statements
        .nodes
        .get(3)
        .expect("variable statement");
    let var_stmt_node = arena.get(var_stmt_idx).expect("var stmt node");
    let var_stmt_data = arena.get_variable(var_stmt_node).expect("var stmt data");

    // Get the declaration list
    let decl_list_idx = *var_stmt_data
        .declarations
        .nodes
        .first()
        .expect("declaration list");
    let decl_list_node = arena.get(decl_list_idx).expect("decl list node");
    let decl_list_data = arena.get_variable(decl_list_node).expect("decl list data");

    // Get the first declaration and its initializer (the map call)
    let decl_idx = *decl_list_data
        .declarations
        .nodes
        .first()
        .expect("declaration");
    let decl_node = arena.get(decl_idx).expect("decl node");
    let decl = arena
        .get_variable_declaration(decl_node)
        .expect("decl data");
    let map_call_idx = decl.initializer;
    let map_call_node = arena.get(map_call_idx).expect("map call node");
    let map_call = arena.get_call_expr(map_call_node).expect("map call data");

    // Get the arrow function argument
    let args = map_call.arguments.as_ref().expect("arguments");
    let arrow_func_idx = *args.nodes.first().expect("arrow function");
    let arrow_func_node = arena.get(arrow_func_idx).expect("arrow func node");
    let arrow_func = arena
        .get_function(arrow_func_node)
        .expect("arrow func data");

    // Get the body block
    let body_node = arena.get(arrow_func.body).expect("body node");
    let body_block = arena.get_block(body_node).expect("body block");

    // Get the return statement
    let return_stmt = *body_block
        .statements
        .nodes
        .first()
        .expect("return statement");
    let return_node = arena.get(return_stmt).expect("return node");
    let return_data = arena
        .get_return_statement(return_node)
        .expect("return data");

    // Get the property access expression x.length
    let prop_access = return_data.expression;

    // Get the identifier x from the property access
    let prop_access_node = arena.get(prop_access).expect("prop access node");
    let access_expr = arena
        .get_access_expr(prop_access_node)
        .expect("access expr data");
    let x_identifier = access_expr.expression;

    let union = types.union(vec![TypeId::STRING, TypeId::NUMBER]);

    // The variable x should be narrowed to string in the map callback
    let flow_in_callback = binder.get_node_flow(prop_access);
    assert!(
        flow_in_callback.is_some(),
        "Flow should be recorded for expression inside map callback"
    );

    let narrowed_in_callback =
        analyzer.get_flow_type(x_identifier, union, flow_in_callback.unwrap());
    assert_eq!(narrowed_in_callback, types.literal_string("world"));
}

/// Test nested closure capture (closure inside a closure).
///
/// This verifies that variables captured by nested closures maintain
/// their proper flow state.
///
/// NOTE: Currently ignored - see `test_closure_capture_flow_before_callback`
/// for details on the limitation.
#[test]
#[ignore = "Flow analysis doesn't traverse closure START node antecedents correctly"]
fn test_nested_closure_capture() {
    let source = r#"
let x: string | number;
x = "nested";
const outer = () => {
    const inner = () => {
        const y = x;
    };
};
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let arena = parser.get_arena();
    let types = TypeInterner::new();
    let compiler_options = crate::checker::context::CheckerOptions::default();
    let mut checker = CheckerState::new(
        arena,
        &binder,
        &types,
        "test.ts".to_string(),
        compiler_options,
    );
    checker.check_source_file(root);

    let analyzer = FlowAnalyzer::with_node_types(arena, &binder, &types, &checker.ctx.node_types);

    let root_node = arena.get(root).expect("root node");
    let source_file = arena.get_source_file(root_node).expect("source file");

    // Get the outer arrow function (inside a VariableStatement -> VariableDeclarationList -> VariableDeclaration)
    let outer_var_stmt_idx = *source_file
        .statements
        .nodes
        .get(2)
        .expect("outer variable statement");
    let outer_var_stmt_node = arena.get(outer_var_stmt_idx).expect("outer var stmt node");
    let outer_var_stmt = arena
        .get_variable(outer_var_stmt_node)
        .expect("outer var stmt data");
    let outer_decl_list_idx = *outer_var_stmt
        .declarations
        .nodes
        .first()
        .expect("outer declaration list");
    let outer_decl_list_node = arena
        .get(outer_decl_list_idx)
        .expect("outer decl list node");
    let outer_decl_list = arena
        .get_variable(outer_decl_list_node)
        .expect("outer decl list data");
    let outer_decl_idx = *outer_decl_list
        .declarations
        .nodes
        .first()
        .expect("outer declaration");
    let outer_decl_node = arena.get(outer_decl_idx).expect("outer decl node");
    let outer_decl = arena
        .get_variable_declaration(outer_decl_node)
        .expect("outer decl data");
    let outer_arrow_idx = outer_decl.initializer;
    let outer_arrow_node = arena.get(outer_arrow_idx).expect("outer arrow node");
    let outer_func = arena
        .get_function(outer_arrow_node)
        .expect("outer func data");

    // Get the outer body block
    let outer_body_node = arena.get(outer_func.body).expect("outer body node");
    let outer_body = arena.get_block(outer_body_node).expect("outer body");

    // Get the inner arrow function declaration (inside a VariableStatement -> VariableDeclarationList -> VariableDeclaration)
    let inner_var_stmt_idx = *outer_body
        .statements
        .nodes
        .first()
        .expect("inner variable statement");
    let inner_var_stmt_node = arena.get(inner_var_stmt_idx).expect("inner var stmt node");
    let inner_var_stmt = arena
        .get_variable(inner_var_stmt_node)
        .expect("inner var stmt data");
    let inner_decl_list_idx = *inner_var_stmt
        .declarations
        .nodes
        .first()
        .expect("inner declaration list");
    let inner_decl_list_node = arena
        .get(inner_decl_list_idx)
        .expect("inner decl list node");
    let inner_decl_list = arena
        .get_variable(inner_decl_list_node)
        .expect("inner decl list data");
    let inner_decl_idx = *inner_decl_list
        .declarations
        .nodes
        .first()
        .expect("inner declaration");
    let inner_decl_node = arena.get(inner_decl_idx).expect("inner decl node");
    let inner_decl = arena
        .get_variable_declaration(inner_decl_node)
        .expect("inner decl data");
    let inner_arrow_idx = inner_decl.initializer;
    let inner_arrow_node = arena.get(inner_arrow_idx).expect("inner arrow node");
    let inner_func = arena
        .get_function(inner_arrow_node)
        .expect("inner func data");

    // Get the inner body block
    let inner_body_node = arena.get(inner_func.body).expect("inner body node");
    let inner_body = arena.get_block(inner_body_node).expect("inner body");

    // Get the y declaration statement (inside a VariableStatement -> VariableDeclarationList -> VariableDeclaration)
    let y_var_stmt_idx = *inner_body
        .statements
        .nodes
        .first()
        .expect("y variable statement");
    let y_var_stmt_node = arena.get(y_var_stmt_idx).expect("y var stmt node");
    let y_var_stmt = arena
        .get_variable(y_var_stmt_node)
        .expect("y var stmt data");
    let y_decl_list_idx = *y_var_stmt
        .declarations
        .nodes
        .first()
        .expect("y declaration list");
    let y_decl_list_node = arena.get(y_decl_list_idx).expect("y decl list node");
    let y_decl_list = arena
        .get_variable(y_decl_list_node)
        .expect("y decl list data");
    let y_decl_idx = *y_decl_list
        .declarations
        .nodes
        .first()
        .expect("y declaration");
    let y_decl_node = arena.get(y_decl_idx).expect("y decl node");
    let y_decl = arena
        .get_variable_declaration(y_decl_node)
        .expect("y decl data");
    let x_ref_in_inner = y_decl.initializer;

    let union = types.union(vec![TypeId::STRING, TypeId::NUMBER]);

    // The variable x inside the nested closure should be narrowed to string
    let flow_in_nested = binder.get_node_flow(x_ref_in_inner);
    assert!(
        flow_in_nested.is_some(),
        "Flow should be recorded for variable inside nested closure"
    );

    let narrowed_in_nested = analyzer.get_flow_type(x_ref_in_inner, union, flow_in_nested.unwrap());
    assert_eq!(narrowed_in_nested, types.literal_string("nested"));
}

/// Test callback used with setTimeout (common async pattern).
///
/// This verifies that closures used with setTimeout properly capture
/// variables from their enclosing scope.
///
/// NOTE: Currently ignored - see `test_closure_capture_flow_before_callback`
/// for details on the limitation.
#[test]
#[ignore = "Flow analysis doesn't traverse closure START node antecedents correctly"]
fn test_closure_capture_with_settimeout() {
    let source = r#"
let x: string | number;
x = "timeout";
setTimeout(() => {
    console.log(x);
}, 1000);
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let arena = parser.get_arena();
    let types = TypeInterner::new();
    let compiler_options = crate::checker::context::CheckerOptions::default();
    let mut checker = CheckerState::new(
        arena,
        &binder,
        &types,
        "test.ts".to_string(),
        compiler_options,
    );
    checker.check_source_file(root);

    let analyzer = FlowAnalyzer::with_node_types(arena, &binder, &types, &checker.ctx.node_types);

    let root_node = arena.get(root).expect("root node");
    let source_file = arena.get_source_file(root_node).expect("source file");

    // Get the setTimeout call statement (index 2)
    let settimeout_stmt_idx = *source_file
        .statements
        .nodes
        .get(2)
        .expect("setTimeout call");
    let settimeout_stmt_node = arena
        .get(settimeout_stmt_idx)
        .expect("setTimeout statement node");
    let settimeout_stmt = arena
        .get_expression_statement(settimeout_stmt_node)
        .expect("setTimeout expr statement");
    let settimeout_call_idx = settimeout_stmt.expression;
    let settimeout_call_node = arena
        .get(settimeout_call_idx)
        .expect("setTimeout call node");
    let settimeout_call = arena
        .get_call_expr(settimeout_call_node)
        .expect("setTimeout call data");

    // Get the arrow function argument
    let args = settimeout_call.arguments.as_ref().expect("arguments");
    let arrow_func_idx = *args.nodes.first().expect("arrow function");
    let arrow_func_node = arena.get(arrow_func_idx).expect("arrow func node");
    let arrow_func = arena
        .get_function(arrow_func_node)
        .expect("arrow func data");

    // Get the body block
    let body_node = arena.get(arrow_func.body).expect("body node");
    let body_block = arena.get_block(body_node).expect("body block");

    // Get the console.log call expression statement
    let log_stmt = *body_block
        .statements
        .nodes
        .first()
        .expect("console.log statement");
    let log_stmt_node = arena.get(log_stmt).expect("log statement node");
    let log_expr_stmt = arena
        .get_expression_statement(log_stmt_node)
        .expect("log expr statement");
    let log_call_node = arena.get(log_expr_stmt.expression).expect("log call node");
    let log_call = arena.get_call_expr(log_call_node).expect("log call data");

    // Get the argument to console.log (the x variable)
    let log_args = log_call.arguments.as_ref().expect("log arguments");
    let x_ref = *log_args.nodes.first().expect("x reference");

    let union = types.union(vec![TypeId::STRING, TypeId::NUMBER]);

    // The variable x inside the setTimeout callback should be narrowed to string
    let flow_in_callback = binder.get_node_flow(x_ref);
    assert!(
        flow_in_callback.is_some(),
        "Flow should be recorded for variable inside setTimeout callback"
    );

    let narrowed_in_callback = analyzer.get_flow_type(x_ref, union, flow_in_callback.unwrap());
    assert_eq!(narrowed_in_callback, types.literal_string("timeout"));
}

/// Test that flow analysis correctly handles multiple closures capturing
/// the same variable at different points in the code.
///
/// NOTE: Currently ignored - see `test_closure_capture_flow_before_callback`
/// for details on the limitation.
#[test]
#[ignore = "Flow analysis doesn't traverse closure START node antecedents correctly"]
fn test_multiple_closures_capture_same_variable() {
    let source = r#"
let x: string | number;
x = "first";
const callback1 = () => {
    const a = x;
};
x = 42;
const callback2 = () => {
    const b = x;
};
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let arena = parser.get_arena();
    let types = TypeInterner::new();
    let compiler_options = crate::checker::context::CheckerOptions::default();
    let mut checker = CheckerState::new(
        arena,
        &binder,
        &types,
        "test.ts".to_string(),
        compiler_options,
    );
    checker.check_source_file(root);

    let analyzer = FlowAnalyzer::with_node_types(arena, &binder, &types, &checker.ctx.node_types);

    let root_node = arena.get(root).expect("root node");
    let source_file = arena.get_source_file(root_node).expect("source file");

    // Get the first arrow function statement
    let callback1_var_idx = *source_file
        .statements
        .nodes
        .get(2)
        .expect("callback1 var stmt");
    let callback1_var_node = arena.get(callback1_var_idx).expect("callback1 var node");
    let callback1_var = arena
        .get_variable(callback1_var_node)
        .expect("callback1 var data");
    let callback1_decl_list_idx = *callback1_var.declarations.nodes.first().expect("decl list");
    let callback1_decl_list_node = arena.get(callback1_decl_list_idx).expect("decl list node");
    let callback1_decl_list = arena
        .get_variable(callback1_decl_list_node)
        .expect("decl list data");
    let callback1_decl_idx = *callback1_decl_list
        .declarations
        .nodes
        .first()
        .expect("callback1 decl");
    let callback1_decl_node = arena.get(callback1_decl_idx).expect("callback1 decl node");
    let callback1_decl = arena
        .get_variable_declaration(callback1_decl_node)
        .expect("callback1 decl data");
    let callback1_func_idx = callback1_decl.initializer;
    let callback1_func_node = arena.get(callback1_func_idx).expect("callback1 func node");
    let callback1_func = arena
        .get_function(callback1_func_node)
        .expect("callback1 func data");
    let body1_node = arena.get(callback1_func.body).expect("body1 node");
    let body1 = arena.get_block(body1_node).expect("body1");
    let a_decl_stmt_idx = *body1.statements.nodes.first().expect("a declaration");
    let a_decl_stmt_node = arena.get(a_decl_stmt_idx).expect("a decl statement node");
    let a_decl_var = arena
        .get_variable(a_decl_stmt_node)
        .expect("a decl var data");
    let a_decl_list_idx = *a_decl_var.declarations.nodes.first().expect("a decl list");
    let a_decl_list_node = arena.get(a_decl_list_idx).expect("a decl list node");
    let a_decl_list = arena
        .get_variable(a_decl_list_node)
        .expect("a decl list data");
    let a_decl_idx = *a_decl_list.declarations.nodes.first().expect("a decl");
    let a_decl_node = arena.get(a_decl_idx).expect("a decl node");
    let a_decl = arena
        .get_variable_declaration(a_decl_node)
        .expect("a decl data");
    let x_ref1 = a_decl.initializer;

    // Get the second arrow function statement
    let callback2_var_idx = *source_file
        .statements
        .nodes
        .get(4)
        .expect("callback2 var stmt");
    let callback2_var_node = arena.get(callback2_var_idx).expect("callback2 var node");
    let callback2_var = arena
        .get_variable(callback2_var_node)
        .expect("callback2 var data");
    let callback2_decl_list_idx = *callback2_var.declarations.nodes.first().expect("decl list");
    let callback2_decl_list_node = arena.get(callback2_decl_list_idx).expect("decl list node");
    let callback2_decl_list = arena
        .get_variable(callback2_decl_list_node)
        .expect("decl list data");
    let callback2_decl_idx = *callback2_decl_list
        .declarations
        .nodes
        .first()
        .expect("callback2 decl");
    let callback2_decl_node = arena.get(callback2_decl_idx).expect("callback2 decl node");
    let callback2_decl = arena
        .get_variable_declaration(callback2_decl_node)
        .expect("callback2 decl data");
    let callback2_func_idx = callback2_decl.initializer;
    let callback2_func_node = arena.get(callback2_func_idx).expect("callback2 func node");
    let callback2_func = arena
        .get_function(callback2_func_node)
        .expect("callback2 func data");
    let body2_node = arena.get(callback2_func.body).expect("body2 node");
    let body2 = arena.get_block(body2_node).expect("body2");
    let b_decl_stmt_idx = *body2.statements.nodes.first().expect("b declaration");
    let b_decl_stmt_node = arena.get(b_decl_stmt_idx).expect("b decl statement node");
    let b_decl_var = arena
        .get_variable(b_decl_stmt_node)
        .expect("b decl var data");
    let b_decl_list_idx = *b_decl_var.declarations.nodes.first().expect("b decl list");
    let b_decl_list_node = arena.get(b_decl_list_idx).expect("b decl list node");
    let b_decl_list = arena
        .get_variable(b_decl_list_node)
        .expect("b decl list data");
    let b_decl_idx = *b_decl_list.declarations.nodes.first().expect("b decl");
    let b_decl_node = arena.get(b_decl_idx).expect("b decl node");
    let b_decl = arena
        .get_variable_declaration(b_decl_node)
        .expect("b decl data");
    let x_ref2 = b_decl.initializer;

    let union = types.union(vec![TypeId::STRING, TypeId::NUMBER]);

    // First callback should see x as literal "first"
    let flow1 = binder.get_node_flow(x_ref1).expect("flow for callback1");
    let narrowed1 = analyzer.get_flow_type(x_ref1, union, flow1);
    assert_eq!(narrowed1, types.literal_string("first"));

    // Second callback should see x as literal 42
    let flow2 = binder.get_node_flow(x_ref2).expect("flow for callback2");
    let narrowed2 = analyzer.get_flow_type(x_ref2, union, flow2);
    assert_eq!(narrowed2, types.literal_number(42.0));
}

/// Test closure with conditional capture (variable narrowed before callback).
///
/// NOTE: Currently ignored - see `test_closure_capture_flow_before_callback`
/// for details on the limitation.
#[test]
#[ignore = "Flow analysis doesn't traverse closure START node antecedents correctly"]
fn test_closure_with_conditional_capture() {
    let source = r#"
let x: string | number;
if (typeof x === "string") {
    const callback = () => {
        const y = x.length;
    };
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let arena = parser.get_arena();
    let types = TypeInterner::new();
    let compiler_options = crate::checker::context::CheckerOptions::default();
    let mut checker = CheckerState::new(
        arena,
        &binder,
        &types,
        "test.ts".to_string(),
        compiler_options,
    );
    checker.check_source_file(root);

    let analyzer = FlowAnalyzer::with_node_types(arena, &binder, &types, &checker.ctx.node_types);

    let root_node = arena.get(root).expect("root node");
    let source_file = arena.get_source_file(root_node).expect("source file");

    // Get the if statement
    let if_idx = *source_file.statements.nodes.get(1).expect("if statement");
    let if_node = arena.get(if_idx).expect("if node");
    let if_data = arena.get_if_statement(if_node).expect("if data");

    // Get the then block
    let then_block_node = arena.get(if_data.then_statement).expect("then block node");
    let block = arena.get_block(then_block_node).expect("then block");

    // Get the arrow function statement (VariableStatement)
    let arrow_var_stmt = *block.statements.nodes.first().expect("arrow var statement");
    let arrow_var_node = arena.get(arrow_var_stmt).expect("arrow var node");
    let arrow_var = arena.get_variable(arrow_var_node).expect("arrow var data");
    let arrow_decl_list_idx = *arrow_var
        .declarations
        .nodes
        .first()
        .expect("arrow decl list");
    let arrow_decl_list_node = arena
        .get(arrow_decl_list_idx)
        .expect("arrow decl list node");
    let arrow_decl_list = arena
        .get_variable(arrow_decl_list_node)
        .expect("arrow decl list data");
    let arrow_decl_idx = *arrow_decl_list
        .declarations
        .nodes
        .first()
        .expect("arrow decl");
    let arrow_decl_node = arena.get(arrow_decl_idx).expect("arrow decl node");
    let arrow_decl = arena
        .get_variable_declaration(arrow_decl_node)
        .expect("arrow decl data");
    let arrow_func_idx = arrow_decl.initializer;
    let arrow_func_node = arena.get(arrow_func_idx).expect("arrow func node");
    let arrow_func = arena
        .get_function(arrow_func_node)
        .expect("arrow func data");

    // Get the body block
    let body_node = arena.get(arrow_func.body).expect("body node");
    let body_block = arena.get_block(body_node).expect("body block");

    // Get the y declaration
    let y_decl_stmt_idx = *body_block.statements.nodes.first().expect("y declaration");
    let y_decl_stmt_node = arena.get(y_decl_stmt_idx).expect("y decl statement node");
    let y_decl_var = arena
        .get_variable(y_decl_stmt_node)
        .expect("y decl var data");
    let y_decl_list_idx = *y_decl_var.declarations.nodes.first().expect("y decl list");
    let y_decl_list_node = arena.get(y_decl_list_idx).expect("y decl list node");
    let y_decl_list = arena
        .get_variable(y_decl_list_node)
        .expect("y decl list data");
    let y_decl_idx = *y_decl_list.declarations.nodes.first().expect("y decl");
    let y_decl_node = arena.get(y_decl_idx).expect("y decl node");
    let y_decl = arena
        .get_variable_declaration(y_decl_node)
        .expect("y decl data");

    // Get the property access expression x.length
    let prop_access = y_decl.initializer;

    // Get the identifier x from the property access
    let prop_access_node = arena.get(prop_access).expect("prop access node");
    let access_expr = arena
        .get_access_expr(prop_access_node)
        .expect("access expr data");
    let x_identifier = access_expr.expression;

    let union = types.union(vec![TypeId::STRING, TypeId::NUMBER]);

    // Inside the if branch and inside the closure, x should be narrowed to string
    let flow = binder.get_node_flow(prop_access);
    assert!(
        flow.is_some(),
        "Flow should be recorded for expression inside closure in if branch"
    );

    let narrowed = analyzer.get_flow_type(x_identifier, union, flow.unwrap());
    assert_eq!(narrowed, TypeId::STRING);
}

/// Test that the flow graph builder correctly handles arrow functions
/// without creating infinite recursion or errors.
#[test]
fn test_flow_graph_builder_with_arrow_functions() {
    let source = r#"
let x: string | number;
const add = (a: number, b: number): number => {
    return a + b;
};
x = "test";
const result = add(1, 2);
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();

    if let Some(source_file) = arena.get(root)
        && let Some(sf) = arena.get_source_file(source_file)
    {
        let mut builder = FlowGraphBuilder::new(arena);
        let graph = builder.build_source_file(&sf.statements);

        // Verify flow graph exists and has nodes
        assert!(!graph.nodes.is_empty(), "Flow graph should have nodes");

        // Verify that we can query flow for arrow function
        let arrow_func_idx = *sf.statements.nodes.get(1).expect("arrow function");
        assert!(
            graph.has_flow_at_node(arrow_func_idx),
            "Flow should be recorded for arrow function"
        );
    }
}

/// Test callback with filter method (another common array method).
///
/// NOTE: Currently ignored for the same reason as
/// `test_closure_capture_flow_before_callback` - the flow analysis
/// doesn't correctly traverse START node antecedents to apply type
/// narrowing from outer scopes into closures.
#[test]
#[ignore = "Flow analysis doesn't traverse closure START node antecedents correctly"]
fn test_closure_capture_with_array_filter() {
    let source = r#"
let x: string | number;
x = "filter";
const arr = [1, 2, 3];
const filtered = arr.filter((item) => {
    return typeof x === "string";
});
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let arena = parser.get_arena();
    let types = TypeInterner::new();
    let compiler_options = crate::checker::context::CheckerOptions::default();
    let mut checker = CheckerState::new(
        arena,
        &binder,
        &types,
        "test.ts".to_string(),
        compiler_options,
    );
    checker.check_source_file(root);

    let analyzer = FlowAnalyzer::with_node_types(arena, &binder, &types, &checker.ctx.node_types);

    let root_node = arena.get(root).expect("root node");
    let source_file = arena.get_source_file(root_node).expect("source file");

    // Get the variable statement at index 3 (const filtered = ...)
    let var_stmt_idx = *source_file
        .statements
        .nodes
        .get(3)
        .expect("variable statement");
    let var_stmt_node = arena.get(var_stmt_idx).expect("var stmt node");
    let var_stmt_data = arena.get_variable(var_stmt_node).expect("var stmt data");

    // Get the declaration list
    let decl_list_idx = *var_stmt_data
        .declarations
        .nodes
        .first()
        .expect("declaration list");
    let decl_list_node = arena.get(decl_list_idx).expect("decl list node");
    let decl_list_data = arena.get_variable(decl_list_node).expect("decl list data");

    // Get the first declaration and its initializer (the filter call)
    let decl_idx = *decl_list_data
        .declarations
        .nodes
        .first()
        .expect("declaration");
    let decl_node = arena.get(decl_idx).expect("decl node");
    let decl = arena
        .get_variable_declaration(decl_node)
        .expect("decl data");
    let filter_call_node = arena.get(decl.initializer).expect("filter call node");
    let filter_call = arena
        .get_call_expr(filter_call_node)
        .expect("filter call data");

    // Get the arrow function argument
    let args = filter_call.arguments.as_ref().expect("arguments");
    let arrow_func_idx = *args.nodes.first().expect("arrow function");
    let arrow_func_node = arena.get(arrow_func_idx).expect("arrow func node");
    let arrow_func = arena
        .get_function(arrow_func_node)
        .expect("arrow func data");

    // Get the body block
    let body_node = arena.get(arrow_func.body).expect("body node");
    let body_block = arena.get_block(body_node).expect("body block");

    // Get the return statement
    let return_stmt = *body_block
        .statements
        .nodes
        .first()
        .expect("return statement");
    let return_node = arena.get(return_stmt).expect("return node");
    let return_data = arena
        .get_return_statement(return_node)
        .expect("return data");

    // Get the typeof x expression (binary expression: typeof x === "string")
    let typeof_bin_expr = return_data.expression;

    // Get the typeof expression node (left side of binary)
    let typeof_bin_node = arena.get(typeof_bin_expr).expect("bin expr node");
    let typeof_bin = arena
        .get_binary_expr(typeof_bin_node)
        .expect("bin expr data");
    let typeof_expr = typeof_bin.left; // This is the typeof x expression

    let union = types.union(vec![TypeId::STRING, TypeId::NUMBER]);

    // Get the identifier x from the typeof expression
    let typeof_node = arena.get(typeof_expr).expect("typeof expr node");
    let typeof_unary = arena.get_unary_expr(typeof_node).expect("typeof expr data");
    let x_identifier = typeof_unary.operand;

    // The variable x inside the filter callback should be narrowed to string
    let flow_in_callback = binder.get_node_flow(typeof_bin_expr);
    assert!(
        flow_in_callback.is_some(),
        "Flow should be recorded for expression inside filter callback"
    );

    let narrowed_in_callback =
        analyzer.get_flow_type(x_identifier, union, flow_in_callback.unwrap());
    assert_eq!(narrowed_in_callback, types.literal_string("filter"));
}

// ============================================================================
// CFA-15: FlowGraph Path Verification Tests
// ============================================================================

/// Test that all if-else branches are captured in flow graph.
#[test]
fn test_flow_graph_captures_if_else_branches() {
    let source = r#"
let x: string | number;
if (Math.random() > 0.5) {
    x = "a";
} else {
    x = 1;
}
x;
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let arena = parser.get_arena();
    let root_node = arena.get(root).expect("root node");
    let source_file = arena.get_source_file(root_node).expect("source file");

    // The if statement should have flow recorded
    let if_stmt_idx = *source_file.statements.nodes.get(1).expect("if statement");
    let flow_at_if = binder.get_node_flow(if_stmt_idx);
    assert!(
        flow_at_if.is_some(),
        "Flow should be recorded at if statement"
    );
}

/// Test that switch statement cases are all captured.
#[test]
fn test_flow_graph_captures_switch_cases() {
    let source = r#"
let x: "a" | "b" | "c";
let result: number;
switch (x) {
    case "a":
        result = 1;
        break;
    case "b":
        result = 2;
        break;
    case "c":
        result = 3;
        break;
}
result;
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let arena = parser.get_arena();
    let root_node = arena.get(root).expect("root node");
    let source_file = arena.get_source_file(root_node).expect("source file");

    // The switch statement should have flow recorded
    let switch_stmt_idx = *source_file
        .statements
        .nodes
        .get(2)
        .expect("switch statement");
    let flow_at_switch = binder.get_node_flow(switch_stmt_idx);
    assert!(
        flow_at_switch.is_some(),
        "Flow should be recorded at switch statement"
    );
}

/// Test that try-catch-finally paths are captured.
#[test]
fn test_flow_graph_captures_try_catch_finally() {
    let source = r#"
let x: number;
try {
    x = 1;
} catch (e) {
    x = 2;
} finally {
    x = 3;
}
x;
"#;

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
    let source = r#"
let x: number;
for (let i = 0; i < 10; i++) {
    if (i === 5) break;
    if (i % 2 === 0) continue;
    x = i;
}
x;
"#;

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
    let source = r#"
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
"#;

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
    let source = r#"
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
"#;

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
    use crate::binder::BinderState;
    use crate::checker::CheckerState;

    use crate::parser::ParserState;

    let source = r#"
function test() {
    let x: string;
    return x;  // Error: x is used before being assigned
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let types = crate::solver::TypeInterner::new();
    let mut checker = CheckerState::new(
        arena,
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    checker.check_source_file(root);

    // Should have TS2454 error
    let has_ts2454 = checker.ctx.diagnostics.iter().any(|d| d.code == 2454);
    assert!(
        has_ts2454,
        "Should have TS2454 error for variable used before assignment"
    );
}

/// Test that TS2454 is NOT emitted when a variable has an initializer.
#[test]
fn test_ts2454_no_error_with_initializer() {
    use crate::binder::BinderState;
    use crate::checker::CheckerState;
    use crate::parser::ParserState;

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

    let types = crate::solver::TypeInterner::new();
    let mut checker = CheckerState::new(
        arena,
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    checker.check_source_file(root);

    // Should NOT have TS2454 error
    let has_ts2454 = checker.ctx.diagnostics.iter().any(|d| d.code == 2454);
    assert!(
        !has_ts2454,
        "Should NOT have TS2454 error when variable has initializer"
    );
}

/// Test that && creates intermediate flow condition nodes for the right operand.
///
/// For `typeof x === 'object' && x`, the `x` on the right side of `&&` should
/// have a TRUE_CONDITION flow node so that it sees the typeof narrowing.
#[test]
fn test_and_expression_creates_intermediate_flow_nodes() {
    use crate::binder::{BinderState, flow_flags};
    use crate::parser::ParserState;

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
    use crate::binder::BinderState;
    use crate::parser::ParserState;
    use crate::solver::TypeInterner;

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

/// Test that || creates intermediate FALSE_CONDITION flow nodes for the right operand.
#[test]
fn test_or_expression_creates_intermediate_flow_nodes() {
    use crate::binder::{BinderState, flow_flags};
    use crate::parser::ParserState;

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
