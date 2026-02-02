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
use crate::parser::NodeIndex;
use crate::parser::syntax_kind_ext;
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

        // Try to find the global Generator type from lib contexts
        // TSC emits TS2318 when Generator is not available
        if let Some(gen_base) = self.lookup_global_type("Generator") {
            // Generator extends IterableIterator, so check for that too
            // TSC emits TS2318 for IterableIterator when processing generators
            if !self.ctx.has_name_in_lib("IterableIterator") {
                use crate::lib_loader;
                self.ctx
                    .push_diagnostic(lib_loader::emit_error_global_type_missing(
                        "IterableIterator",
                        self.ctx.file_name.clone(),
                        0,
                        0,
                    ));
            }
            return self.ctx.types.application(
                gen_base,
                vec![info.yield_type, info.return_type, info.next_type],
            );
        }

        // Generator global not found - emit TS2318 regardless of noLib setting.
        // TSC emits this error even with noLib: true when generator functions are used.
        use crate::lib_loader;
        self.ctx
            .push_diagnostic(lib_loader::emit_error_global_type_missing(
                "Generator",
                self.ctx.file_name.clone(),
                0,
                0,
            ));

        // Also check for IterableIterator (Generator extends it)
        if !self.ctx.has_name_in_lib("IterableIterator") {
            self.ctx
                .push_diagnostic(lib_loader::emit_error_global_type_missing(
                    "IterableIterator",
                    self.ctx.file_name.clone(),
                    0,
                    0,
                ));
        }

        // Fall back to structural Generator type
        self.create_generator_type(info.yield_type, info.return_type, info.next_type)
    }

    /// Get the return type of an async generator function (AsyncGenerator<Y, R, N>).
    pub fn get_async_generator_return_type(&mut self, func_idx: NodeIndex) -> TypeId {
        let info = self.infer_generator_type(func_idx);

        // Try to find the global AsyncGenerator type from lib contexts
        // TSC emits TS2318 when AsyncGenerator is not available
        if let Some(async_gen_base) = self.lookup_global_type("AsyncGenerator") {
            // AsyncGenerator extends AsyncIterableIterator, so check for that too
            // TSC emits TS2318 for AsyncIterableIterator when processing async generators
            if !self.ctx.has_name_in_lib("AsyncIterableIterator") {
                use crate::lib_loader;
                self.ctx
                    .push_diagnostic(lib_loader::emit_error_global_type_missing(
                        "AsyncIterableIterator",
                        self.ctx.file_name.clone(),
                        0,
                        0,
                    ));
            }
            return self.ctx.types.application(
                async_gen_base,
                vec![info.yield_type, info.return_type, info.next_type],
            );
        }

        // AsyncGenerator global not found - emit TS2318 regardless of noLib setting.
        // TSC emits this error even with noLib: true when async generator functions are used.
        use crate::lib_loader;
        self.ctx
            .push_diagnostic(lib_loader::emit_error_global_type_missing(
                "AsyncGenerator",
                self.ctx.file_name.clone(),
                0,
                0,
            ));

        // Also check for AsyncIterableIterator (AsyncGenerator extends it)
        if !self.ctx.has_name_in_lib("AsyncIterableIterator") {
            self.ctx
                .push_diagnostic(lib_loader::emit_error_global_type_missing(
                    "AsyncIterableIterator",
                    self.ctx.file_name.clone(),
                    0,
                    0,
                ));
        }

        // Fall back to structural AsyncGenerator type
        self.create_async_generator_type(info.yield_type, info.return_type, info.next_type)
    }

    // =========================================================================
    // Helper methods
    // =========================================================================

    fn get_function_body(&self, node: &crate::parser::node::Node) -> NodeIndex {
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
        use crate::solver::judge::IterableKind;
        // Use Judge's classify_iterable for type classification
        !matches!(
            self.judge_classify_iterable(type_id),
            IterableKind::NotIterable
        )
    }

    fn get_iterable_element_type(&self, type_id: TypeId) -> TypeId {
        use crate::solver::judge::IterableKind;
        // Use Judge's classify_iterable to get element type information
        match self.judge_classify_iterable(type_id) {
            IterableKind::Array(elem) => elem,
            IterableKind::Tuple(elems) => {
                if elems.is_empty() {
                    TypeId::NEVER
                } else {
                    self.ctx.types.union(elems)
                }
            }
            IterableKind::String => TypeId::STRING,
            IterableKind::SyncIterator { element_type, .. } => element_type,
            IterableKind::AsyncIterator { element_type, .. } => element_type,
            IterableKind::NotIterable => TypeId::ANY,
        }

        if type_id == TypeId::STRING {
            return TypeId::STRING;
        }

        TypeId::ANY
    }

    fn get_iterator_return_type(&self, type_id: TypeId) -> TypeId {
        // Get the return type of an iterator (the TReturn in Generator<Y, TReturn, N>)
        // This is used for yield* to get the result of the delegated iterator
        // Check if this is a generator-like type and extract TReturn
        if let Some(type_key) = self.ctx.types.lookup(type_id) {
            match type_key {
                crate::solver::TypeKey::Object(shape_id) => {
                    let shape = self.ctx.types.object_shape(shape_id);
                    // Look for a 'return' method and extract its return type
                    for prop in &shape.properties {
                        let prop_name = self.ctx.types.resolve_atom_ref(prop.name);
                        if prop_name.as_ref() == "return" && prop.is_method {
                            // Extract the return type from the method's return
                            return self.extract_iterator_result_value(prop.type_id);
                        }
                    }
                }
                _ => {}
            }
        }
        TypeId::ANY
    }

    /// Extract the value type from an IteratorResult<T, TReturn>-like type
    fn extract_iterator_result_value(&self, type_id: TypeId) -> TypeId {
        if let Some(type_key) = self.ctx.types.lookup(type_id) {
            match type_key {
                crate::solver::TypeKey::Object(shape_id) => {
                    let shape = self.ctx.types.object_shape(shape_id);
                    for prop in &shape.properties {
                        let prop_name = self.ctx.types.resolve_atom_ref(prop.name);
                        if prop_name.as_ref() == "value" {
                            return prop.type_id;
                        }
                    }
                }
                _ => {}
            }
        }
        TypeId::ANY
    }

    /// Create a Generator<Y, R, N> type as a structural object type.
    ///
    /// Generator<Y, R, N> has the following structure:
    /// ```typescript
    /// interface Generator<T = unknown, TReturn = any, TNext = unknown> {
    ///   next(...args: [] | [TNext]): IteratorResult<T, TReturn>;
    ///   return(value?: TReturn): IteratorResult<T, TReturn>;
    ///   throw(e?: any): IteratorResult<T, TReturn>;
    ///   [Symbol.iterator](): Generator<T, TReturn, TNext>;
    /// }
    /// ```
    fn create_generator_type(
        &self,
        yield_type: TypeId,
        return_type: TypeId,
        next_type: TypeId,
    ) -> TypeId {
        // Create IteratorResult<Y, R> = { value: Y; done: false } | { value: R; done: true }
        let iterator_result = self.create_iterator_result_type(yield_type, return_type);

        // Create the Generator interface with next, return, throw methods
        let next_name = self.ctx.types.intern_string("next");
        let return_name = self.ctx.types.intern_string("return");
        let throw_name = self.ctx.types.intern_string("throw");

        // next method signature: (value?: TNext) => IteratorResult<Y, R>
        let next_method = self.ctx.types.function(crate::solver::FunctionShape {
            type_params: vec![],
            params: vec![crate::solver::ParamInfo {
                name: Some(self.ctx.types.intern_string("value")),
                type_id: next_type,
                optional: true,
                rest: false,
            }],
            this_type: None,
            return_type: iterator_result,
            type_predicate: None,
            is_constructor: false,
            is_method: true,
        });

        // return method signature: (value?: R) => IteratorResult<Y, R>
        let return_method = self.ctx.types.function(crate::solver::FunctionShape {
            type_params: vec![],
            params: vec![crate::solver::ParamInfo {
                name: Some(self.ctx.types.intern_string("value")),
                type_id: return_type,
                optional: true,
                rest: false,
            }],
            this_type: None,
            return_type: iterator_result,
            type_predicate: None,
            is_constructor: false,
            is_method: true,
        });

        // throw method signature: (e?: any) => IteratorResult<Y, R>
        let throw_method = self.ctx.types.function(crate::solver::FunctionShape {
            type_params: vec![],
            params: vec![crate::solver::ParamInfo {
                name: Some(self.ctx.types.intern_string("e")),
                type_id: TypeId::ANY,
                optional: true,
                rest: false,
            }],
            this_type: None,
            return_type: iterator_result,
            type_predicate: None,
            is_constructor: false,
            is_method: true,
        });

        // Create Generator object type
        self.ctx.types.object(vec![
            crate::solver::PropertyInfo {
                name: next_name,
                type_id: next_method,
                write_type: next_method,
                optional: false,
                readonly: true,
                is_method: true,
            },
            crate::solver::PropertyInfo {
                name: return_name,
                type_id: return_method,
                write_type: return_method,
                optional: false,
                readonly: true,
                is_method: true,
            },
            crate::solver::PropertyInfo {
                name: throw_name,
                type_id: throw_method,
                write_type: throw_method,
                optional: false,
                readonly: true,
                is_method: true,
            },
        ])
    }

    /// Create an AsyncGenerator<Y, R, N> type as a structural object type.
    ///
    /// AsyncGenerator<Y, R, N> has the following structure:
    /// ```typescript
    /// interface AsyncGenerator<T = unknown, TReturn = any, TNext = unknown> {
    ///   next(...args: [] | [TNext]): Promise<IteratorResult<T, TReturn>>;
    ///   return(value?: TReturn | PromiseLike<TReturn>): Promise<IteratorResult<T, TReturn>>;
    ///   throw(e?: any): Promise<IteratorResult<T, TReturn>>;
    ///   [Symbol.asyncIterator](): AsyncGenerator<T, TReturn, TNext>;
    /// }
    /// ```
    fn create_async_generator_type(
        &self,
        yield_type: TypeId,
        return_type: TypeId,
        next_type: TypeId,
    ) -> TypeId {
        // Create IteratorResult<Y, R> = { value: Y; done: false } | { value: R; done: true }
        let iterator_result = self.create_iterator_result_type(yield_type, return_type);

        // Wrap in Promise-like: { then: IteratorResult<Y, R> }
        // This is a simplified Promise representation for structural matching
        let promise_iterator_result = self.create_promise_type(iterator_result);

        // Create the AsyncGenerator interface with next, return, throw methods
        let next_name = self.ctx.types.intern_string("next");
        let return_name = self.ctx.types.intern_string("return");
        let throw_name = self.ctx.types.intern_string("throw");

        // next method signature: (value?: TNext) => Promise<IteratorResult<Y, R>>
        let next_method = self.ctx.types.function(crate::solver::FunctionShape {
            type_params: vec![],
            params: vec![crate::solver::ParamInfo {
                name: Some(self.ctx.types.intern_string("value")),
                type_id: next_type,
                optional: true,
                rest: false,
            }],
            this_type: None,
            return_type: promise_iterator_result,
            type_predicate: None,
            is_constructor: false,
            is_method: true,
        });

        // return method signature: (value?: R) => Promise<IteratorResult<Y, R>>
        let return_method = self.ctx.types.function(crate::solver::FunctionShape {
            type_params: vec![],
            params: vec![crate::solver::ParamInfo {
                name: Some(self.ctx.types.intern_string("value")),
                type_id: return_type,
                optional: true,
                rest: false,
            }],
            this_type: None,
            return_type: promise_iterator_result,
            type_predicate: None,
            is_constructor: false,
            is_method: true,
        });

        // throw method signature: (e?: any) => Promise<IteratorResult<Y, R>>
        let throw_method = self.ctx.types.function(crate::solver::FunctionShape {
            type_params: vec![],
            params: vec![crate::solver::ParamInfo {
                name: Some(self.ctx.types.intern_string("e")),
                type_id: TypeId::ANY,
                optional: true,
                rest: false,
            }],
            this_type: None,
            return_type: promise_iterator_result,
            type_predicate: None,
            is_constructor: false,
            is_method: true,
        });

        // Create AsyncGenerator object type
        self.ctx.types.object(vec![
            crate::solver::PropertyInfo {
                name: next_name,
                type_id: next_method,
                write_type: next_method,
                optional: false,
                readonly: true,
                is_method: true,
            },
            crate::solver::PropertyInfo {
                name: return_name,
                type_id: return_method,
                write_type: return_method,
                optional: false,
                readonly: true,
                is_method: true,
            },
            crate::solver::PropertyInfo {
                name: throw_name,
                type_id: throw_method,
                write_type: throw_method,
                optional: false,
                readonly: true,
                is_method: true,
            },
        ])
    }

    /// Create an IteratorResult<Y, R> type.
    /// IteratorResult<Y, R> = { done?: false; value: Y } | { done: true; value: R }
    fn create_iterator_result_type(&self, yield_type: TypeId, return_type: TypeId) -> TypeId {
        // Try to find the global IteratorResult type from lib contexts
        if let Some(iterator_result_base) = self.lookup_global_type("IteratorResult") {
            return self
                .ctx
                .types
                .application(iterator_result_base, vec![yield_type, return_type]);
        }

        // Fallback: create structural IteratorResult<T, TReturn>
        let done_name = self.ctx.types.intern_string("done");
        let value_name = self.ctx.types.intern_string("value");

        // IteratorYieldResult<Y> = { done?: false; value: Y }
        // Note: done is optional per TypeScript spec
        let yield_result = self.ctx.types.object(vec![
            crate::solver::PropertyInfo {
                name: done_name,
                type_id: self.ctx.types.literal_boolean(false),
                write_type: self.ctx.types.literal_boolean(false),
                optional: true, // done?: false
                readonly: false,
                is_method: false,
            },
            crate::solver::PropertyInfo {
                name: value_name,
                type_id: yield_type,
                write_type: yield_type,
                optional: false,
                readonly: false,
                is_method: false,
            },
        ]);

        // IteratorReturnResult<R> = { done: true; value: R }
        let return_result = self.ctx.types.object(vec![
            crate::solver::PropertyInfo {
                name: done_name,
                type_id: self.ctx.types.literal_boolean(true),
                write_type: self.ctx.types.literal_boolean(true),
                optional: false, // done: true (required)
                readonly: false,
                is_method: false,
            },
            crate::solver::PropertyInfo {
                name: value_name,
                type_id: return_type,
                write_type: return_type,
                optional: false,
                readonly: false,
                is_method: false,
            },
        ]);

        // IteratorResult<Y, R> = IteratorYieldResult<Y> | IteratorReturnResult<R>
        self.ctx.types.union2(yield_result, return_result)
    }

    /// Create Promise<T> type.
    fn create_promise_type(&self, inner_type: TypeId) -> TypeId {
        // Try to find the global Promise interface from lib contexts
        if let Some(promise_base) = self.lookup_global_type("Promise") {
            return self.ctx.types.application(promise_base, vec![inner_type]);
        }

        // Fallback: use the synthetic Promise base type
        // This allows the type to be recognized as promise-like even without lib types
        self.ctx
            .types
            .application(TypeId::PROMISE_BASE, vec![inner_type])
    }

    /// Look up a global type by name from lib contexts.
    fn lookup_global_type(&self, name: &str) -> Option<TypeId> {
        use crate::solver::TypeLowering;

        for lib_ctx in &self.ctx.lib_contexts {
            if let Some(sym_id) = lib_ctx.binder.file_locals.get(name) {
                if let Some(symbol) = lib_ctx.binder.get_symbol(sym_id) {
                    // Lower the type from the lib file's arena
                    let lowering = TypeLowering::new(lib_ctx.arena.as_ref(), self.ctx.types);
                    // For interfaces, use all declarations (handles declaration merging)
                    if !symbol.declarations.is_empty() {
                        return Some(lowering.lower_interface_declarations(&symbol.declarations));
                    }
                    // For type aliases and other single-declaration types
                    let decl_idx = symbol.value_declaration;
                    if decl_idx.0 != u32::MAX {
                        return Some(lowering.lower_type(decl_idx));
                    }
                }
            }
        }

        // Also check the current file's file_locals
        if let Some(sym_id) = self.ctx.binder.file_locals.get(name) {
            if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
                let lowering = crate::solver::TypeLowering::new(self.ctx.arena, self.ctx.types);
                if !symbol.declarations.is_empty() {
                    return Some(lowering.lower_interface_declarations(&symbol.declarations));
                }
            }
        }

        None
    }
}

