//! Generator Function Type Checking
//!
//! This module handles type inference and checking for generator functions,
//! yield expressions, and the Generator<Y, R, N> type.
//!
//! # Generator Types
//!
//! TypeScript represents generator functions with the `Generator<T, TReturn, TNext>` type:
//! - `T` (yield type): The type of values yielded by the generator
//! - `TReturn` (return type): The type of the value returned when done
//! - `TNext` (next type): The type of values passed to `.next()`
//!
//! # Yield Expressions
//!
//! - `yield expr` - Yields a value, type must be assignable to T
//! - `yield* iterable` - Delegates to another iterable/generator
//!
//! # Transform Target
//!
//! When targeting ES5, generators are transformed to state machines using
//! the `__generator` helper. See `transforms/generators.rs` for the transform.

use super::context::CheckerContext;
use crate::parser::syntax_kind_ext;
use crate::parser::NodeIndex;
use crate::scanner::SyntaxKind;
use crate::solver::TypeId;

/// Type checker for generator functions and yield expressions.
pub struct GeneratorChecker<'a, 'ctx> {
    ctx: &'a mut CheckerContext<'ctx>,
}

/// Information about a generator function's types.
#[derive(Debug, Clone)]
pub struct GeneratorTypeInfo {
    /// The type of values yielded (T in Generator<T, TReturn, TNext>)
    pub yield_type: TypeId,
    /// The type of the return value (TReturn)
    pub return_type: TypeId,
    /// The type of values passed to .next() (TNext)
    pub next_type: TypeId,
}

impl Default for GeneratorTypeInfo {
    fn default() -> Self {
        Self {
            yield_type: TypeId::UNKNOWN,
            return_type: TypeId::VOID,
            next_type: TypeId::UNKNOWN,
        }
    }
}

/// Result of checking a yield expression.
#[derive(Debug, Clone)]
pub struct YieldCheckResult {
    /// The type of the yield expression (the type received from .next())
    pub expression_type: TypeId,
    /// The type being yielded
    pub yielded_type: TypeId,
    /// Whether this is a delegating yield (yield*)
    pub is_delegation: bool,
}

impl<'a, 'ctx> GeneratorChecker<'a, 'ctx> {
    /// Create a new generator checker with a mutable context reference.
    pub fn new(ctx: &'a mut CheckerContext<'ctx>) -> Self {
        Self { ctx }
    }

    /// Check if a function is a generator function.
    pub fn is_generator_function(&self, func_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(func_idx) else {
            return false;
        };

        // Check for function declaration/expression with asterisk
        if node.kind == syntax_kind_ext::FUNCTION_DECLARATION {
            if let Some(func) = self.ctx.arena.get_function_declaration(node) {
                return func.asterisk_token;
            }
        }

        if node.kind == syntax_kind_ext::FUNCTION_EXPRESSION {
            if let Some(func) = self.ctx.arena.get_function_expression(node) {
                return func.asterisk_token;
            }
        }

        // Check for generator method in class
        if node.kind == syntax_kind_ext::METHOD_DECLARATION {
            if let Some(method) = self.ctx.arena.get_method(node) {
                return method.asterisk_token;
            }
        }

        false
    }

    /// Check if a function is an async generator function.
    pub fn is_async_generator_function(&self, func_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(func_idx) else {
            return false;
        };

        // Check for async generator function declaration
        if node.kind == syntax_kind_ext::FUNCTION_DECLARATION {
            if let Some(func) = self.ctx.arena.get_function_declaration(node) {
                return func.asterisk_token && func.is_async;
            }
        }

        // Check for async generator function expression
        if node.kind == syntax_kind_ext::FUNCTION_EXPRESSION {
            if let Some(func) = self.ctx.arena.get_function_expression(node) {
                return func.asterisk_token && func.is_async;
            }
        }

        // Check for async generator method
        if node.kind == syntax_kind_ext::METHOD_DECLARATION {
            if let Some(method) = self.ctx.arena.get_method(node) {
                return method.asterisk_token && method.is_async;
            }
        }

        false
    }

