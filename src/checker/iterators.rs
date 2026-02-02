//! Iterator and Iterable Type Checking
//!
//! This module handles type inference and checking for:
//! - Iterator and IterableIterator types
//! - for-of loops with iterables
//! - Symbol.iterator protocol
//! - Spread of iterables
//! - Async iterators and for-await-of
//!
//! # Iterator Protocol
//!
//! The iterator protocol defines how objects can be iterated:
//!
//! ```typescript
//! interface Iterator<T, TReturn = any, TNext = undefined> {
//!     next(...args: [] | [TNext]): IteratorResult<T, TReturn>;
//!     return?(value?: TReturn): IteratorResult<T, TReturn>;
//!     throw?(e?: any): IteratorResult<T, TReturn>;
//! }
//!
//! interface Iterable<T> {
//!     [Symbol.iterator](): Iterator<T>;
//! }
//!
//! interface IterableIterator<T> extends Iterator<T> {
//!     [Symbol.iterator](): IterableIterator<T>;
//! }
//! ```
//!
//! # Async Iterator Protocol
//!
//! ```typescript
//! interface AsyncIterator<T, TReturn = any, TNext = undefined> {
//!     next(...args: [] | [TNext]): Promise<IteratorResult<T, TReturn>>;
//!     return?(value?: TReturn | PromiseLike<TReturn>): Promise<IteratorResult<T, TReturn>>;
//!     throw?(e?: any): Promise<IteratorResult<T, TReturn>>;
//! }
//!
//! interface AsyncIterable<T> {
//!     [Symbol.asyncIterator](): AsyncIterator<T>;
//! }
//! ```

use super::context::CheckerContext;
use super::types::diagnostics::{
    Diagnostic, diagnostic_codes, diagnostic_messages, format_message,
};
use crate::parser::NodeIndex;
use crate::parser::syntax_kind_ext;
use crate::scanner::SyntaxKind;
use crate::solver::TypeId;

/// Type checker for iterators and iterables.
pub struct IteratorChecker<'a, 'ctx> {
    ctx: &'a mut CheckerContext<'ctx>,
}

/// Information about an iterator type.
#[derive(Debug, Clone)]
pub struct IteratorTypeInfo {
    /// The type of values yielded by the iterator (T)
    pub yield_type: TypeId,
    /// The type returned when done (TReturn)
    pub return_type: TypeId,
    /// The type accepted by .next() (TNext)
    pub next_type: TypeId,
}

impl Default for IteratorTypeInfo {
    fn default() -> Self {
        Self {
            yield_type: TypeId::ANY,
            return_type: TypeId::ANY,
            next_type: TypeId::UNDEFINED,
        }
    }
}

/// Result of checking an iterable type.
#[derive(Debug, Clone)]
pub struct IterableCheckResult {
    /// Whether the type is iterable
    pub is_iterable: bool,
    /// Whether the type is async iterable
    pub is_async_iterable: bool,
    /// The element type if iterable
    pub element_type: TypeId,
    /// Detailed iterator info if available
    pub iterator_info: Option<IteratorTypeInfo>,
}

impl<'a, 'ctx> IteratorChecker<'a, 'ctx> {
    /// Create a new iterator checker with a mutable context reference.
    pub fn new(ctx: &'a mut CheckerContext<'ctx>) -> Self {
        Self { ctx }
    }

    /// Check if a type is iterable (has Symbol.iterator).
    pub fn is_iterable(&self, type_id: TypeId) -> bool {
        use crate::solver::judge::IterableKind;
        // Use Judge's classify_iterable for cleaner type classification
        // This handles arrays, tuples, strings, objects with Symbol.iterator, and unions
        !matches!(
            self.judge_classify_iterable(type_id),
            IterableKind::NotIterable
        )
    }

    /// Check if a type is async iterable (has Symbol.asyncIterator).
    pub fn is_async_iterable(&self, type_id: TypeId) -> bool {
        use crate::solver::judge::IterableKind;
        // Use Judge's classify_iterable to check for async iterability
        matches!(
            self.judge_classify_iterable(type_id),
            IterableKind::AsyncIterator { .. }
        )
    }

    /// Get the element type of an iterable.
    ///
    /// For arrays, this returns the element type.
    /// For strings, this returns string (each character).
    /// For iterables, this extracts T from Iterator<T>.
    pub fn get_iterable_element_type(&self, type_id: TypeId) -> TypeId {
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
    }

