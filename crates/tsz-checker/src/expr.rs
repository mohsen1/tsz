//! Expression Type Checking
//!
//! This module handles type inference and checking for expressions.
//! It follows the "Check Fast, Explain Slow" pattern where we first
//! infer types, then use the solver to explain any failures.
//!
//! ## Integration with CheckerState
//!
//! `ExpressionChecker` serves as the primary dispatcher for expression types.
//! Simple expressions are handled directly here, while complex expressions
//! that need full `CheckerState` context return `TypeId::DELEGATE` to signal
//! that `CheckerState::compute_type_of_node` should handle them.
//!
//! ### Expressions handled directly:
//! - Simple literals without contextual typing (null)
//! - typeof expressions (always string)
//! - void expressions (always undefined)
//! - Postfix unary (++/-- always return number)
//! - Parenthesized expressions (pass through)
//!
//! ### Expressions delegated to CheckerState:
//! - Literals with contextual typing (numeric, string, boolean, template)
//! - Identifiers, this, super (need symbol resolution)
//! - Binary expressions (need operator overloading, narrowing)
//! - Call/new expressions (need signature resolution)
//! - Property/element access (need object type resolution)
//! - Function/arrow expressions (need signature building)
//! - Object/array literals (need contextual typing)
//! - Type assertions (as/satisfies) (need type node resolution)
//! - Conditional expressions (need union type building)
//! - Await expressions (need Promise unwrapping)

use super::context::CheckerContext;
use std::cell::Cell;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

/// Maximum recursion depth for expression checking to prevent stack overflow
const MAX_EXPR_CHECK_DEPTH: u32 = 500;

/// Expression type checker that operates on the shared context.
///
/// This is a stateless checker that borrows the context mutably.
/// All type inference for expressions goes through this checker.
pub struct ExpressionChecker<'a, 'ctx> {
    ctx: &'a mut CheckerContext<'ctx>,
    /// Recursion depth counter for stack overflow protection
    depth: Cell<u32>,
}

impl<'a, 'ctx> ExpressionChecker<'a, 'ctx> {
    /// Create a new expression checker with a mutable context reference.
    pub fn new(ctx: &'a mut CheckerContext<'ctx>) -> Self {
        Self {
            ctx,
            depth: Cell::new(0),
        }
    }

    /// Check an expression and return its type.
    ///
    /// This is the main entry point for expression type checking.
    /// It handles caching and dispatches to specific expression handlers.
    pub fn check(&mut self, idx: NodeIndex) -> TypeId {
        self.check_with_context(idx, None)
    }

    /// Check an expression with a contextual type hint.
    ///
    /// Contextual types enable downward inference where the expected type
    /// influences the inferred type. For example:
    /// - `const x: string = expr` - `expr` is checked with context `string`
    /// - `const f: (x: number) => void = (x) => {}` - `x` is inferred as `number`
    ///
    /// # Caching Behavior
    ///
    /// When `context_type` is `Some`, the cache is **bypassed** to avoid
    /// incorrect results. The same expression can have different types
    /// depending on the context, so caching by NodeIndex alone is unsound.
    pub fn check_with_context(&mut self, idx: NodeIndex, context_type: Option<TypeId>) -> TypeId {
        // Stack overflow protection
        let current_depth = self.depth.get();
        if current_depth >= MAX_EXPR_CHECK_DEPTH {
            return TypeId::ERROR;
        }
        self.depth.set(current_depth + 1);

        let result = if let Some(ctx_type) = context_type {
            // Bypass cache when contextual type is provided
            // Contextual types can produce different results for the same node
            self.compute_type_with_context(idx, ctx_type)
        } else {
            // Check cache first for non-contextual checks
            if let Some(&cached) = self.ctx.node_types.get(&idx.0) {
                self.depth.set(current_depth);
                return cached;
            }

            // Compute and cache
            let result = self.compute_type(idx);
            self.ctx.node_types.insert(idx.0, result);
            result
        };

        self.depth.set(current_depth);
        result
    }

