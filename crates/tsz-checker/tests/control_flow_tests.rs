use super::FlowAnalyzer;
use crate::CheckerState;
use crate::flow_graph_builder::FlowGraphBuilder;
use tsz_binder::BinderState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;
use tsz_solver::construction::TypeInterner;
use tsz_solver::{PropertyInfo, TypeId, Visibility};
fn parse_test_source(source: &str) -> (tsz_parser::ParserState, tsz_parser::parser::NodeIndex) {
    let mut parser = tsz_parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    (parser, root)
}

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
    assert!(branch_idx.is_some(), "missing branch statement");
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

fn get_function_if_branch_expression(
    arena: &NodeArena,
    root: NodeIndex,
    fn_index: usize,
    body_stmt_index: usize,
    is_then: bool,
) -> NodeIndex {
    let root_node = arena.get(root).expect("root node");
    let source_file = arena.get_source_file(root_node).expect("source file");
    let fn_idx = *source_file
        .statements
        .nodes
        .get(fn_index)
        .expect("function statement");
    let fn_node = arena.get(fn_idx).expect("function node");
    let fn_data = arena.get_function(fn_node).expect("function data");
    let body_idx = fn_data.body;
    let body_node = arena.get(body_idx).expect("function body");
    let body = arena.get_block(body_node).expect("function body block");
    let if_idx = *body
        .statements
        .nodes
        .get(body_stmt_index)
        .expect("if statement");
    let if_node = arena.get(if_idx).expect("if node");
    let if_data = arena.get_if_statement(if_node).expect("if data");
    let branch_idx = if is_then {
        if_data.then_statement
    } else {
        if_data.else_statement
    };
    assert!(branch_idx.is_some(), "missing branch statement");
    extract_expression_from_statement(arena, branch_idx)
}

fn get_statement_expression(arena: &NodeArena, root: NodeIndex, stmt_index: usize) -> NodeIndex {
    let root_node = arena.get(root).expect("root node");
    let source_file = arena.get_source_file(root_node).expect("source file");
    let stmt_idx = *source_file
        .statements
        .nodes
        .get(stmt_index)
        .expect("statement");
    extract_expression_from_statement(arena, stmt_idx)
}

fn get_function_body_statement_expression(
    arena: &NodeArena,
    root: NodeIndex,
    fn_index: usize,
    stmt_index: usize,
) -> NodeIndex {
    let root_node = arena.get(root).expect("root node");
    let source_file = arena.get_source_file(root_node).expect("source file");
    let fn_idx = *source_file
        .statements
        .nodes
        .get(fn_index)
        .expect("function statement");
    let fn_node = arena.get(fn_idx).expect("function node");
    let function = arena.get_function(fn_node).expect("function data");
    let body_node = arena.get(function.body).expect("function body node");
    let body = arena.get_block(body_node).expect("function body");
    let stmt_idx = *body
        .statements
        .nodes
        .get(stmt_index)
        .expect("body statement");
    extract_expression_from_statement(arena, stmt_idx)
}

fn get_method_call_receiver_identifier(
    arena: &NodeArena,
    root: NodeIndex,
    stmt_index: usize,
) -> NodeIndex {
    let expr_idx = get_statement_expression(arena, root, stmt_index);
    let call_node = arena.get(expr_idx).expect("call node");
    let call = arena.get_call_expr(call_node).expect("call expr");
    let callee_node = arena.get(call.expression).expect("callee node");
    let access = arena.get_access_expr(callee_node).expect("callee access");
    access.expression
}

// Split into under-cap shards to satisfy the 2000-line limit (CLAUDE.md §19).
// Each shard contains a contiguous slice of control_flow_tests tests.
include!("control_flow_tests_parts/part_00.rs");
include!("control_flow_tests_parts/part_01.rs");
include!("control_flow_tests_parts/part_02.rs");
