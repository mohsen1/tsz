//! Expression Type Checking
//!
//! This module handles type inference and checking for expressions.
//! It follows the "Check Fast, Explain Slow" pattern where we first
//! infer types, then use the solver to explain any failures.

use super::context::CheckerContext;
use crate::parser::NodeIndex;
use crate::solver::TypeId;

/// Expression type checker that operates on the shared context.
///
/// This is a stateless checker that borrows the context mutably.
/// All type inference for expressions goes through this checker.
pub struct ExpressionChecker<'a, 'ctx> {
    ctx: &'a mut CheckerContext<'ctx>,
}

impl<'a, 'ctx> ExpressionChecker<'a, 'ctx> {
    /// Create a new expression checker with a mutable context reference.
    pub fn new(ctx: &'a mut CheckerContext<'ctx>) -> Self {
        Self { ctx }
    }

    /// Check an expression and return its type.
    ///
    /// This is the main entry point for expression type checking.
    /// It handles caching and dispatches to specific expression handlers.
    pub fn check(&mut self, idx: NodeIndex) -> TypeId {
        // Check cache first
        if let Some(&cached) = self.ctx.node_types.get(&idx.0) {
            return cached;
        }

        // Compute and cache
        let result = self.compute_type(idx);
        self.ctx.node_types.insert(idx.0, result);
        result
    }

    /// Compute the type of an expression (internal, not cached).
    fn compute_type(&mut self, idx: NodeIndex) -> TypeId {
        use crate::parser::syntax_kind_ext;
        use crate::scanner::SyntaxKind;

        let Some(node) = self.ctx.arena.get(idx) else {
            // Return UNKNOWN instead of ANY to expose missing nodes as errors
            return TypeId::UNKNOWN;
        };

        match node.kind {
            // Literals - use compile-time constant TypeIds
            k if k == SyntaxKind::NumericLiteral as u16 => TypeId::NUMBER,
            k if k == SyntaxKind::StringLiteral as u16 => TypeId::STRING,
            k if k == SyntaxKind::TrueKeyword as u16 => self.ctx.types.literal_boolean(true),
            k if k == SyntaxKind::FalseKeyword as u16 => self.ctx.types.literal_boolean(false),
            k if k == SyntaxKind::NullKeyword as u16 => TypeId::NULL,

            // Postfix unary expression - ++ and -- always return number
            k if k == syntax_kind_ext::POSTFIX_UNARY_EXPRESSION => TypeId::NUMBER,

            // typeof expression always returns string
            k if k == syntax_kind_ext::TYPE_OF_EXPRESSION => TypeId::STRING,

            // void expression always returns undefined
            k if k == syntax_kind_ext::VOID_EXPRESSION => TypeId::UNDEFINED,

            // Parenthesized expression - pass through to inner expression
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                if let Some(paren) = self.ctx.arena.get_parenthesized(node) {
                    self.check(paren.expression)
                } else {
                    // Return UNKNOWN instead of ANY to expose parsing failures
                    TypeId::UNKNOWN
                }
            }

            // Default case - return UNKNOWN for unhandled expressions instead of ANY
            // This exposes type errors that were previously hidden by the permissive ANY default
            _ => TypeId::UNKNOWN,
        }
    }

    /// Get the context reference (for read-only access).
    pub fn context(&self) -> &CheckerContext<'ctx> {
        self.ctx
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::solver::TypeInterner;
    use crate::thin_binder::ThinBinderState;
    use crate::thin_parser::ThinParserState;

    #[test]
    fn test_expression_checker_numeric_literal() {
        let source = "42";
        let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut binder = ThinBinderState::new();
        binder.bind_source_file(parser.get_arena(), root);

        let types = TypeInterner::new();
        let mut ctx =
            CheckerContext::new(parser.get_arena(), &binder, &types, "test.ts".to_string(), false);

        // Get the expression statement and its expression
        if let Some(root_node) = parser.get_arena().get(root) {
            if let Some(sf_data) = parser.get_arena().get_source_file(root_node) {
                if let Some(&stmt_idx) = sf_data.statements.nodes.first() {
                    if let Some(stmt_node) = parser.get_arena().get(stmt_idx) {
                        if let Some(expr_stmt) =
                            parser.get_arena().get_expression_statement(stmt_node)
                        {
                            let mut checker = ExpressionChecker::new(&mut ctx);
                            let ty = checker.check(expr_stmt.expression);
                            assert_eq!(ty, TypeId::NUMBER);
                        }
                    }
                }
            }
        }
    }
}