    /// Compute the type of an expression without caching.
    ///
    /// This is called by `CheckerState::compute_type_of_node` to get an initial
    /// type for expressions. Returns `TypeId::DELEGATE` if the expression needs
    /// full `CheckerState` context for proper type resolution.
    ///
    /// Simple expressions that don't need contextual typing or symbol resolution
    /// are handled directly here. Complex expressions delegate to CheckerState.
    pub fn compute_type_uncached(&mut self, idx: NodeIndex) -> TypeId {
        self.compute_type_impl(idx, None)
    }

    /// Compute the type of an expression with contextual typing (no caching).
    ///
    /// This is called when a contextual type is available (e.g., from variable
    /// declarations, assignments, function parameters). The contextual type
    /// influences how the expression is inferred.
    fn compute_type_with_context(&mut self, idx: NodeIndex, context_type: TypeId) -> TypeId {
        self.compute_type_impl(idx, Some(context_type))
    }

    /// Compute the type of an expression (internal, not cached).
    fn compute_type(&mut self, idx: NodeIndex) -> TypeId {
        self.compute_type_impl(idx, None)
    }

    /// Core implementation for computing expression types.
    ///
    /// Returns `TypeId::DELEGATE` for complex expressions that need CheckerState.
    ///
    /// # Parameters
    /// - `idx`: The node index to check
    /// - `context_type`: Optional contextual type hint for downward inference
    fn compute_type_impl(&mut self, idx: NodeIndex, context_type: Option<TypeId>) -> TypeId {
        let Some(node) = self.ctx.arena.get(idx) else {
            // Return UNKNOWN instead of ANY to expose missing nodes as errors
            return TypeId::UNKNOWN;
        };

        match node.kind {
            // =====================================================================
            // Simple expressions handled directly
            // =====================================================================

            // Null literal - always TypeId::NULL (context doesn't affect null)
            k if k == SyntaxKind::NullKeyword as u16 => TypeId::NULL,

            // typeof expression always returns string (context doesn't affect typeof)
            k if k == syntax_kind_ext::TYPE_OF_EXPRESSION => TypeId::STRING,

            // void expression always returns undefined (context doesn't affect void)
            k if k == syntax_kind_ext::VOID_EXPRESSION => TypeId::UNDEFINED,

            // Parenthesized expression - pass through context to inner expression
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                if let Some(paren) = self.ctx.arena.get_parenthesized(node) {
                    // Recursively check inner expression with same context
                    self.compute_type_impl(paren.expression, context_type)
                } else {
                    // Return DELEGATE to let CheckerState handle malformed nodes
                    TypeId::DELEGATE
                }
            }

            // =====================================================================
            // Literals with contextual typing - DELEGATE to CheckerState
            // These need contextual typing analysis to decide between literal types
            // (e.g., `42` as literal) vs widened types (e.g., `number`).
            // =====================================================================
            k if k == SyntaxKind::NumericLiteral as u16 => TypeId::DELEGATE,
            k if k == SyntaxKind::StringLiteral as u16 => TypeId::DELEGATE,
            k if k == SyntaxKind::TrueKeyword as u16 => TypeId::DELEGATE,
            k if k == SyntaxKind::FalseKeyword as u16 => TypeId::DELEGATE,
            k if k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 => TypeId::DELEGATE,

            // =====================================================================
            // Expressions requiring symbol resolution - DELEGATE to CheckerState
            // =====================================================================
            k if k == SyntaxKind::Identifier as u16 => TypeId::DELEGATE,
            k if k == SyntaxKind::ThisKeyword as u16 => TypeId::DELEGATE,
            k if k == SyntaxKind::SuperKeyword as u16 => TypeId::DELEGATE,

            // =====================================================================
            // Complex expressions - DELEGATE to CheckerState
            // =====================================================================

            // Binary expressions need operator type resolution and narrowing
            k if k == syntax_kind_ext::BINARY_EXPRESSION => TypeId::DELEGATE,

            // Call/new expressions need signature resolution
            k if k == syntax_kind_ext::CALL_EXPRESSION => TypeId::DELEGATE,
            k if k == syntax_kind_ext::NEW_EXPRESSION => TypeId::DELEGATE,

            // Property/element access need object type resolution
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => TypeId::DELEGATE,
            k if k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => TypeId::DELEGATE,

            // Conditional expressions need union type building
            k if k == syntax_kind_ext::CONDITIONAL_EXPRESSION => TypeId::DELEGATE,

            // Function expressions need signature building
            k if k == syntax_kind_ext::FUNCTION_EXPRESSION => TypeId::DELEGATE,
            k if k == syntax_kind_ext::ARROW_FUNCTION => TypeId::DELEGATE,

            // Object/array literals need contextual typing
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION => TypeId::DELEGATE,
            k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => TypeId::DELEGATE,

            // Class expressions need class type building
            k if k == syntax_kind_ext::CLASS_EXPRESSION => TypeId::DELEGATE,

            // Unary expressions need operand type checking
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => TypeId::DELEGATE,
            k if k == syntax_kind_ext::POSTFIX_UNARY_EXPRESSION => TypeId::DELEGATE,

            // Await expressions need Promise unwrapping
            k if k == syntax_kind_ext::AWAIT_EXPRESSION => TypeId::DELEGATE,

            // Type assertions need type node resolution
            k if k == syntax_kind_ext::AS_EXPRESSION => TypeId::DELEGATE,
            k if k == syntax_kind_ext::SATISFIES_EXPRESSION => TypeId::DELEGATE,
            k if k == syntax_kind_ext::TYPE_ASSERTION => TypeId::DELEGATE,

            // Template expressions need string interpolation handling
            k if k == syntax_kind_ext::TEMPLATE_EXPRESSION => TypeId::DELEGATE,

            // Variable declarations need initializer/annotation handling
            k if k == syntax_kind_ext::VARIABLE_DECLARATION => TypeId::DELEGATE,

            // Function declarations need signature building
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => TypeId::DELEGATE,

            // =====================================================================
            // Type nodes - DELEGATE to CheckerState
            // These are not expressions but may be passed through get_type_of_node
            // =====================================================================
            k if k == syntax_kind_ext::TYPE_REFERENCE => TypeId::DELEGATE,
            k if k == syntax_kind_ext::UNION_TYPE => TypeId::DELEGATE,
            k if k == syntax_kind_ext::INTERSECTION_TYPE => TypeId::DELEGATE,
            k if k == syntax_kind_ext::ARRAY_TYPE => TypeId::DELEGATE,
            k if k == syntax_kind_ext::TYPE_OPERATOR => TypeId::DELEGATE,
            k if k == syntax_kind_ext::FUNCTION_TYPE => TypeId::DELEGATE,
            k if k == syntax_kind_ext::TYPE_LITERAL => TypeId::DELEGATE,
            k if k == syntax_kind_ext::TYPE_QUERY => TypeId::DELEGATE,
            k if k == syntax_kind_ext::QUALIFIED_NAME => TypeId::DELEGATE,

            // Type keywords - DELEGATE to CheckerState for consistency
            k if k == SyntaxKind::NumberKeyword as u16 => TypeId::DELEGATE,
            k if k == SyntaxKind::StringKeyword as u16 => TypeId::DELEGATE,
            k if k == SyntaxKind::BooleanKeyword as u16 => TypeId::DELEGATE,
            k if k == SyntaxKind::VoidKeyword as u16 => TypeId::DELEGATE,
            k if k == SyntaxKind::AnyKeyword as u16 => TypeId::DELEGATE,
            k if k == SyntaxKind::NeverKeyword as u16 => TypeId::DELEGATE,
            k if k == SyntaxKind::UnknownKeyword as u16 => TypeId::DELEGATE,
            k if k == SyntaxKind::UndefinedKeyword as u16 => TypeId::DELEGATE,
            k if k == SyntaxKind::ObjectKeyword as u16 => TypeId::DELEGATE,
            k if k == SyntaxKind::BigIntKeyword as u16 => TypeId::DELEGATE,
            k if k == SyntaxKind::SymbolKeyword as u16 => TypeId::DELEGATE,

            // JSX elements - DELEGATE to CheckerState
            k if k == syntax_kind_ext::JSX_ELEMENT => TypeId::DELEGATE,
            k if k == syntax_kind_ext::JSX_SELF_CLOSING_ELEMENT => TypeId::DELEGATE,
            k if k == syntax_kind_ext::JSX_FRAGMENT => TypeId::DELEGATE,

            // =====================================================================
            // Default - unknown node type, delegate to CheckerState
            // =====================================================================
            _ => TypeId::DELEGATE,
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
}
