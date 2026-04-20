//! Tests for Checker - Type checker using `NodeArena` and Solver
//!
//! This module contains comprehensive type checking tests organized into categories:
//! - Basic type checking (creation, intrinsic types, type interning)
//! - Type compatibility and assignability
//! - Excess property checking
//! - Function overloads and call resolution
//! - Generic types and type inference
//! - Control flow analysis
//! - Error diagnostics
use crate::binder::BinderState;
use crate::checker::state::CheckerState;
use crate::parser::ParserState;
use crate::parser::node::NodeArena;
use crate::test_fixtures::{TestContext, merge_shared_lib_symbols, setup_lib_contexts};
use tsz_solver::{TypeId, TypeInterner, Visibility, types::RelationCacheKey, types::TypeData};

// =============================================================================
// Basic Type Checker Tests
// =============================================================================
#[test]
fn test_namespace_value_member_alias_missing_error() {
    use crate::parser::ParserState;

    let source = r#"
namespace Outer {
    export namespace Inner {
        export const value = 1;
    }
}
import Alias = Outer.Inner;
const ok = Alias.value;
const bad = Alias.missing;
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    let missing_count = codes.iter().filter(|&&code| code == 2339).count();
    assert_eq!(
        missing_count, 1,
        "Expected one 2339 error for missing namespace alias member, got: {codes:?}"
    );

    let ok_sym = binder.file_locals.get("ok").expect("ok should exist");
    // For const literals, we get literal types
    let literal_1 = types.literal_number(1.0);
    assert_eq!(checker.get_type_of_symbol(ok_sym), literal_1);
}

#[test]
fn test_nested_namespace_value_member_missing_error() {
    use crate::parser::ParserState;

    let source = r#"
namespace Outer {
    export namespace Inner {
        export const ok = 1;
    }
}
const okValue = Outer.Inner.ok;
const badValue = Outer.Inner.missing;
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    let missing_count = codes.iter().filter(|&&code| code == 2339).count();
    assert_eq!(
        missing_count, 1,
        "Expected one 2339 error for missing nested namespace value member, got: {codes:?}"
    );

    let ok_sym = binder
        .file_locals
        .get("okValue")
        .expect("okValue should exist");
    // For const literals, we get literal types
    let literal_1 = types.literal_number(1.0);
    assert_eq!(checker.get_type_of_symbol(ok_sym), literal_1);
}

#[test]
fn test_namespace_value_member_not_exported_error() {
    use crate::parser::ParserState;

    let source = r#"
namespace NS {
    export const ok = 1;
    const hidden = 2;
}
const ok = NS.ok;
const bad = NS.hidden;
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    let missing_count = codes.iter().filter(|&&code| code == 2339).count();
    assert_eq!(
        missing_count, 1,
        "Expected one 2339 error for non-exported namespace value member, got: {codes:?}"
    );

    let ok_sym = binder.file_locals.get("ok").expect("ok should exist");
    // For const literals, we get literal types
    let literal_1 = types.literal_number(1.0);
    assert_eq!(checker.get_type_of_symbol(ok_sym), literal_1);
}

#[test]
fn test_deep_binary_expression_type_check() {
    use crate::parser::ParserState;

    const COUNT: usize = 50000;
    let mut source = String::with_capacity(COUNT * 4);
    for i in 0..COUNT {
        if i > 0 {
            source.push_str(" + ");
        }
        source.push('0');
    }
    source.push(';');

    let mut parser = ParserState::new("test.ts".to_string(), source);
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    assert!(checker.ctx.diagnostics.is_empty());
}