/// Extract the yield type from an AsyncGenerator type for for-await-of loops.
pub fn get_async_iterable_element_type(
    types: &crate::solver::TypeInterner,
    type_id: TypeId,
) -> TypeId {
    if let Some(type_key) = types.lookup(type_id) {
        match type_key {
            crate::solver::TypeKey::Object(shape_id) => {
                let shape = types.object_shape(shape_id);
                // Look for the 'next' method which returns Promise<IteratorResult<T, R>>
                for prop in &shape.properties {
                    let prop_name = types.resolve_atom_ref(prop.name);
                    if prop_name.as_ref() == "next" && prop.is_method {
                        // Extract the element type from the Promise<IteratorResult<T, R>>
                        return extract_async_iterator_element(types, prop.type_id);
                    }
                }
            }
            crate::solver::TypeKey::Array(elem) => return elem,
            _ => {}
        }
    }
    TypeId::ANY
}

/// Extract element type from a Promise<IteratorResult<T, R>> structure.
fn extract_async_iterator_element(
    types: &crate::solver::TypeInterner,
    method_type: TypeId,
) -> TypeId {
    // The method type should be a function
    if let Some(crate::solver::TypeKey::Function(func_id)) = types.lookup(method_type) {
        let func = types.function_shape(func_id);
        // Get the return type which should be Promise<IteratorResult<T, R>>
        let promise_type = func.return_type;

        // Extract from Promise-like structure
        if let Some(crate::solver::TypeKey::Object(shape_id)) = types.lookup(promise_type) {
            let shape = types.object_shape(shape_id);
            for prop in &shape.properties {
                let prop_name = types.resolve_atom_ref(prop.name);
                if prop_name.as_ref() == "then" {
                    // The 'then' property holds the inner type (IteratorResult<T, R>)
                    return extract_iterator_result_yield_type(types, prop.type_id);
                }
            }
        }
    }
    TypeId::ANY
}