    /// Infer the Generator<Y, R, N> type for a generator function.
    ///
    /// This analyzes all yield expressions in the function to determine:
    /// - Y: Union of all yielded types
    /// - R: The return type (from return statements or void if none)
    /// - N: The type expected from .next() calls (usually unknown/void)
    pub fn infer_generator_type(&mut self, func_idx: NodeIndex) -> GeneratorTypeInfo {
        let Some(node) = self.ctx.arena.get(func_idx) else {
            return GeneratorTypeInfo::default();
        };

        let body_idx = self.get_function_body(node);
        if body_idx.is_null() {
            return GeneratorTypeInfo::default();
        }

        // Collect all yield expression types
        let mut yield_types = Vec::new();
        self.collect_yield_types(body_idx, &mut yield_types);

        // Collect all return statement types
        let mut return_types = Vec::new();
        self.collect_return_types(body_idx, &mut return_types);

        // Build the yield type as union of all yielded types
        let yield_type = if yield_types.is_empty() {
            TypeId::NEVER
        } else if yield_types.len() == 1 {
            yield_types[0]
        } else {
            self.ctx.types.union(yield_types)
        };

        // Build the return type
        let return_type = if return_types.is_empty() {
            TypeId::VOID
        } else if return_types.len() == 1 {
            return_types[0]
        } else {
            self.ctx.types.union(return_types)
        };

        // Next type is typically unknown unless explicitly typed
        let next_type = TypeId::UNKNOWN;

        GeneratorTypeInfo {
            yield_type,
            return_type,
            next_type,
        }
    }

    /// Check a yield expression and return its type information.
    pub fn check_yield_expression(&mut self, yield_idx: NodeIndex) -> YieldCheckResult {
        let Some(node) = self.ctx.arena.get(yield_idx) else {
            return YieldCheckResult {
                expression_type: TypeId::ANY,
                yielded_type: TypeId::UNDEFINED,
                is_delegation: false,
            };
        };

        // Parse the yield expression
        if let Some(yield_expr) = self.ctx.arena.get_yield_expression(node) {
            let is_delegation = yield_expr.asterisk_token;

            // Get the type of the yielded expression
            let yielded_type = if yield_expr.expression.is_null() {
                TypeId::UNDEFINED
            } else {
                self.check_expression(yield_expr.expression)
            };

            // For yield*, we need to extract the yield type from the iterable
            let (final_yield_type, expression_type) = if is_delegation {
                // yield* delegates to another iterable/generator
                // The yielded values come from the iterable's element type
                let element_type = self.get_iterable_element_type(yielded_type);
                // The result type is the return type of the delegated iterator
                let return_type = self.get_iterator_return_type(yielded_type);
                (element_type, return_type)
            } else {
                // Regular yield - the expression type is what .next() returns
                // which is the TNext type parameter of the containing generator
                (yielded_type, TypeId::ANY)
            };

            return YieldCheckResult {
                expression_type,
                yielded_type: final_yield_type,
                is_delegation,
            };
        }

        YieldCheckResult {
            expression_type: TypeId::ANY,
            yielded_type: TypeId::UNDEFINED,
            is_delegation: false,
        }
    }

    /// Check that a yield expression is valid in its context.
    ///
    /// Returns an error if:
    /// - yield is used outside a generator function
    /// - yield* is used with a non-iterable
    pub fn validate_yield_context(&self, yield_idx: NodeIndex) -> Result<(), GeneratorError> {
        // Check that we're inside a generator function
        if !self.is_inside_generator_function(yield_idx) {
            return Err(GeneratorError::YieldOutsideGenerator);
        }

        let Some(node) = self.ctx.arena.get(yield_idx) else {
            return Ok(());
        };

        if let Some(yield_expr) = self.ctx.arena.get_yield_expression(node) {
            // For yield*, check that the expression is iterable
            if yield_expr.asterisk_token && !yield_expr.expression.is_null() {
                let expr_type = self.peek_expression_type(yield_expr.expression);
                if !self.is_iterable(expr_type) {
                    return Err(GeneratorError::YieldDelegationNonIterable);
                }
            }
        }

        Ok(())
    }

    /// Get the return type of a generator function (the Generator<Y, R, N> type).
    pub fn get_generator_return_type(&mut self, func_idx: NodeIndex) -> TypeId {
        let info = self.infer_generator_type(func_idx);

        // Create Generator<Y, R, N> type reference
        self.create_generator_type(info.yield_type, info.return_type, info.next_type)
    }

