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

            // Element access expression - array[index] or object[key]
            k if k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => self.check_element_access(idx),

            // Default case - return UNKNOWN for unhandled expressions instead of ANY
            // This exposes type errors that were previously hidden by the permissive ANY default
            _ => TypeId::UNKNOWN,
        }
    }

    /// Check element access expression (array[index] or object[key])
    fn check_element_access(&mut self, idx: NodeIndex) -> TypeId {
        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::UNKNOWN;
        };

        // Get the element access node data if available
        if let Some(access_data) = self.ctx.arena.get_access_expr(node) {
            // Check the target expression (the thing being accessed)
            let target_type = self.check(access_data.expression);

            // Try to resolve array element type
            if let Some(type_key) = self.ctx.types.lookup(target_type) {
                match type_key {
                    crate::solver::TypeKey::Array(element_type) => {
                        // Direct array access - return the element type
                        return element_type;
                    }
                    crate::solver::TypeKey::Tuple(tuple_list_id) => {
                        // Proper tuple element access with index checking
                        let index_type = self.check(access_data.name_or_argument);
                        if let Some(crate::solver::TypeKey::Literal(literal_value)) =
                            self.ctx.types.lookup(index_type)
                            && let crate::solver::LiteralValue::Number(num) = literal_value {
                                // Check if the numeric index is valid for this tuple
                                let index = num.0 as usize;
                                let tuple_list = self.ctx.types.tuple_list(tuple_list_id);
                                if index < tuple_list.len() {
                                    // Return the specific element type
                                    return tuple_list[index].type_id;
                                }
                                // Index out of bounds - in TypeScript this is undefined
                                return TypeId::UNDEFINED;
                            }
                        // For non-literal indices or invalid cases, return union of all tuple elements
                        let tuple_list = self.ctx.types.tuple_list(tuple_list_id);
                        if tuple_list.is_empty() {
                            return TypeId::NEVER;
                        }
                        let element_types: Vec<TypeId> =
                            tuple_list.iter().map(|elem| elem.type_id).collect();
                        return self.ctx.types.union(element_types);
                    }
                    _ => {
                        // Not an array or tuple - fall through to default behavior
                    }
                }
            }

            // If we can't resolve the specific element type, fall back to ANY
            // Using ANY instead of UNKNOWN allows more code to work while we implement
            // more sophisticated element access logic
            TypeId::ANY
        } else {
            TypeId::UNKNOWN
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
    use crate::binder::BinderState;
    use crate::parser::ParserState;
    use crate::solver::TypeInterner;

    #[test]
    fn test_expression_checker_numeric_literal() {
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
            crate::checker::context::CheckerOptions::default(),
        );

        // Get the expression statement and its expression
        if let Some(root_node) = parser.get_arena().get(root)
            && let Some(sf_data) = parser.get_arena().get_source_file(root_node)
                && let Some(&stmt_idx) = sf_data.statements.nodes.first()
                    && let Some(stmt_node) = parser.get_arena().get(stmt_idx)
                        && let Some(expr_stmt) =
                            parser.get_arena().get_expression_statement(stmt_node)
                        {
                            let mut checker = ExpressionChecker::new(&mut ctx);
                            let ty = checker.check(expr_stmt.expression);
                            assert_eq!(ty, TypeId::NUMBER);
                        }
    }
}