/// Extract yield type from an IteratorResult<T, R> structure.
fn extract_iterator_result_yield_type(
    types: &crate::solver::TypeInterner,
    result_type: TypeId,
) -> TypeId {
    // IteratorResult is a union: { value: Y; done: false } | { value: R; done: true }
    if let Some(type_key) = types.lookup(result_type) {
        match type_key {
            crate::solver::TypeKey::Union(list_id) => {
                let members = types.type_list(list_id);
                // Get the first member (yield result) and extract value
                if let Some(&first) = members.first() {
                    return extract_value_from_object(types, first);
                }
            }
            crate::solver::TypeKey::Object(shape_id) => {
                return extract_value_from_object_shape(types, shape_id);
            }
            _ => {}
        }
    }
    TypeId::ANY
}

/// Extract 'value' property from an object type.
fn extract_value_from_object(types: &crate::solver::TypeInterner, type_id: TypeId) -> TypeId {
    if let Some(crate::solver::TypeKey::Object(shape_id)) = types.lookup(type_id) {
        return extract_value_from_object_shape(types, shape_id);
    }
    TypeId::ANY
}

/// Extract 'value' property from an object shape.
fn extract_value_from_object_shape(
    types: &crate::solver::TypeInterner,
    shape_id: crate::solver::ObjectShapeId,
) -> TypeId {
    let shape = types.object_shape(shape_id);
    for prop in &shape.properties {
        let prop_name = types.resolve_atom_ref(prop.name);
        if prop_name.as_ref() == "value" {
            return prop.type_id;
        }
    }
    TypeId::ANY
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
                write!(
                    f,
                    "A 'yield' expression is only allowed in a generator body"
                )
            }
            GeneratorError::YieldDelegationNonIterable => {
                write!(
                    f,
                    "Type is not iterable. Must have a '[Symbol.iterator]()' method"
                )
            }
            GeneratorError::YieldTypeMismatch { expected, actual } => {
                write!(
                    f,
                    "Type '{:?}' is not assignable to type '{:?}'",
                    actual, expected
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binder::BinderState;
    use crate::parser::ParserState;
    use crate::solver::TypeInterner;

    fn create_context(source: &str) -> (ParserState, BinderState, TypeInterner) {
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let mut binder = BinderState::new();
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

    #[test]
    fn test_is_async_generator_function() {
        let source = "async function* gen() { yield 1; }";
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
                    assert!(checker.is_async_generator_function(func_idx));
                }
            }
        }
    }

    #[test]
    fn test_create_async_generator_type_has_methods() {
        // Test that create_async_generator_type creates a valid type structure
        let source = "async function* gen() { yield 1; }";
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
                    let mut checker = GeneratorChecker::new(&mut ctx);
                    let async_gen_type = checker.get_async_generator_return_type(func_idx);

                    // Verify it's not just TypeId::ANY anymore
                    // The created type should have proper structure
                    if let Some(type_key) = types.lookup(async_gen_type) {
                        match type_key {
                            crate::solver::TypeKey::Object(shape_id) => {
                                let shape = types.object_shape(shape_id);
                                // Should have next, return, throw methods
                                let has_next = shape
                                    .properties
                                    .iter()
                                    .any(|p| types.resolve_atom_ref(p.name).as_ref() == "next");
                                let has_return = shape
                                    .properties
                                    .iter()
                                    .any(|p| types.resolve_atom_ref(p.name).as_ref() == "return");
                                let has_throw = shape
                                    .properties
                                    .iter()
                                    .any(|p| types.resolve_atom_ref(p.name).as_ref() == "throw");
                                assert!(has_next, "AsyncGenerator should have 'next' method");
                                assert!(has_return, "AsyncGenerator should have 'return' method");
                                assert!(has_throw, "AsyncGenerator should have 'throw' method");
                            }
                            _ => panic!("Expected Object type key for AsyncGenerator"),
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn test_async_generator_yield_type_extraction() {
        // Test that we can extract the yield type from an AsyncGenerator
        let types = TypeInterner::new();

        // Create AsyncGenerator<number, void, unknown> structure manually
        let yield_type = TypeId::NUMBER;
        let return_type = TypeId::VOID;
        let next_type = TypeId::UNKNOWN;

        // Create IteratorResult<number, void>
        let value_name = types.intern_string("value");
        let done_name = types.intern_string("done");
        let yield_result = types.object(vec![
            crate::solver::PropertyInfo {
                name: value_name,
                type_id: yield_type,
                write_type: yield_type,
                optional: false,
                readonly: true,
                is_method: false,
            },
            crate::solver::PropertyInfo {
                name: done_name,
                type_id: types.literal_boolean(false),
                write_type: types.literal_boolean(false),
                optional: false,
                readonly: true,
                is_method: false,
            },
        ]);

        // Wrap in Promise-like
        let then_name = types.intern_string("then");
        let promise_result = types.object(vec![crate::solver::PropertyInfo {
            name: then_name,
            type_id: yield_result,
            write_type: yield_result,
            optional: false,
            readonly: true,
            is_method: true,
        }]);

        // Create next method
        let next_method = types.function(crate::solver::FunctionShape {
            type_params: vec![],
            params: vec![crate::solver::ParamInfo {
                name: Some(types.intern_string("value")),
                type_id: next_type,
                optional: true,
                rest: false,
            }],
            this_type: None,
            return_type: promise_result,
            type_predicate: None,
            is_constructor: false,
            is_method: true,
        });

        // Create AsyncGenerator-like object
        let next_name = types.intern_string("next");
        let async_gen = types.object(vec![crate::solver::PropertyInfo {
            name: next_name,
            type_id: next_method,
            write_type: next_method,
            optional: false,
            readonly: true,
            is_method: true,
        }]);

        // Extract the element type
        let extracted = get_async_iterable_element_type(&types, async_gen);

        // Should be number (the yield type)
        assert_eq!(
            extracted,
            TypeId::NUMBER,
            "Should extract number as yield type from AsyncGenerator<number, void, unknown>"
        );
    }
}
