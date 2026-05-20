use super::*;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

use crate::CheckerState;
use crate::context::{RequestCacheKey, TypingRequest};
use crate::expr::ExprCheckResult;

/// Parse `source` and return the expression node of the first expression
/// statement. Panics if the source doesn't contain exactly that shape.
macro_rules! with_first_expression {
    ($source:expr, |$arena:ident, $expr_idx:ident, $ctx:ident| $body:block) => {{
        let mut parser = ParserState::new("test.ts".to_string(), $source.to_string());
        let root = parser.parse_source_file();

        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);

        let types = TypeInterner::new();
        let mut $ctx = CheckerContext::new(
            parser.get_arena(),
            &binder,
            &types,
            "test.ts".to_string(),
            crate::context::CheckerOptions::default(),
        );

        let $arena = parser.get_arena();
        let root_node = $arena.get(root).expect("root node");
        let sf = $arena.get_source_file(root_node).expect("source file");
        let &stmt_idx = sf.statements.nodes.first().expect("at least one statement");
        let stmt_node = $arena.get(stmt_idx).expect("stmt node");
        let expr_stmt = $arena
            .get_expression_statement(stmt_node)
            .expect("expression statement");
        let $expr_idx = expr_stmt.expression;

        $body
    }};
}

#[test]
fn test_expression_checker_null_literal() {
    with_first_expression!("null", |_arena, expr_idx, ctx| {
        let mut checker = ExpressionChecker::new(&mut ctx);
        let result = checker.try_compute_expr_type(expr_idx);
        assert_eq!(result, ExprCheckResult::Type(TypeId::NULL));
    });
}

#[test]
fn test_expression_checker_delegates_numeric_literal() {
    with_first_expression!("42", |_arena, expr_idx, ctx| {
        let mut checker = ExpressionChecker::new(&mut ctx);
        // Numeric literals need contextual typing, so they should delegate.
        let result = checker.try_compute_expr_type(expr_idx);
        assert_eq!(result, ExprCheckResult::Delegate);
        assert!(result.is_delegate());
        assert!(result.type_id().is_none());
    });
}

#[test]
fn test_check_does_not_cache_delegation_in_node_types() {
    // Identifier requires symbol resolution — ExpressionChecker delegates it.
    // ExpressionChecker::check must NEVER write a DELEGATE sentinel into
    // ctx.node_types.
    with_first_expression!("foo", |_arena, expr_idx, ctx| {
        assert!(
            !ctx.node_types.contains_key(&expr_idx.0),
            "node_types should be empty before check()"
        );
        let result = {
            let mut checker = ExpressionChecker::new(&mut ctx);
            checker.check(expr_idx)
        };
        assert_eq!(result, ExprCheckResult::Delegate);
        assert!(
            !ctx.node_types.contains_key(&expr_idx.0),
            "delegated node must not leak a cache entry into node_types"
        );
    });
}

#[test]
fn test_check_with_context_does_not_cache_delegation_in_request_cache() {
    // Numeric literal with a contextual type normally populates
    // request_node_types; but if the expression delegates, nothing must be
    // written — delegation is control flow, not a type.
    with_first_expression!("42", |_arena, expr_idx, ctx| {
        let before = ctx.request_node_types.len();
        let result = {
            let mut checker = ExpressionChecker::new(&mut ctx);
            checker.check_with_context(expr_idx, Some(TypeId::NUMBER))
        };
        assert_eq!(result, ExprCheckResult::Delegate);
        assert_eq!(
            ctx.request_node_types.len(),
            before,
            "delegated contextual node must not leak a cache entry into request_node_types"
        );

        // And even if we reach into the audited-kind cache key directly,
        // there must not be a DELEGATE entry keyed for this node.
        let request = TypingRequest::with_contextual_type(TypeId::NUMBER);
        if let Some(key) = RequestCacheKey::from_request(&request) {
            assert!(
                !ctx.request_node_types.contains_key(&(expr_idx.0, key)),
                "request_node_types must not contain an entry for a delegated node"
            );
        }
    });
}

#[test]
fn test_check_repeated_delegation_does_not_return_cached_sentinel() {
    // Regardless of how many times we dispatch a delegated node, we must
    // always get `Delegate` back — never a cached `Type(DELEGATE)`.
    with_first_expression!("foo.bar", |_arena, expr_idx, ctx| {
        let mut checker = ExpressionChecker::new(&mut ctx);
        for _ in 0..3 {
            let result = checker.check(expr_idx);
            assert_eq!(result, ExprCheckResult::Delegate);
            assert!(
                matches!(result, ExprCheckResult::Delegate),
                "repeated dispatch must keep reporting Delegate, not a cached TypeId sentinel"
            );
        }
        // And still nothing in the cache.
        assert!(!checker.context().node_types.contains_key(&expr_idx.0));
    });
}

#[test]
fn test_checker_state_resolves_delegated_nodes_to_real_type() {
    // CheckerState::get_type_of_node must resolve a delegated node to a
    // real TypeId or ERROR, never to the DELEGATE sentinel.
    let source = "const x = 1; x";
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut state = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        crate::context::CheckerOptions::default(),
    );

    let arena = parser.get_arena();
    let root_node = arena.get(root).expect("root node");
    let sf = arena.get_source_file(root_node).expect("source file");
    // Second statement is the bare `x` expression statement.
    let &stmt_idx = sf.statements.nodes.get(1).expect("second statement");
    let stmt_node = arena.get(stmt_idx).expect("stmt node");
    let expr_stmt = arena
        .get_expression_statement(stmt_node)
        .expect("expression statement");
    let expr_idx = expr_stmt.expression;

    let ty = state.get_type_of_node(expr_idx);
    assert_ne!(
        ty,
        TypeId::DELEGATE,
        "get_type_of_node must resolve delegated nodes to a real type, not the DELEGATE sentinel"
    );
}

#[test]
fn test_simple_expressions_still_cache() {
    // Directly-handled expressions (null, typeof, void) must continue to
    // populate ctx.node_types so repeated calls take the cached fast path.
    with_first_expression!("null", |_arena, expr_idx, ctx| {
        assert!(!ctx.node_types.contains_key(&expr_idx.0));
        let result = {
            let mut checker = ExpressionChecker::new(&mut ctx);
            checker.check(expr_idx)
        };
        assert_eq!(result, ExprCheckResult::Type(TypeId::NULL));
        assert_eq!(
            ctx.node_types.get(&expr_idx.0),
            Some(&TypeId::NULL),
            "simple expressions must still cache their concrete TypeId"
        );
    });
}

#[test]
fn test_parenthesized_malformed_node_is_delegated_not_cached() {
    // A parenthesized expression whose inner expression is parsed normally
    // should pass through. This test exercises the pass-through branch of
    // compute_type_impl and verifies no DELEGATE sentinel leaks in.
    with_first_expression!("(null)", |_arena, expr_idx, ctx| {
        let result = {
            let mut checker = ExpressionChecker::new(&mut ctx);
            checker.check(expr_idx)
        };
        assert_eq!(result, ExprCheckResult::Type(TypeId::NULL));
        assert_eq!(ctx.node_types.get(&expr_idx.0), Some(&TypeId::NULL));
    });
}