#[test]
fn test_scoped_identifier_resolution_uses_binder_scopes() {
    use crate::parser::ParserState;
    use crate::parser::syntax_kind_ext;

    let source = r#"
let x = 1;
{
    let x = "hi";
    x;
}
x;
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let root_node = arena.get(root).expect("root node");
    let source_file = arena.get_source_file(root_node).expect("source file");

    let block_idx = source_file
        .statements
        .nodes
        .iter()
        .copied()
        .find(|&idx| {
            arena
                .get(idx)
                .is_some_and(|node| node.kind == syntax_kind_ext::BLOCK)
        })
        .expect("block statement");
    let block = arena
        .get_block(arena.get(block_idx).expect("block node"))
        .expect("block data");
    let inner_expr_idx = block
        .statements
        .nodes
        .iter()
        .copied()
        .find(|&idx| {
            arena
                .get(idx)
                .is_some_and(|node| node.kind == syntax_kind_ext::EXPRESSION_STATEMENT)
        })
        .expect("inner expression statement");
    let inner_expr = arena
        .get_expression_statement(arena.get(inner_expr_idx).expect("inner expr node"))
        .expect("inner expression data");

    let outer_expr_idx = source_file
        .statements
        .nodes
        .iter()
        .copied()
        .find(|&idx| {
            arena
                .get(idx)
                .is_some_and(|node| node.kind == syntax_kind_ext::EXPRESSION_STATEMENT)
        })
        .expect("outer expression statement");
    let outer_expr = arena
        .get_expression_statement(arena.get(outer_expr_idx).expect("outer expr node"))
        .expect("outer expression data");

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena,
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let inner_type = checker.get_type_of_node(inner_expr.expression);
    let outer_type = checker.get_type_of_node(outer_expr.expression);

    assert_eq!(inner_type, TypeId::STRING);
    assert_eq!(outer_type, TypeId::NUMBER);
}

/// Test that flow narrowing applies in if branches
///
/// NOTE: Currently ignored - flow narrowing in conditional branches is not fully
/// implemented. The flow analysis doesn't correctly apply type narrowing from
/// typeof/type guards in if statements and for loops.
#[test]
fn test_flow_narrowing_applies_in_if_branch() {
    use crate::parser::ParserState;
    use crate::parser::syntax_kind_ext;

    let source = r#"
let x: string | number;
if (typeof x === "string") {
    x;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let root_node = arena.get(root).expect("root node");
    let source_file = arena.get_source_file(root_node).expect("source file");

    let if_idx = source_file
        .statements
        .nodes
        .iter()
        .copied()
        .find(|&idx| {
            arena
                .get(idx)
                .is_some_and(|node| node.kind == syntax_kind_ext::IF_STATEMENT)
        })
        .expect("if statement");
    let if_node = arena.get(if_idx).expect("if node");
    let if_data = arena.get_if_statement(if_node).expect("if data");

    let then_node = arena.get(if_data.then_statement).expect("then node");
    let block = arena.get_block(then_node).expect("then block");
    let expr_stmt_idx = block
        .statements
        .nodes
        .iter()
        .copied()
        .find(|&idx| {
            arena
                .get(idx)
                .is_some_and(|node| node.kind == syntax_kind_ext::EXPRESSION_STATEMENT)
        })
        .expect("expression statement");
    let expr_stmt_node = arena.get(expr_stmt_idx).expect("expression node");
    let expr_stmt = arena
        .get_expression_statement(expr_stmt_node)
        .expect("expression statement data");

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena,
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let narrowed = checker.get_type_of_node(expr_stmt.expression);
    assert_eq!(narrowed, TypeId::STRING);
}

#[test]
fn test_flow_narrowing_not_applied_in_closure() {
    use crate::parser::ParserState;

    let source = r#"
let x: string | number;
x = Math.random() > 0.5 ? "hello" : 42;
if (typeof x === "string") {
    const run = () => {
        x.toFixed(2);
    };
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    merge_shared_lib_symbols(&mut binder);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let codes: Vec<u32> = checker.ctx.diagnostics.iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&2339),
        "Expected error 2339 for closure without narrowing, got: {codes:?}"
    );
}

