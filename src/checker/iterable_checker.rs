//! Iterable/Iterator Type Checking Module
//!
//! This module contains iterable and iterator type checking methods for CheckerState
//! as part of Phase 2 architecture refactoring.
//!
//! The methods in this module handle:
//! - Checking if a type is iterable (has Symbol.iterator protocol)
//! - Checking if a type is async iterable (has Symbol.asyncIterator protocol)
//! - Computing element types for for-of loops
//! - Emitting appropriate errors for non-iterable types
//!
//! This module extends CheckerState with methods for iterable/iterator protocol
//! checking, providing cleaner APIs for iteration-related type operations.

use crate::checker::state::CheckerState;
use crate::checker::types::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
use crate::parser::NodeIndex;
use crate::solver::TypeId;
use crate::solver::type_queries::{
    AsyncIterableTypeKind, ForOfElementKind, FullIterableTypeKind, classify_async_iterable_type,
    classify_for_of_element_type, classify_full_iterable_type,
};

// =============================================================================
// Iterable Type Checking Methods
// =============================================================================

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Iterable Protocol Checking
    // =========================================================================

    /// Check if a type is iterable (has Symbol.iterator protocol).
    ///
    /// A type is iterable if it is:
    /// - String type
    /// - Array type
    /// - Tuple type
    /// - Has a [Symbol.iterator] method
    /// - A union where all members are iterable
    /// - An intersection where at least one member is iterable
    pub fn is_iterable_type(&self, type_id: TypeId) -> bool {
        // Intrinsic types that are always iterable or not iterable
        if type_id == TypeId::ANY || type_id == TypeId::UNKNOWN || type_id == TypeId::ERROR {
            return true; // Don't report errors on any/unknown/error
        }
        if type_id == TypeId::STRING {
            return true;
        }
        if type_id == TypeId::NUMBER
            || type_id == TypeId::BOOLEAN
            || type_id == TypeId::VOID
            || type_id == TypeId::NULL
            || type_id == TypeId::UNDEFINED
            || type_id == TypeId::NEVER
            || type_id == TypeId::SYMBOL
            || type_id == TypeId::BIGINT
        {
            return false;
        }

        self.is_iterable_type_classified(type_id)
    }

    /// Internal helper that uses the solver's classification enum to determine iterability.
    fn is_iterable_type_classified(&self, type_id: TypeId) -> bool {
        match classify_full_iterable_type(self.ctx.types, type_id) {
            FullIterableTypeKind::Array(_) => true,
            FullIterableTypeKind::Tuple(_) => true,
            FullIterableTypeKind::StringLiteral(_) => true,
            FullIterableTypeKind::Union(members) => {
                members.iter().all(|&m| self.is_iterable_type(m))
            }
            FullIterableTypeKind::Intersection(members) => {
                // Intersection is iterable if at least one member is iterable
                members.iter().any(|&m| self.is_iterable_type(m))
            }
            FullIterableTypeKind::Object(shape_id) => {
                // Check if object has a [Symbol.iterator] method or 'next' method
                self.object_has_iterator_method(shape_id)
            }
            FullIterableTypeKind::Application { base } => {
                // Check if the base type is iterable
                // This handles Set<T>, Map<K, V>, ReadonlyArray<T>, etc.
                self.is_iterable_type(base)
            }
            FullIterableTypeKind::TypeParameter { constraint } => {
                if let Some(c) = constraint {
                    self.is_iterable_type(c)
                } else {
                    // Unconstrained type parameters (extends unknown/any) should not error
                    // TypeScript does NOT emit TS2488 for unconstrained type parameters
                    false
                }
            }
            FullIterableTypeKind::Readonly(inner) => {
                // Unwrap readonly wrapper and check inner type
                self.is_iterable_type(inner)
            }
            // Index access, Conditional, Mapped - not directly iterable
            FullIterableTypeKind::ComplexType => false,
            // Functions, classes without Symbol.iterator are not iterable
            FullIterableTypeKind::FunctionOrCallable => false,
            // Unknown type - not iterable
            FullIterableTypeKind::NotIterable => false,
        }
    }

    /// Check if an object shape has a Symbol.iterator method.
    ///
    /// An object is iterable if it has a [Symbol.iterator]() method that returns an iterator.
    /// An iterator (with just a next() method) is NOT automatically iterable.
    fn object_has_iterator_method(&self, shape_id: crate::solver::ObjectShapeId) -> bool {
        let shape = self.ctx.types.object_shape(shape_id);

        // Check for [Symbol.iterator] method (iterable protocol)
        for prop in &shape.properties {
            let prop_name = self.ctx.types.resolve_atom_ref(prop.name);
            if prop_name.as_ref() == "[Symbol.iterator]" && prop.is_method {
                return true;
            }
        }

        // TODO: Check call signatures for generators when CallableShape is implemented

        false
    }

    /// Check if a type is async iterable (has Symbol.asyncIterator protocol).
    pub fn is_async_iterable_type(&self, type_id: TypeId) -> bool {
        // Intrinsic types that are always iterable or not iterable
        if type_id == TypeId::ANY || type_id == TypeId::UNKNOWN || type_id == TypeId::ERROR {
            return true; // Don't report errors on any/unknown/error
        }

        self.is_async_iterable_type_classified(type_id)
    }

    /// Internal helper that uses the solver's classification enum to determine async iterability.
    fn is_async_iterable_type_classified(&self, type_id: TypeId) -> bool {
        match classify_async_iterable_type(self.ctx.types, type_id) {
            AsyncIterableTypeKind::Union(members) => {
                members.iter().all(|&m| self.is_async_iterable_type(m))
            }
            AsyncIterableTypeKind::Object(shape_id) => {
                // Check if object has a [Symbol.asyncIterator] method
                let shape = self.ctx.types.object_shape(shape_id);
                for prop in &shape.properties {
                    let prop_name = self.ctx.types.resolve_atom_ref(prop.name);
                    if prop_name.as_ref() == "[Symbol.asyncIterator]" && prop.is_method {
                        return true;
                    }
                }
                false
            }
            AsyncIterableTypeKind::Readonly(inner) => {
                // Unwrap readonly wrapper and check inner type
                self.is_async_iterable_type(inner)
            }
            AsyncIterableTypeKind::NotAsyncIterable => false,
        }
    }

    // =========================================================================
    // For-Of Element Type Computation
    // =========================================================================

    /// Compute the element type produced by a `for (... of expr)` loop.
    ///
    /// This is a best-effort implementation for common cases (arrays/tuples/unions).
    pub fn for_of_element_type(&mut self, iterable_type: TypeId) -> TypeId {
        if iterable_type == TypeId::ANY
            || iterable_type == TypeId::UNKNOWN
            || iterable_type == TypeId::ERROR
        {
            return iterable_type;
        }

        self.for_of_element_type_classified(iterable_type, 0)
    }

    /// Internal helper that uses the solver's classification enum to compute element type.
    /// The depth parameter prevents infinite loops from circular readonly types.
    fn for_of_element_type_classified(&mut self, type_id: TypeId, depth: usize) -> TypeId {
        if depth > 100 {
            return TypeId::ANY;
        }

        match classify_for_of_element_type(self.ctx.types, type_id) {
            ForOfElementKind::Array(elem) => elem,
            ForOfElementKind::Tuple(elements) => {
                let mut member_types: Vec<TypeId> = elements.iter().map(|e| e.type_id).collect();
                if member_types.is_empty() {
                    TypeId::NEVER
                } else if member_types.len() == 1 {
                    member_types.pop().unwrap_or(TypeId::ANY)
                } else {
                    self.ctx.types.union(member_types)
                }
            }
            ForOfElementKind::Union(members) => {
                let mut element_types = Vec::with_capacity(members.len());
                for member in members {
                    element_types.push(self.for_of_element_type_classified(member, depth + 1));
                }
                self.ctx.types.union(element_types)
            }
            ForOfElementKind::Readonly(inner) => {
                // Unwrap readonly wrapper and compute element type for inner
                self.for_of_element_type_classified(inner, depth + 1)
            }
            ForOfElementKind::Other => TypeId::ANY,
        }
    }

    // =========================================================================
    // For-Of Iterability Checking with Error Reporting
    // =========================================================================

    /// Check iterability of a for-of expression and emit TS2488/TS2504 if not iterable.
    ///
    /// Returns `true` if the type is iterable (or async iterable for for-await-of).
    pub fn check_for_of_iterability(
        &mut self,
        expr_type: TypeId,
        expr_idx: NodeIndex,
        is_async: bool,
    ) -> bool {
        // Skip error/any/unknown types to prevent false positives
        if expr_type == TypeId::ANY || expr_type == TypeId::UNKNOWN || expr_type == TypeId::ERROR {
            return true;
        }

        // For async for-of, first check async iterable, then fall back to sync iterable
        if is_async {
            if self.is_async_iterable_type(expr_type) || self.is_iterable_type(expr_type) {
                return true;
            }
            // Not async iterable - emit TS2504
            if let Some((start, end)) = self.get_node_span(expr_idx) {
                let type_str = self.format_type(expr_type);
                let message = format_message(
                    diagnostic_messages::TYPE_MUST_HAVE_SYMBOL_ASYNC_ITERATOR,
                    &[&type_str],
                );
                self.error(
                    start,
                    end.saturating_sub(start),
                    message,
                    diagnostic_codes::TYPE_MUST_HAVE_SYMBOL_ASYNC_ITERATOR,
                );
            }
            return false;
        }

        // Regular for-of - check sync iterability
        if self.is_iterable_type(expr_type) {
            return true;
        }

        // Not iterable - emit TS2488
        if let Some((start, end)) = self.get_node_span(expr_idx) {
            let type_str = self.format_type(expr_type);
            let message = format_message(
                diagnostic_messages::TYPE_MUST_HAVE_SYMBOL_ITERATOR,
                &[&type_str],
            );
            self.error(
                start,
                end.saturating_sub(start),
                message,
                diagnostic_codes::TYPE_MUST_HAVE_SYMBOL_ITERATOR,
            );
        }
        false
    }

    /// Check iterability of a spread argument and emit TS2488 if not iterable.
    ///
    /// Used for spread in array literals and function call arguments.
    /// Returns `true` if the type is iterable.
    pub fn check_spread_iterability(&mut self, spread_type: TypeId, expr_idx: NodeIndex) -> bool {
        // Skip error types and any/unknown
        if spread_type == TypeId::ANY
            || spread_type == TypeId::UNKNOWN
            || spread_type == TypeId::ERROR
        {
            return true;
        }

        if self.is_iterable_type(spread_type) {
            return true;
        }

        // Not iterable - emit TS2488
        if let Some((start, end)) = self.get_node_span(expr_idx) {
            let type_str = self.format_type(spread_type);
            let message = format_message(
                diagnostic_messages::TYPE_MUST_HAVE_SYMBOL_ITERATOR,
                &[&type_str],
            );
            self.error(
                start,
                end.saturating_sub(start),
                message,
                diagnostic_codes::TYPE_MUST_HAVE_SYMBOL_ITERATOR,
            );
        }
        false
    }

    /// Check iterability for array destructuring patterns and emit TS2488 if not iterable.
    ///
    /// This function is called before assigning types to binding elements in array
    /// destructuring to ensure that the source type is iterable.
    ///
    /// ## Parameters:
    /// - `pattern_idx`: The array binding pattern node index
    /// - `pattern_type`: The type being destructured
    /// - `init_expr`: The initializer expression (used for error location)
    ///
    /// ## Validation:
    /// - Checks if pattern_type is iterable
    /// - Emits TS2488 if the type is not iterable
    /// - Skips check for ANY, UNKNOWN, ERROR types (defer to other checks)
    pub fn check_destructuring_iterability(
        &mut self,
        pattern_idx: NodeIndex,
        pattern_type: TypeId,
        init_expr: NodeIndex,
    ) -> bool {
        // Skip check for types that defer to other validation
        if pattern_type == TypeId::ANY
            || pattern_type == TypeId::UNKNOWN
            || pattern_type == TypeId::ERROR
        {
            return true;
        }

        // Check if the type is iterable
        if self.is_iterable_type(pattern_type) {
            return true;
        }

        // Not iterable - emit TS2488
        // Use the initializer expression for error location if available
        let error_idx = if init_expr.is_some() {
            init_expr
        } else {
            pattern_idx
        };

        if let Some((start, end)) = self.get_node_span(error_idx) {
            let type_str = self.format_type(pattern_type);
            let message = format_message(
                diagnostic_messages::TYPE_MUST_HAVE_SYMBOL_ITERATOR,
                &[&type_str],
            );
            self.error(
                start,
                end.saturating_sub(start),
                message,
                diagnostic_codes::TYPE_MUST_HAVE_SYMBOL_ITERATOR,
            );
        }
        false
    }
}