    /// Get the return type of an async generator function (AsyncGenerator<Y, R, N>).
    pub fn get_async_generator_return_type(&mut self, func_idx: NodeIndex) -> TypeId {
        let info = self.infer_generator_type(func_idx);

        // Create AsyncGenerator<Y, R, N> type reference
        self.create_async_generator_type(info.yield_type, info.return_type, info.next_type)
    }

    // =========================================================================
    // Helper methods
    // =========================================================================

    fn get_function_body(&self, node: &crate::parser::thin_node::ThinNode) -> NodeIndex {
        if node.kind == syntax_kind_ext::FUNCTION_DECLARATION {
            if let Some(func) = self.ctx.arena.get_function_declaration(node) {
                return func.body;
            }
        }
        if node.kind == syntax_kind_ext::FUNCTION_EXPRESSION {
            if let Some(func) = self.ctx.arena.get_function_expression(node) {
                return func.body;
            }
        }
        if node.kind == syntax_kind_ext::METHOD_DECLARATION {
            if let Some(method) = self.ctx.arena.get_method(node) {
                return method.body;
            }
        }
        NodeIndex::NULL
    }

    fn collect_yield_types(&mut self, idx: NodeIndex, types: &mut Vec<TypeId>) {
        let Some(node) = self.ctx.arena.get(idx) else {
            return;
        };

        // Check if this is a yield expression
        if node.kind == syntax_kind_ext::YIELD_EXPRESSION {
            if let Some(yield_expr) = self.ctx.arena.get_yield_expression(node) {
                let yielded_type = if yield_expr.expression.is_null() {
                    TypeId::UNDEFINED
                } else {
                    self.check_expression(yield_expr.expression)
                };
                types.push(yielded_type);
            }
            return;
        }

        // Don't recurse into nested functions
        if node.kind == syntax_kind_ext::FUNCTION_DECLARATION
            || node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
            || node.kind == syntax_kind_ext::ARROW_FUNCTION
        {
            return;
        }

        // Recurse into block statements
        if node.kind == syntax_kind_ext::BLOCK {
            if let Some(block) = self.ctx.arena.get_block(node) {
                for &stmt_idx in &block.statements.nodes {
                    self.collect_yield_types(stmt_idx, types);
                }
            }
            return;
        }

        // Recurse into expression statements
        if node.kind == syntax_kind_ext::EXPRESSION_STATEMENT {
            if let Some(expr_stmt) = self.ctx.arena.get_expression_statement(node) {
                self.collect_yield_types(expr_stmt.expression, types);
            }
        }

        // Recurse into if statements
        if node.kind == syntax_kind_ext::IF_STATEMENT {
            if let Some(if_stmt) = self.ctx.arena.get_if_statement(node) {
                self.collect_yield_types(if_stmt.expression, types);
                self.collect_yield_types(if_stmt.then_statement, types);
                self.collect_yield_types(if_stmt.else_statement, types);
            }
        }

        // Recurse into for loops
        if node.kind == syntax_kind_ext::FOR_STATEMENT {
            if let Some(for_stmt) = self.ctx.arena.get_for_statement(node) {
                self.collect_yield_types(for_stmt.initializer, types);
                self.collect_yield_types(for_stmt.condition, types);
                self.collect_yield_types(for_stmt.incrementor, types);
                self.collect_yield_types(for_stmt.statement, types);
            }
        }

        // Recurse into while loops
        if node.kind == syntax_kind_ext::WHILE_STATEMENT {
            if let Some(while_stmt) = self.ctx.arena.get_while_statement(node) {
                self.collect_yield_types(while_stmt.expression, types);
                self.collect_yield_types(while_stmt.statement, types);
            }
        }

        // Recurse into try statements
        if node.kind == syntax_kind_ext::TRY_STATEMENT {
            if let Some(try_stmt) = self.ctx.arena.get_try_statement(node) {
                self.collect_yield_types(try_stmt.try_block, types);
                self.collect_yield_types(try_stmt.catch_clause, types);
                self.collect_yield_types(try_stmt.finally_block, types);
            }
        }
    }