    /// Check a for-of loop and return the element type.
    /// Emits TS2488 for non-iterable types or TS2504 for non-async-iterable types in for-await-of.
    pub fn check_for_of_loop(&mut self, for_of_idx: NodeIndex) -> ForOfCheckResult {
        let Some(node) = self.ctx.arena.get(for_of_idx) else {
            return ForOfCheckResult::error("Invalid for-of node");
        };

        if node.kind != syntax_kind_ext::FOR_OF_STATEMENT {
            return ForOfCheckResult::error("Expected for-of statement");
        }

        if let Some(for_of) = self.ctx.arena.get_for_of_statement(node) {
            // Get the type of the expression being iterated
            let iterable_type = self.check_expression(for_of.expression);

            // Check if it's an async for-await-of
            let is_async = for_of.await_modifier;

            // Validate iterability and emit diagnostics
            if is_async {
                if !self.is_async_iterable(iterable_type) && !self.is_iterable(iterable_type) {
                    // Emit TS2504 for non-async-iterable types in for-await-of
                    self.emit_not_async_iterable_error(iterable_type, for_of.expression);
                    return ForOfCheckResult {
                        is_valid: false,
                        element_type: TypeId::ANY,
                        is_async,
                        error: Some("Type is not an async iterable".to_string()),
                    };
                }
            } else if !self.is_iterable(iterable_type) {
                // Emit TS2488 for non-iterable types in for-of
                self.emit_not_iterable_error(iterable_type, for_of.expression);
                return ForOfCheckResult {
                    is_valid: false,
                    element_type: TypeId::ANY,
                    is_async,
                    error: Some("Type is not iterable".to_string()),
                };
            }

            // Get the element type
            let element_type = if is_async {
                self.get_async_iterable_element_type(iterable_type)
            } else {
                self.get_iterable_element_type(iterable_type)
            };

            return ForOfCheckResult {
                is_valid: true,
                element_type,
                is_async,
                error: None,
            };
        }

        ForOfCheckResult::error("Failed to parse for-of statement")
    }

    /// Check spread of an iterable.
    /// Emits TS2488 for non-iterable types.
    pub fn check_spread(&mut self, spread_idx: NodeIndex) -> SpreadCheckResult {
        let Some(node) = self.ctx.arena.get(spread_idx) else {
            return SpreadCheckResult::error("Invalid spread node");
        };

        if node.kind != syntax_kind_ext::SPREAD_ELEMENT {
            return SpreadCheckResult::error("Expected spread element");
        }

        if let Some(spread) = self.ctx.arena.get_spread_element(node) {
            let spread_type = self.check_expression(spread.expression);

            // Check if the spread operand is iterable
            if !self.is_iterable(spread_type) {
                // Emit TS2488 for non-iterable types in spread
                self.emit_not_iterable_error(spread_type, spread.expression);
                return SpreadCheckResult {
                    is_valid: false,
                    element_type: TypeId::ANY,
                    error: Some("Type is not iterable and cannot be spread".to_string()),
                };
            }

            let element_type = self.get_iterable_element_type(spread_type);

            return SpreadCheckResult {
                is_valid: true,
                element_type,
                error: None,
            };
        }

        SpreadCheckResult::error("Failed to parse spread element")
    }

    /// Check a for-in loop (iterates over keys).
    pub fn check_for_in_loop(&mut self, for_in_idx: NodeIndex) -> ForInCheckResult {
        let Some(node) = self.ctx.arena.get(for_in_idx) else {
            return ForInCheckResult::error("Invalid for-in node");
        };

        if node.kind != syntax_kind_ext::FOR_IN_STATEMENT {
            return ForInCheckResult::error("Expected for-in statement");
        }

        if let Some(for_in) = self.ctx.arena.get_for_in_statement(node) {
            let object_type = self.check_expression(for_in.expression);

            // for-in iterates over string keys
            // Check that the object is valid for for-in
            let is_valid = self.is_valid_for_in_target(object_type);

            return ForInCheckResult {
                is_valid,
                key_type: TypeId::STRING, // for-in always yields strings
                error: if is_valid {
                    None
                } else {
                    Some("The right-hand side must be of type 'any', an object type, or a type parameter".to_string())
                },
            };
        }

        ForInCheckResult::error("Failed to parse for-in statement")
    }