#[test]
fn test_flow_narrowing_applies_in_while() {
    use crate::parser::ParserState;
    use crate::parser::syntax_kind_ext;

    let source = r#"
let x: string | number = Math.random() > 0.5 ? "hello" : 42;
while (typeof x === "string") {
    x;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let root_node = arena.get(root).expect("root node");
    let source_file = arena.get_source_file(root_node).expect("source file");

    let while_idx = source_file
        .statements
        .nodes
        .iter()
        .copied()
        .find(|&idx| {
            arena
                .get(idx)
                .is_some_and(|node| node.kind == syntax_kind_ext::WHILE_STATEMENT)
        })
        .expect("while statement");
    let while_node = arena.get(while_idx).expect("while node");
    let loop_data = arena.get_loop(while_node).expect("while data");

    let body_node = arena.get(loop_data.statement).expect("while body");
    let block = arena.get_block(body_node).expect("while block");
    let expr_stmt_idx = block
        .statements
        .nodes
        .iter()
        .copied()
        .find(|&idx| {
            arena
                .get(idx)
                .is_some_and(|node| node.kind == syntax_kind_ext::EXPRESSION_STATEMENT)
        })
        .expect("inner expression statement");
    let expr_stmt = arena
        .get_expression_statement(arena.get(expr_stmt_idx).expect("inner expr node"))
        .expect("inner expression data");

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena,
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let inner_type = checker.get_type_of_node(expr_stmt.expression);
    assert_eq!(inner_type, TypeId::STRING);
}

/// Test that flow narrowing applies in for loops
///
/// NOTE: Currently ignored - see `test_flow_narrowing_applies_in_if_branch`.
#[test]
fn test_flow_narrowing_applies_in_for() {
    use crate::parser::ParserState;
    use crate::parser::syntax_kind_ext;

    let source = r#"
let x: string | number;
for (; typeof x === "string"; ) {
    x;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let root_node = arena.get(root).expect("root node");
    let source_file = arena.get_source_file(root_node).expect("source file");

    let for_idx = source_file
        .statements
        .nodes
        .iter()
        .copied()
        .find(|&idx| {
            arena
                .get(idx)
                .is_some_and(|node| node.kind == syntax_kind_ext::FOR_STATEMENT)
        })
        .expect("for statement");
    let for_node = arena.get(for_idx).expect("for node");
    let loop_data = arena.get_loop(for_node).expect("for data");

    let body_node = arena.get(loop_data.statement).expect("for body");
    let block = arena.get_block(body_node).expect("for block");
    let expr_stmt_idx = block
        .statements
        .nodes
        .iter()
        .copied()
        .find(|&idx| {
            arena
                .get(idx)
                .is_some_and(|node| node.kind == syntax_kind_ext::EXPRESSION_STATEMENT)
        })
        .expect("inner expression statement");
    let expr_stmt = arena
        .get_expression_statement(arena.get(expr_stmt_idx).expect("inner expr node"))
        .expect("inner expression data");

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena,
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let inner_type = checker.get_type_of_node(expr_stmt.expression);
    assert_eq!(inner_type, TypeId::STRING);
}

/// Test that flow narrowing is not applied in for-of body
///
/// NOTE: Currently ignored - flow narrowing in for-of loops is not fully implemented.
#[test]
fn test_flow_narrowing_not_applied_in_for_of_body() {
    use crate::parser::ParserState;
    use crate::parser::syntax_kind_ext;

    let source = r#"
let x: string | number;
for (const value of [x]) {
    x;
}
"#;

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let arena = parser.get_arena();
    let root_node = arena.get(root).expect("root node");
    let source_file = arena.get_source_file(root_node).expect("source file");

    let for_idx = source_file
        .statements
        .nodes
        .iter()
        .copied()
        .find(|&idx| {
            arena
                .get(idx)
                .is_some_and(|node| node.kind == syntax_kind_ext::FOR_OF_STATEMENT)
        })
        .expect("for-of statement");
    let for_node = arena.get(for_idx).expect("for-of node");
    let for_data = arena.get_for_in_of(for_node).expect("for-of data");

    let body_node = arena.get(for_data.statement).expect("for-of body");
    let block = arena.get_block(body_node).expect("for-of block");
    let expr_stmt_idx = block
        .statements
        .nodes
        .iter()
        .copied()
        .find(|&idx| {
            arena
                .get(idx)
                .is_some_and(|node| node.kind == syntax_kind_ext::EXPRESSION_STATEMENT)
        })
        .expect("inner expression statement");
    let expr_stmt = arena
        .get_expression_statement(arena.get(expr_stmt_idx).expect("inner expr node"))
        .expect("inner expression data");

    let mut binder = BinderState::new();
    binder.bind_source_file(arena, root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena,
        &binder,
        &types,
        "test.ts".to_string(),
        crate::checker::context::CheckerOptions::default(),
    );
    setup_lib_contexts(&mut checker);
    checker.check_source_file(root);

    let inner_type = checker.get_type_of_node(expr_stmt.expression);
    let expected = checker
        .ctx
        .types
        .union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(inner_type, expected);
}