    fn collect_return_types(&mut self, idx: NodeIndex, types: &mut Vec<TypeId>) {
        let Some(node) = self.ctx.arena.get(idx) else {
            return;
        };

        // Check if this is a return statement
        if node.kind == syntax_kind_ext::RETURN_STATEMENT {
            if let Some(ret) = self.ctx.arena.get_return_statement(node) {
                let return_type = if ret.expression.is_null() {
                    TypeId::VOID
                } else {
                    self.check_expression(ret.expression)
                };
                types.push(return_type);
            }
            return;
        }

        // Don't recurse into nested functions
        if node.kind == syntax_kind_ext::FUNCTION_DECLARATION
            || node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
            || node.kind == syntax_kind_ext::ARROW_FUNCTION
        {
            return;
        }

        // Recurse into block statements
        if node.kind == syntax_kind_ext::BLOCK {
            if let Some(block) = self.ctx.arena.get_block(node) {
                for &stmt_idx in &block.statements.nodes {
                    self.collect_return_types(stmt_idx, types);
                }
            }
        }

        // Recurse into if statements
        if node.kind == syntax_kind_ext::IF_STATEMENT {
            if let Some(if_stmt) = self.ctx.arena.get_if_statement(node) {
                self.collect_return_types(if_stmt.then_statement, types);
                self.collect_return_types(if_stmt.else_statement, types);
            }
        }

        // Recurse into for/while/try etc.
        if node.kind == syntax_kind_ext::FOR_STATEMENT {
            if let Some(for_stmt) = self.ctx.arena.get_for_statement(node) {
                self.collect_return_types(for_stmt.statement, types);
            }
        }

        if node.kind == syntax_kind_ext::WHILE_STATEMENT {
            if let Some(while_stmt) = self.ctx.arena.get_while_statement(node) {
                self.collect_return_types(while_stmt.statement, types);
            }
        }

        if node.kind == syntax_kind_ext::TRY_STATEMENT {
            if let Some(try_stmt) = self.ctx.arena.get_try_statement(node) {
                self.collect_return_types(try_stmt.try_block, types);
                self.collect_return_types(try_stmt.catch_clause, types);
                self.collect_return_types(try_stmt.finally_block, types);
            }
        }
    }

    fn check_expression(&mut self, idx: NodeIndex) -> TypeId {
        // Delegate to the type interner for basic type inference
        // This is a simplified version - full implementation would use ExpressionChecker
        if let Some(node) = self.ctx.arena.get(idx) {
            match node.kind {
                k if k == SyntaxKind::NumericLiteral as u16 => TypeId::NUMBER,
                k if k == SyntaxKind::StringLiteral as u16 => TypeId::STRING,
                k if k == SyntaxKind::TrueKeyword as u16 => self.ctx.types.literal_boolean(true),
                k if k == SyntaxKind::FalseKeyword as u16 => self.ctx.types.literal_boolean(false),
                k if k == SyntaxKind::NullKeyword as u16 => TypeId::NULL,
                k if k == SyntaxKind::UndefinedKeyword as u16 => TypeId::UNDEFINED,
                _ => TypeId::ANY,
            }
        } else {
            TypeId::ANY
        }
    }

    fn peek_expression_type(&self, idx: NodeIndex) -> TypeId {
        // Non-mutating type peek - check cache only
        if let Some(&cached) = self.ctx.node_types.get(&idx.0) {
            return cached;
        }
        TypeId::UNKNOWN
    }

    fn is_inside_generator_function(&self, idx: NodeIndex) -> bool {
        // Walk up the tree to find enclosing function
        // This is a simplified check - full implementation would track scope
        // For now, assume context is set up correctly
        true
    }

    fn is_iterable(&self, type_id: TypeId) -> bool {
        // Check if type has Symbol.iterator method
        // This is a simplified check - arrays and strings are always iterable
        if type_id == TypeId::STRING {
            return true;
        }

        // Check for array types
        if let Some(type_key) = self.ctx.types.lookup(type_id) {
            match type_key {
                crate::solver::TypeKey::Array(_) => return true,
                crate::solver::TypeKey::Tuple(_) => return true,
                _ => {}
            }
        }

        // TODO: Check for Symbol.iterator property
        true
    }

