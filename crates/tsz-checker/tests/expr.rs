use super::*;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

#[test]
fn test_expression_checker_null_literal() {
    let source = "null";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut ctx = CheckerContext::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::context::CheckerOptions::default(),
    );

    // Get the expression statement and its expression
    if let Some(root_node) = parser.get_arena().get(root)
        && let Some(sf_data) = parser.get_arena().get_source_file(root_node)
        && let Some(&stmt_idx) = sf_data.statements.nodes.first()
        && let Some(stmt_node) = parser.get_arena().get(stmt_idx)
        && let Some(expr_stmt) = parser.get_arena().get_expression_statement(stmt_node)
    {
        let mut checker = ExpressionChecker::new(&mut ctx);
        let ty = checker.compute_type_uncached(expr_stmt.expression);
        assert_eq!(ty, TypeId::NULL);
    }
}

#[test]
fn test_expression_checker_delegates_numeric_literal() {
    let source = "42";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut ctx = CheckerContext::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::context::CheckerOptions::default(),
    );

    // Get the expression statement and its expression
    if let Some(root_node) = parser.get_arena().get(root)
        && let Some(sf_data) = parser.get_arena().get_source_file(root_node)
        && let Some(&stmt_idx) = sf_data.statements.nodes.first()
        && let Some(stmt_node) = parser.get_arena().get(stmt_idx)
        && let Some(expr_stmt) = parser.get_arena().get_expression_statement(stmt_node)
    {
        let mut checker = ExpressionChecker::new(&mut ctx);
        // Numeric literals need contextual typing, so they should delegate
        let ty = checker.compute_type_uncached(expr_stmt.expression);
        assert_eq!(ty, TypeId::DELEGATE);
    }
}
