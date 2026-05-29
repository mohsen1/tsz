//! Comprehensive parser unit tests covering operator precedence, arrow functions,
//! type syntax, declarations, class syntax, statements, and error recovery.

use crate::parser::node::NodeArena;
use crate::parser::node_flags;
use crate::parser::node_view::NodeAccess;
use crate::parser::syntax_kind_ext;
use crate::parser::test_fixture::{parse_source, parse_source_named};
use crate::parser::{NodeIndex, ParserState};
use tsz_common::diagnostics::diagnostic_codes;
use tsz_scanner::SyntaxKind;

// =============================================================================
// Helpers
// =============================================================================

fn assert_no_errors(parser: &ParserState, context: &str) {
    let diags = parser.get_diagnostics();
    assert!(
        diags.is_empty(),
        "{context}: expected no errors, got {}: {:?}",
        diags.len(),
        diags.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

fn assert_has_errors(parser: &ParserState, context: &str) {
    assert!(
        !parser.get_diagnostics().is_empty(),
        "{context}: expected errors but got none"
    );
}

fn get_first_statement(arena: &NodeArena, root: NodeIndex) -> NodeIndex {
    let sf = arena.get_source_file_at(root).expect("missing source file");
    assert!(
        !sf.statements.nodes.is_empty(),
        "expected at least one statement"
    );
    sf.statements.nodes[0]
}

fn get_statements(arena: &NodeArena, root: NodeIndex) -> Vec<NodeIndex> {
    let sf = arena.get_source_file_at(root).expect("missing source file");
    sf.statements.nodes.clone()
}

fn get_first_variable_declaration(arena: &NodeArena, root: NodeIndex) -> NodeIndex {
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt node");
    let var_stmt = arena.get_variable(stmt_node).expect("variable statement");
    let decl_list_idx = var_stmt.declarations.nodes[0];
    let decl_list_node = arena.get(decl_list_idx).expect("var decl list node");
    let decl_list = arena
        .get_variable(decl_list_node)
        .expect("variable declaration list");
    decl_list.declarations.nodes[0]
}

/// For `const x = <expr>;` or `let x = <expr>;`, extract the initializer expression.
/// Structure: `VARIABLE_STATEMENT` -> [`VARIABLE_DECLARATION_LIST`] -> [`VARIABLE_DECLARATION`, ...]
fn get_var_initializer(arena: &NodeArena, root: NodeIndex) -> NodeIndex {
    let decl_idx = get_first_variable_declaration(arena, root);
    let decl_node = arena.get(decl_idx).expect("var decl node");
    let decl = arena
        .get_variable_declaration(decl_node)
        .expect("var decl data");
    decl.initializer
}

fn get_first_function_var_initializer(arena: &NodeArena, root: NodeIndex) -> NodeIndex {
    let func_idx = get_first_statement(arena, root);
    let func_node = arena.get(func_idx).expect("function decl");
    let func = arena
        .get_function(func_node)
        .expect("function declaration data");
    let body_node = arena.get(func.body).expect("function body");
    let block = arena.get_block(body_node).expect("block data");
    let var_stmt_node = arena
        .get(block.statements.nodes[0])
        .expect("variable statement");
    let var_stmt = arena.get_variable(var_stmt_node).expect("variable data");
    let decl_list_node = arena
        .get(var_stmt.declarations.nodes[0])
        .expect("declaration list");
    let decl_list = arena
        .get_variable(decl_list_node)
        .expect("declaration-list data");
    let decl_node = arena
        .get(decl_list.declarations.nodes[0])
        .expect("declaration");
    let decl = arena
        .get_variable_declaration(decl_node)
        .expect("variable declaration data");
    decl.initializer
}

/// For `<expr>;` at the top level, extract the expression of the first
/// statement (which must be an expression statement).
fn get_first_expression_statement_expr(arena: &NodeArena, root: NodeIndex) -> NodeIndex {
    let stmt_idx = get_first_statement(arena, root);
    let stmt_node = arena.get(stmt_idx).expect("stmt");
    let expr_stmt = arena
        .get_expression_statement(stmt_node)
        .expect("expression statement");
    expr_stmt.expression
}

fn get_var_type_annotation(arena: &NodeArena, root: NodeIndex) -> NodeIndex {
    let decl_idx = get_first_variable_declaration(arena, root);
    let decl_node = arena.get(decl_idx).expect("var decl node");
    let decl = arena
        .get_variable_declaration(decl_node)
        .expect("var decl data");
    decl.type_annotation
}

fn node_text<'a>(arena: &NodeArena, source: &'a str, idx: NodeIndex) -> &'a str {
    let node = arena.get(idx).expect("node");
    &source[node.pos as usize..node.end as usize]
}

/// For a binary expression node, get its data.
fn get_binary(arena: &NodeArena, idx: NodeIndex) -> (NodeIndex, u16, NodeIndex) {
    let node = arena.get(idx).expect("node");
    let bin = arena.get_binary_expr(node).expect("binary expr data");
    (bin.left, bin.operator_token, bin.right)
}

// =============================================================================
// 1. Operator Precedence Tests (15+ tests)
// =============================================================================

// Split into under-cap shards to satisfy the 2000-line limit (CLAUDE.md §19).
// Each shard contains a contiguous slice of parser_unit_tests tests.
include!("parser_unit_tests_parts/part_00.rs");
include!("parser_unit_tests_parts/part_01.rs");
include!("parser_unit_tests_parts/part_02.rs");