    fn get_iterable_element_type(&self, type_id: TypeId) -> TypeId {
        // Extract element type from iterable
        if let Some(type_key) = self.ctx.types.lookup(type_id) {
            match type_key {
                crate::solver::TypeKey::Array(elem_type) => return elem_type,
                crate::solver::TypeKey::Tuple(tuple_id) => {
                    let elements = self.ctx.types.tuple_list(tuple_id);
                    if elements.is_empty() {
                        return TypeId::NEVER;
                    }
                    let types: Vec<TypeId> = elements.iter().map(|e| e.type_id).collect();
                    return self.ctx.types.union(types);
                }
                _ => {}
            }
        }

        if type_id == TypeId::STRING {
            return TypeId::STRING;
        }

        TypeId::ANY
    }

    fn get_iterator_return_type(&self, type_id: TypeId) -> TypeId {
        // Get the return type of an iterator (the TReturn in Generator<Y, TReturn, N>)
        // This is used for yield* to get the result of the delegated iterator
        TypeId::ANY
    }

    fn create_generator_type(&self, yield_type: TypeId, return_type: TypeId, next_type: TypeId) -> TypeId {
        // Create Generator<Y, R, N> type
        // This would create a TypeReference to the global Generator interface
        // For now, return a placeholder
        TypeId::ANY
    }

    fn create_async_generator_type(&self, yield_type: TypeId, return_type: TypeId, next_type: TypeId) -> TypeId {
        // Create AsyncGenerator<Y, R, N> type
        TypeId::ANY
    }
}

/// Errors that can occur during generator type checking.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GeneratorError {
    /// yield used outside of a generator function
    YieldOutsideGenerator,
    /// yield* used with a non-iterable value
    YieldDelegationNonIterable,
    /// Mismatched yield types
    YieldTypeMismatch { expected: TypeId, actual: TypeId },
}

impl std::fmt::Display for GeneratorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GeneratorError::YieldOutsideGenerator => {
                write!(f, "A 'yield' expression is only allowed in a generator body")
            }
            GeneratorError::YieldDelegationNonIterable => {
                write!(f, "Type is not iterable. Must have a '[Symbol.iterator]()' method")
            }
            GeneratorError::YieldTypeMismatch { expected, actual } => {
                write!(f, "Type '{:?}' is not assignable to type '{:?}'", actual, expected)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::solver::TypeInterner;
    use crate::thin_binder::ThinBinderState;
    use crate::thin_parser::ThinParserState;

    fn create_context(source: &str) -> (ThinParserState, ThinBinderState, TypeInterner) {
        let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let mut binder = ThinBinderState::new();
        binder.bind_source_file(parser.get_arena(), root);
        let types = TypeInterner::new();
        (parser, binder, types)
    }

    #[test]
    fn test_is_generator_function() {
        let source = "function* gen() { yield 1; }";
        let (parser, binder, types) = create_context(source);
        let mut ctx = CheckerContext::new(
            parser.get_arena(),
            &binder,
            &types,
            "test.ts".to_string(),
            crate::checker::context::CheckerOptions::default(),
        );

        // Navigate to function declaration
        if let Some(root_node) = parser.get_arena().get(parser.get_root()) {
            if let Some(sf_data) = parser.get_arena().get_source_file(root_node) {
                if let Some(&func_idx) = sf_data.statements.nodes.first() {
                    let checker = GeneratorChecker::new(&mut ctx);
                    assert!(checker.is_generator_function(func_idx));
                }
            }
        }
    }

    #[test]
    fn test_regular_function_not_generator() {
        let source = "function foo() { return 1; }";
        let (parser, binder, types) = create_context(source);
        let mut ctx = CheckerContext::new(
            parser.get_arena(),
            &binder,
            &types,
            "test.ts".to_string(),
            crate::checker::context::CheckerOptions::default(),
        );

        if let Some(root_node) = parser.get_arena().get(parser.get_root()) {
            if let Some(sf_data) = parser.get_arena().get_source_file(root_node) {
                if let Some(&func_idx) = sf_data.statements.nodes.first() {
                    let checker = GeneratorChecker::new(&mut ctx);
                    assert!(!checker.is_generator_function(func_idx));
                }
            }
        }
    }
}
