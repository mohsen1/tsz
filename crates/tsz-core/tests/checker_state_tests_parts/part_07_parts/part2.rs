/// Test that flow narrowing applies in for loops
///
/// NOTE: Currently ignored - see `test_flow_narrowing_applies_in_if_branch`.
#[test]
fn test_flow_narrowing_applies_in_for() {
    use crate::parser::syntax_kind_ext;

    let source = r#"
let x: string | number;
for (; typeof x === "string"; ) {
    x;
}
"#;

    let (parser, root) = parse_test_source(source);

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
    use crate::parser::syntax_kind_ext;

    let source = r#"
let x: string | number;
for (const value of [x]) {
    x;
}
"#;

    let (parser, root) = parse_test_source(source);

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

/// Test that flow narrowing is not applied in for-in body
///
/// NOTE: Currently ignored - flow narrowing in for-in loops is not fully implemented.
#[test]
fn test_flow_narrowing_not_applied_in_for_in_body() {
    use crate::parser::syntax_kind_ext;

    let source = r#"
let x: string | number;
for (const key in { a: x }) {
    x;
}
"#;

    let (parser, root) = parse_test_source(source);

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
                .is_some_and(|node| node.kind == syntax_kind_ext::FOR_IN_STATEMENT)
        })
        .expect("for-in statement");
    let for_node = arena.get(for_idx).expect("for-in node");
    let for_data = arena.get_for_in_of(for_node).expect("for-in data");

    let body_node = arena.get(for_data.statement).expect("for-in body");
    let block = arena.get_block(body_node).expect("for-in block");
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

/// Test that flow narrowing is not applied in do-while body
///
/// NOTE: Currently ignored - flow narrowing in do-while loops is not fully implemented.
#[test]
fn test_flow_narrowing_not_applied_in_do_while_body() {
    let source = r#"
let x: string | number;
do {
    x.toUpperCase();
} while (typeof x === "string");
"#;

    let (parser, root) = parse_test_source(source);

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
        "Expected error 2339 for do-while body without narrowing, got: {codes:?}"
    );
}

/// Test that flow narrowing is not applied after while loop exit
///
/// NOTE: Currently ignored - see `test_flow_narrowing_not_applied_after_for_exit`.
#[test]
fn test_flow_narrowing_not_applied_after_while_exit() {
    use crate::parser::syntax_kind_ext;

    let source = r#"
let x: string | number;
while (typeof x === "string") {
    break;
}
x;
"#;

    let (parser, root) = parse_test_source(source);

    let arena = parser.get_arena();
    let root_node = arena.get(root).expect("root node");
    let source_file = arena.get_source_file(root_node).expect("source file");

    let expr_stmt_idx = *source_file
        .statements
        .nodes
        .iter()
        .rfind(|&&idx| {
            arena
                .get(idx)
                .is_some_and(|node| node.kind == syntax_kind_ext::EXPRESSION_STATEMENT)
        })
        .expect("expression statement");
    let expr_stmt = arena
        .get_expression_statement(arena.get(expr_stmt_idx).expect("expr node"))
        .expect("expression data");

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

    let after_type = checker.get_type_of_node(expr_stmt.expression);
    let expected = checker
        .ctx
        .types
        .union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(after_type, expected);
}