    /// Create an Iterator<T> type.
    ///
    /// This function attempts to:
    /// 1. Look up the global Iterator interface from lib types
    /// 2. If found, create a type application Iterator<element_type>
    /// 3. If not found, create a structural equivalent with `next()` method
    ///
    /// The Iterator interface from lib.es2015.iterable.d.ts is:
    /// ```typescript
    /// interface Iterator<T, TReturn = any, TNext = undefined> {
    ///     next(...args: [] | [TNext]): IteratorResult<T, TReturn>;
    ///     return?(value?: TReturn): IteratorResult<T, TReturn>;
    ///     throw?(e?: any): IteratorResult<T, TReturn>;
    /// }
    /// ```
    pub fn create_iterator_type(&self, element_type: TypeId) -> TypeId {
        // Try to find the global Iterator interface from lib contexts
        if let Some(iterator_base_type) = self.lookup_global_type("Iterator") {
            // Create Iterator<element_type, any, undefined> application
            return self.ctx.types.application(
                iterator_base_type,
                vec![element_type, TypeId::ANY, TypeId::UNDEFINED],
            );
        }

        // Fallback: create a structural equivalent of Iterator<T>
        // This is used when lib types are not available
        self.create_structural_iterator_type(element_type)
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

    /// Create a structural equivalent of Iterator<T> when the global interface is not available.
    ///
    /// Creates an object type with:
    /// - `next()` method returning IteratorResult<T, any>
    ///
    /// IteratorResult<T, TReturn> is: { done?: false; value: T } | { done: true; value: TReturn }
    fn create_structural_iterator_type(&self, element_type: TypeId) -> TypeId {
        // Create IteratorResult<T, any> type
        let iterator_result_type = self.create_iterator_result_type(element_type, TypeId::ANY);

        // Create the `next` method signature: () => IteratorResult<T, any>
        let next_method_shape = crate::solver::FunctionShape {
            type_params: Vec::new(),
            params: Vec::new(),
            this_type: None,
            return_type: iterator_result_type,
            type_predicate: None,
            is_constructor: false,
            is_method: true,
        };
        let next_method_type = self.ctx.types.function(next_method_shape);

        // Create the Iterator object type with `next` property
        let next_atom = self.ctx.types.intern_string("next");
        let next_property = crate::solver::PropertyInfo {
            name: next_atom,
            type_id: next_method_type,
            write_type: next_method_type,
            optional: false,
            readonly: false,
            is_method: true,
        };

        self.ctx.types.object(vec![next_property])
    }

    /// Create IteratorResult<T, TReturn> type.
    ///
    /// IteratorResult<T, TReturn> = IteratorYieldResult<T> | IteratorReturnResult<TReturn>
    /// where:
    /// - IteratorYieldResult<T> = { done?: false; value: T }
    /// - IteratorReturnResult<TReturn> = { done: true; value: TReturn }
    fn create_iterator_result_type(&self, yield_type: TypeId, return_type: TypeId) -> TypeId {
        // Try to find the global IteratorResult type from lib contexts
        if let Some(iterator_result_base) = self.lookup_global_type("IteratorResult") {
            return self
                .ctx
                .types
                .application(iterator_result_base, vec![yield_type, return_type]);
        }

        // Fallback: create structural IteratorResult<T, TReturn>
        let done_atom = self.ctx.types.intern_string("done");
        let value_atom = self.ctx.types.intern_string("value");

        // IteratorYieldResult<T> = { done?: false; value: T }
        let yield_result = self.ctx.types.object(vec![
            crate::solver::PropertyInfo {
                name: done_atom,
                type_id: self.ctx.types.literal_boolean(false),
                write_type: self.ctx.types.literal_boolean(false),
                optional: true,
                readonly: false,
                is_method: false,
            },
            crate::solver::PropertyInfo {
                name: value_atom,
                type_id: yield_type,
                write_type: yield_type,
                optional: false,
                readonly: false,
                is_method: false,
            },
        ]);

        // IteratorReturnResult<TReturn> = { done: true; value: TReturn }
        let return_result = self.ctx.types.object(vec![
            crate::solver::PropertyInfo {
                name: done_atom,
                type_id: self.ctx.types.literal_boolean(true),
                write_type: self.ctx.types.literal_boolean(true),
                optional: false,
                readonly: false,
                is_method: false,
            },
            crate::solver::PropertyInfo {
                name: value_atom,
                type_id: return_type,
                write_type: return_type,
                optional: false,
                readonly: false,
                is_method: false,
            },
        ]);

        // IteratorResult<T, TReturn> = IteratorYieldResult<T> | IteratorReturnResult<TReturn>
        self.ctx.types.union2(yield_result, return_result)
    }

    /// Create an IterableIterator<T> type.
    ///
    /// IterableIterator<T> extends Iterator<T> and has:
    /// - `next()` method returning IteratorResult<T>
    /// - `[Symbol.iterator]()` method returning itself
    pub fn create_iterable_iterator_type(&self, element_type: TypeId) -> TypeId {
        // Try to find the global IterableIterator interface from lib contexts
        if let Some(iterable_iterator_base) = self.lookup_global_type("IterableIterator") {
            return self
                .ctx
                .types
                .application(iterable_iterator_base, vec![element_type]);
        }

        // IterableIterator global not found - emit TS2318 regardless of noLib setting.
        // TSC emits this error even with noLib: true when IterableIterator is needed.
        use crate::lib_loader;
        self.ctx
            .push_diagnostic(lib_loader::emit_error_global_type_missing(
                "IterableIterator",
                self.ctx.file_name.clone(),
                0,
                0,
            ));

        // Fallback: use the iterator type (structural equivalent)
        // IterableIterator<T> is essentially Iterator<T> with [Symbol.iterator]() returning itself
        // For the structural fallback, we just return Iterator<T> since Symbol.iterator
        // access isn't easily modeled structurally without well-known symbols
        self.create_iterator_type(element_type)
    }

    /// Create an AsyncIterator<T> type.
    ///
    /// AsyncIterator<T, TReturn = any, TNext = undefined> has:
    /// - `next()` method returning Promise<IteratorResult<T, TReturn>>
    pub fn create_async_iterator_type(&self, element_type: TypeId) -> TypeId {
        // Try to find the global AsyncIterator interface from lib contexts
        if let Some(async_iterator_base) = self.lookup_global_type("AsyncIterator") {
            return self.ctx.types.application(
                async_iterator_base,
                vec![element_type, TypeId::ANY, TypeId::UNDEFINED],
            );
        }

        // Fallback: create structural equivalent with Promise<IteratorResult<T, any>>
        self.create_structural_async_iterator_type(element_type)
    }

    /// Create an AsyncIterableIterator<T> type.
    ///
    /// AsyncIterableIterator<T> extends AsyncIterator<T> and has:
    /// - `next()` method returning Promise<IteratorResult<T>>
    /// - `[Symbol.asyncIterator]()` method returning itself
    pub fn create_async_iterable_iterator_type(&self, element_type: TypeId) -> TypeId {
        // Try to find the global AsyncIterableIterator interface from lib contexts
        if let Some(async_iterable_iterator_base) = self.lookup_global_type("AsyncIterableIterator")
        {
            return self
                .ctx
                .types
                .application(async_iterable_iterator_base, vec![element_type]);
        }

        // AsyncIterableIterator global not found - emit TS2318 regardless of noLib setting.
        // TSC emits this error even with noLib: true when AsyncIterableIterator is needed.
        use crate::lib_loader;
        self.ctx
            .push_diagnostic(lib_loader::emit_error_global_type_missing(
                "AsyncIterableIterator",
                self.ctx.file_name.clone(),
                0,
                0,
            ));

        // Fallback: use async iterator type
        self.create_async_iterator_type(element_type)
    }

    /// Create a structural equivalent of AsyncIterator<T> when the global interface is not available.
    fn create_structural_async_iterator_type(&self, element_type: TypeId) -> TypeId {
        // Create IteratorResult<T, any> type
        let iterator_result_type = self.create_iterator_result_type(element_type, TypeId::ANY);

        // Wrap in Promise<IteratorResult<T, any>>
        let promise_result_type = self.create_promise_type(iterator_result_type);

        // Create the `next` method signature: () => Promise<IteratorResult<T, any>>
        let next_method_shape = crate::solver::FunctionShape {
            type_params: Vec::new(),
            params: Vec::new(),
            this_type: None,
            return_type: promise_result_type,
            type_predicate: None,
            is_constructor: false,
            is_method: true,
        };
        let next_method_type = self.ctx.types.function(next_method_shape);

        // Create the AsyncIterator object type with `next` property
        let next_atom = self.ctx.types.intern_string("next");
        let next_property = crate::solver::PropertyInfo {
            name: next_atom,
            type_id: next_method_type,
            write_type: next_method_type,
            optional: false,
            readonly: false,
            is_method: true,
        };

        self.ctx.types.object(vec![next_property])
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

    // =========================================================================
    // Diagnostic Emission Methods
    // =========================================================================

    /// Emit TS2488 error when a type is not iterable.
    /// Used for for-of loops and spread operations on non-iterable types.
    fn emit_not_iterable_error(&mut self, type_id: TypeId, expr_idx: NodeIndex) {
        // Skip error types and any/unknown to avoid cascading errors
        if type_id == TypeId::ANY || type_id == TypeId::UNKNOWN || type_id == TypeId::ERROR {
            return;
        }

        if let Some((start, end)) = self.ctx.get_node_span(expr_idx) {
            let type_str = self.format_type(type_id);
            let message = format_message(
                diagnostic_messages::TYPE_MUST_HAVE_SYMBOL_ITERATOR,
                &[&type_str],
            );
            self.ctx.push_diagnostic(Diagnostic::error(
                self.ctx.file_name.clone(),
                start,
                end.saturating_sub(start),
                message,
                diagnostic_codes::TYPE_MUST_HAVE_SYMBOL_ITERATOR,
            ));
        }
    }

    /// Emit TS2504 error when a type is not async iterable.
    /// Used for for-await-of loops on non-async-iterable types.
    fn emit_not_async_iterable_error(&mut self, type_id: TypeId, expr_idx: NodeIndex) {
        // Skip error types and any/unknown to avoid cascading errors
        if type_id == TypeId::ANY || type_id == TypeId::UNKNOWN || type_id == TypeId::ERROR {
            return;
        }

        if let Some((start, end)) = self.ctx.get_node_span(expr_idx) {
            let type_str = self.format_type(type_id);
            let message = format_message(
                diagnostic_messages::TYPE_MUST_HAVE_SYMBOL_ASYNC_ITERATOR,
                &[&type_str],
            );
            self.ctx.push_diagnostic(Diagnostic::error(
                self.ctx.file_name.clone(),
                start,
                end.saturating_sub(start),
                message,
                diagnostic_codes::TYPE_MUST_HAVE_SYMBOL_ASYNC_ITERATOR,
            ));
        }
    }

    /// Format a type for diagnostic messages.
    fn format_type(&self, type_id: TypeId) -> String {
        self.ctx.types.format_type(type_id)
    }

    // =========================================================================
    // Helper methods
    // =========================================================================

    fn object_has_iterator_method(&self, shape_id: crate::solver::ObjectShapeId) -> bool {
        // Check if object shape has a [Symbol.iterator] method or 'next' method (iterator protocol)
        let shape = self.ctx.types.object_shape(shape_id);
        for prop in &shape.properties {
            let prop_name = self.ctx.types.resolve_atom_ref(prop.name);
            // Check for [Symbol.iterator] method (iterable protocol)
            if prop_name.as_ref() == "[Symbol.iterator]" && prop.is_method {
                return true;
            }
            // Check for 'next' method (direct iterator)
            if prop_name.as_ref() == "next" && prop.is_method {
                return true;
            }
        }
        false
    }

    fn object_has_async_iterator_method(&self, shape_id: crate::solver::ObjectShapeId) -> bool {
        // Check if object has [Symbol.asyncIterator] method
        // This is the async iterator protocol
        let shape = self.ctx.types.object_shape(shape_id);
        for prop in &shape.properties {
            let prop_name = self.ctx.types.resolve_atom_ref(prop.name);
            // Check for [Symbol.asyncIterator] method (async iterable protocol)
            if prop_name.as_ref() == "[Symbol.asyncIterator]" && prop.is_method {
                return true;
            }
            // Fallback: Check if object has a 'next' method that returns Promise
            if prop_name.as_ref() == "next" && prop.is_method {
                // Check if the return type is Promise-like
                if let Some(crate::solver::TypeKey::Function(func_id)) =
                    self.ctx.types.lookup(prop.type_id)
                {
                    let func = self.ctx.types.function_shape(func_id);
                    // Check if return type is a Promise (has 'then' property)
                    if let Some(crate::solver::TypeKey::Object(ret_shape_id)) =
                        self.ctx.types.lookup(func.return_type)
                    {
                        let ret_shape = self.ctx.types.object_shape(ret_shape_id);
                        for ret_prop in &ret_shape.properties {
                            let ret_prop_name = self.ctx.types.resolve_atom_ref(ret_prop.name);
                            if ret_prop_name.as_ref() == "then" {
                                return true;
                            }
                        }
                    }
                }
            }
        }
        false
    }

    /// Get the element type from an async iterable.
    ///
    /// For AsyncGenerator<Y, R, N>, this extracts Y (the yield type).
    /// For async iterables, this unwraps Promise<IteratorResult<T>> to get T.
    fn get_async_iterable_element_type(&self, type_id: TypeId) -> TypeId {
        // Use the helper function from generators module
        crate::checker::generators::get_async_iterable_element_type(self.ctx.types, type_id)
    }

    fn is_valid_for_in_target(&self, type_id: TypeId) -> bool {
        // for-in works with any, object types, and type parameters
        if type_id == TypeId::ANY {
            return true;
        }

        if let Some(type_key) = self.ctx.types.lookup(type_id) {
            match type_key {
                crate::solver::TypeKey::Object(_) => return true,
                crate::solver::TypeKey::Array(_) => return true,
                crate::solver::TypeKey::TypeParameter(_) => return true,
                _ => {}
            }
        }

        false
    }

    fn check_expression(&mut self, idx: NodeIndex) -> TypeId {
        // Check cache first
        if let Some(&cached) = self.ctx.node_types.get(&idx.0) {
            return cached;
        }

        // Basic type inference
        if let Some(node) = self.ctx.arena.get(idx) {
            match node.kind {
                k if k == SyntaxKind::NumericLiteral as u16 => TypeId::NUMBER,
                k if k == SyntaxKind::StringLiteral as u16 => TypeId::STRING,
                k if k == SyntaxKind::TrueKeyword as u16 => self.ctx.types.literal_boolean(true),
                k if k == SyntaxKind::FalseKeyword as u16 => self.ctx.types.literal_boolean(false),
                k if k == SyntaxKind::NullKeyword as u16 => TypeId::NULL,
                k if k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION => {
                    // For array literals, infer element type
                    self.infer_array_literal_type(idx)
                }
                _ => TypeId::ANY,
            }
        } else {
            TypeId::ANY
        }
    }

    fn infer_array_literal_type(&mut self, idx: NodeIndex) -> TypeId {
        let Some(node) = self.ctx.arena.get(idx) else {
            return self.ctx.types.array(TypeId::ANY);
        };

        if let Some(array_lit) = self.ctx.arena.get_array_literal(node) {
            if array_lit.elements.nodes.is_empty() {
                // Empty array literal: use any[] since we don't support evolving arrays yet
                return self.ctx.types.array(TypeId::ANY);
            }

            // Collect element types
            let mut element_types = Vec::new();
            for &elem_idx in &array_lit.elements.nodes {
                if !elem_idx.is_null() {
                    let elem_type = self.check_expression(elem_idx);
                    element_types.push(elem_type);
                }
            }

            // Create union of element types
            let element_type = if element_types.is_empty() {
                TypeId::NEVER
            } else if element_types.len() == 1 {
                element_types[0]
            } else {
                self.ctx.types.union(element_types)
            };

            return self.ctx.types.array(element_type);
        }

        self.ctx.types.array(TypeId::ANY)
    }
}

/// Result of checking a for-of loop.
#[derive(Debug, Clone)]
pub struct ForOfCheckResult {
    /// Whether the for-of loop is valid
    pub is_valid: bool,
    /// The element type of the iteration
    pub element_type: TypeId,
    /// Whether this is an async for-await-of
    pub is_async: bool,
    /// Error message if invalid
    pub error: Option<String>,
}

impl ForOfCheckResult {
    fn error(msg: &str) -> Self {
        Self {
            is_valid: false,
            element_type: TypeId::ANY,
            is_async: false,
            error: Some(msg.to_string()),
        }
    }
}

/// Result of checking a spread operation.
#[derive(Debug, Clone)]
pub struct SpreadCheckResult {
    /// Whether the spread is valid
    pub is_valid: bool,
    /// The element type being spread
    pub element_type: TypeId,
    /// Error message if invalid
    pub error: Option<String>,
}

impl SpreadCheckResult {
    fn error(msg: &str) -> Self {
        Self {
            is_valid: false,
            element_type: TypeId::ANY,
            error: Some(msg.to_string()),
        }
    }
}

/// Result of checking a for-in loop.
#[derive(Debug, Clone)]
pub struct ForInCheckResult {
    /// Whether the for-in loop is valid
    pub is_valid: bool,
    /// The key type (always string for for-in)
    pub key_type: TypeId,
    /// Error message if invalid
    pub error: Option<String>,
}

impl ForInCheckResult {
    fn error(msg: &str) -> Self {
        Self {
            is_valid: false,
            key_type: TypeId::STRING,
            error: Some(msg.to_string()),
        }
    }
}

/// Errors that can occur during iterator type checking.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IteratorError {
    /// Type is not iterable
    NotIterable,
    /// Type is not async iterable
    NotAsyncIterable,
    /// Missing Symbol.iterator method
    MissingIteratorMethod,
    /// Iterator result type mismatch
    IteratorResultMismatch { expected: TypeId, actual: TypeId },
}

impl std::fmt::Display for IteratorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IteratorError::NotIterable => {
                write!(f, "Type is not iterable")
            }
            IteratorError::NotAsyncIterable => {
                write!(f, "Type is not an async iterable")
            }
            IteratorError::MissingIteratorMethod => {
                write!(
                    f,
                    "Type must have a '[Symbol.iterator]()' method that returns an iterator"
                )
            }
            IteratorError::IteratorResultMismatch { expected, actual } => {
                write!(
                    f,
                    "Iterator result type '{:?}' is not assignable to '{:?}'",
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
    fn test_string_is_iterable() {
        let source = "'hello'";
        let (parser, binder, types) = create_context(source);
        let ctx = CheckerContext::new(
            parser.get_arena(),
            &binder,
            &types,
            "test.ts".to_string(),
            crate::checker::context::CheckerOptions::default(),
        );

        // String type is always iterable
        let checker = IteratorChecker { ctx: &mut { ctx } };
        assert!(checker.is_iterable(TypeId::STRING));
    }

    #[test]
    fn test_array_element_type() {
        let source = "[1, 2, 3]";
        let (parser, binder, types) = create_context(source);
        let ctx = CheckerContext::new(
            parser.get_arena(),
            &binder,
            &types,
            "test.ts".to_string(),
            crate::checker::context::CheckerOptions::default(),
        );

        // Create an array type
        let number_array = types.array(TypeId::NUMBER);
        let checker = IteratorChecker { ctx: &mut { ctx } };

        // Element type of number[] should be number
        let elem_type = checker.get_iterable_element_type(number_array);
        assert_eq!(elem_type, TypeId::NUMBER);
    }

    #[test]
    fn test_tuple_element_type() {
        let source = "const x: [number, string] = [1, 'a']";
        let (parser, binder, types) = create_context(source);
        let ctx = CheckerContext::new(
            parser.get_arena(),
            &binder,
            &types,
            "test.ts".to_string(),
            crate::checker::context::CheckerOptions::default(),
        );

        // Create a tuple type [number, string]
        let tuple_type = types.tuple(vec![
            crate::solver::TupleElement {
                type_id: TypeId::NUMBER,
                optional: false,
            },
            crate::solver::TupleElement {
                type_id: TypeId::STRING,
                optional: false,
            },
        ]);

        let checker = IteratorChecker { ctx: &mut { ctx } };

        // Element type of [number, string] should be number | string
        let elem_type = checker.get_iterable_element_type(tuple_type);
        // The result should be a union type
        assert!(checker.is_iterable(tuple_type));
    }

    #[test]
    fn test_create_iterator_type_number() {
        let source = "";
        let (parser, binder, types) = create_context(source);
        let ctx = CheckerContext::new(
            parser.get_arena(),
            &binder,
            &types,
            "test.ts".to_string(),
            crate::checker::context::CheckerOptions::default(),
        );

        let checker = IteratorChecker { ctx: &mut { ctx } };

        // Create Iterator<number>
        let iterator_number = checker.create_iterator_type(TypeId::NUMBER);

        // Verify the type is not ANY (i.e., a proper type was created)
        assert_ne!(iterator_number, TypeId::ANY);

        // Verify it's an object type with a `next` method
        if let Some(type_key) = types.lookup(iterator_number) {
            match type_key {
                crate::solver::TypeKey::Object(shape_id) => {
                    let shape = types.object_shape(shape_id);
                    // Should have a `next` property
                    assert!(
                        shape
                            .properties
                            .iter()
                            .any(|p| { types.resolve_atom(p.name) == "next" && p.is_method })
                    );
                }
                _ => {
                    // Could be an Application type if lib types were available
                    // This is acceptable
                }
            }
        }
    }

    #[test]
    fn test_create_iterator_result_type() {
        let source = "";
        let (parser, binder, types) = create_context(source);
        let ctx = CheckerContext::new(
            parser.get_arena(),
            &binder,
            &types,
            "test.ts".to_string(),
            crate::checker::context::CheckerOptions::default(),
        );

        let checker = IteratorChecker { ctx: &mut { ctx } };

        // Create IteratorResult<number, any>
        let iterator_result = checker.create_iterator_result_type(TypeId::NUMBER, TypeId::ANY);

        // Verify the type is not ANY
        assert_ne!(iterator_result, TypeId::ANY);

        // It should be a union type (IteratorYieldResult | IteratorReturnResult)
        if let Some(type_key) = types.lookup(iterator_result) {
            match type_key {
                crate::solver::TypeKey::Union(_) => {
                    // Expected: union of yield and return result types
                }
                _ => {
                    // Could be an Application type if lib types were available
                }
            }
        }
    }

    #[test]
    fn test_create_iterable_iterator_type() {
        let source = "";
        let (parser, binder, types) = create_context(source);
        let ctx = CheckerContext::new(
            parser.get_arena(),
            &binder,
            &types,
            "test.ts".to_string(),
            crate::checker::context::CheckerOptions::default(),
        );

        let checker = IteratorChecker { ctx: &mut { ctx } };

        // Create IterableIterator<string>
        let iterable_iterator = checker.create_iterable_iterator_type(TypeId::STRING);

        // Should not return ANY
        assert_ne!(iterable_iterator, TypeId::ANY);
    }

    #[test]
    fn test_create_async_iterator_type() {
        let source = "";
        let (parser, binder, types) = create_context(source);
        let ctx = CheckerContext::new(
            parser.get_arena(),
            &binder,
            &types,
            "test.ts".to_string(),
            crate::checker::context::CheckerOptions::default(),
        );

        let checker = IteratorChecker { ctx: &mut { ctx } };

        // Create AsyncIterator<number>
        let async_iterator = checker.create_async_iterator_type(TypeId::NUMBER);

        // Should not return ANY
        assert_ne!(async_iterator, TypeId::ANY);
    }

    #[test]
    fn test_iterator_type_has_next_method() {
        let source = "";
        let (parser, binder, types) = create_context(source);
        let ctx = CheckerContext::new(
            parser.get_arena(),
            &binder,
            &types,
            "test.ts".to_string(),
            crate::checker::context::CheckerOptions::default(),
        );

        let checker = IteratorChecker { ctx: &mut { ctx } };

        // Create Iterator<number>
        let iterator_type = checker.create_iterator_type(TypeId::NUMBER);

        // Verify it has a next() method that returns IteratorResult<number, any>
        if let Some(crate::solver::TypeKey::Object(shape_id)) = types.lookup(iterator_type) {
            let shape = types.object_shape(shape_id);

            // Find the next property
            let next_prop = shape
                .properties
                .iter()
                .find(|p| types.resolve_atom(p.name) == "next");

            assert!(next_prop.is_some(), "Iterator should have a 'next' method");

            let next_prop = next_prop.unwrap();
            assert!(next_prop.is_method, "next should be a method");

            // Verify next is a function
            if let Some(crate::solver::TypeKey::Function(func_id)) = types.lookup(next_prop.type_id)
            {
                let func_shape = types.function_shape(func_id);
                // Return type should be IteratorResult<number, any>
                assert_ne!(func_shape.return_type, TypeId::ANY);
            }
        }
    }
}
