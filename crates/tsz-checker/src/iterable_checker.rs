//! Iterable/Iterator Type Checking Module
//!
//! This module contains iterable and iterator type checking methods for `CheckerState`
//! as part of Phase 2 architecture refactoring.
//!
//! The methods in this module handle:
//! - Checking if a type is iterable (has Symbol.iterator protocol)
//! - Checking if a type is async iterable (has Symbol.asyncIterator protocol)
//! - Computing element types for for-of loops
//! - Emitting appropriate errors for non-iterable types
//!
//! This module extends `CheckerState` with methods for iterable/iterator protocol
//! checking, providing cleaner APIs for iteration-related type operations.

use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
use crate::query_boundaries::iterable_checker::{
    AsyncIterableTypeKind, ForOfElementKind, FullIterableTypeKind, call_signatures_for_type,
    classify_async_iterable_type, classify_for_of_element_type, classify_full_iterable_type,
    function_shape_for_type, is_array_type, is_string_literal_type, is_string_type, is_tuple_type,
    union_members_for_type,
};
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

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
    pub fn is_iterable_type(&mut self, type_id: TypeId) -> bool {
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
    fn is_iterable_type_classified(&mut self, type_id: TypeId) -> bool {
        let kind = classify_full_iterable_type(self.ctx.types, type_id);
        match kind {
            FullIterableTypeKind::Array(_)
            | FullIterableTypeKind::Tuple(_)
            | FullIterableTypeKind::StringLiteral(_) => true,
            FullIterableTypeKind::Union(members) => {
                members.iter().all(|&m| self.is_iterable_type(m))
            }
            FullIterableTypeKind::Intersection(members) => {
                // Intersection is iterable if at least one member is iterable
                members.iter().any(|&m| self.is_iterable_type(m))
            }
            FullIterableTypeKind::Object(shape_id) => {
                // Check if object has a [Symbol.iterator] method
                // Fall back to property access resolution for computed properties
                // (e.g., `[Symbol.iterator]: any` may not be stored as a method in the shape)
                self.object_has_iterator_method(shape_id)
                    || self.type_has_symbol_iterator_via_property_access(type_id)
            }
            FullIterableTypeKind::Application { .. } => {
                // Application types (Set<T>, Map<K,V>, Iterable<T>, etc.) may have
                // Lazy(DefId) bases that can't be resolved through the type classification.
                // Use the full property access resolution which handles all the complex
                // resolution paths including Application types with Lazy bases from lib files.
                self.type_has_symbol_iterator_via_property_access(type_id)
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
            FullIterableTypeKind::FunctionOrCallable => {
                // Callable types can have properties (including [Symbol.iterator])
                self.type_has_symbol_iterator_via_property_access(type_id)
            }
            // Lazy(DefId) from lib files - use property access to resolve
            FullIterableTypeKind::NotIterable => {
                self.type_has_symbol_iterator_via_property_access(type_id)
            }
        }
    }

    /// Check if an object shape has a Symbol.iterator method.
    ///
    /// An object is iterable if it has a [Symbol.iterator]() method that returns an iterator.
    /// An iterator (with just a `next()` method) is NOT automatically iterable.
    fn object_has_iterator_method(&self, shape_id: tsz_solver::ObjectShapeId) -> bool {
        let shape = self.ctx.types.object_shape(shape_id);

        // Check for [Symbol.iterator] method (iterable protocol)
        for prop in &shape.properties {
            let prop_name = self.ctx.types.resolve_atom_ref(prop.name);
            if prop_name.as_ref() == "[Symbol.iterator]" {
                if prop.is_method {
                    return true;
                }
                // Non-method properties typed as `any` are callable, so treat them as valid.
                // e.g., `class Foo { [Symbol.iterator]: any; }`
                if prop.type_id == TypeId::ANY {
                    return true;
                }
            }
        }

        false
    }

    /// Check if a type has [Symbol.iterator] using the full property access resolution.
    /// This handles Application types (Set<T>, Map<K,V>) with Lazy(DefId) bases from lib
    /// files, Callable types with iterator properties, and other complex cases where simple
    /// shape inspection fails but the full checker resolution machinery can find the property.
    fn type_has_symbol_iterator_via_property_access(&mut self, type_id: TypeId) -> bool {
        use tsz_solver::operations_property::PropertyAccessResult;
        let result = self.resolve_property_access_with_env(type_id, "[Symbol.iterator]");
        matches!(result, PropertyAccessResult::Success { .. })
    }

    /// Check if a type has a numeric index signature, making it "array-like".
    /// TypeScript allows array destructuring of array-like types without [Symbol.iterator]().
    pub(crate) fn has_numeric_index_signature(&mut self, type_id: TypeId) -> bool {
        // Resolve lazy types first
        let type_id = self.resolve_lazy_type(type_id);
        match classify_full_iterable_type(self.ctx.types, type_id) {
            FullIterableTypeKind::Object(shape_id) => {
                let shape = self.ctx.types.object_shape(shape_id);
                shape.number_index.is_some()
            }
            FullIterableTypeKind::Application { base } => self.has_numeric_index_signature(base),
            FullIterableTypeKind::Readonly(inner) => self.has_numeric_index_signature(inner),
            FullIterableTypeKind::Union(members) => members
                .iter()
                .all(|&m| self.is_iterable_type(m) || self.has_numeric_index_signature(m)),
            _ => false,
        }
    }

    /// Check if a type is async iterable (has Symbol.asyncIterator protocol).
    pub fn is_async_iterable_type(&mut self, type_id: TypeId) -> bool {
        // Intrinsic types that are always iterable or not iterable
        if type_id == TypeId::ANY || type_id == TypeId::UNKNOWN || type_id == TypeId::ERROR {
            return true; // Don't report errors on any/unknown/error
        }

        // Resolve lazy types before checking
        let type_id = self.resolve_lazy_type(type_id);

        self.is_async_iterable_type_classified(type_id)
    }

    /// Internal helper that uses the solver's classification enum to determine async iterability.
    fn is_async_iterable_type_classified(&mut self, type_id: TypeId) -> bool {
        match classify_async_iterable_type(self.ctx.types, type_id) {
            AsyncIterableTypeKind::Union(members) => {
                members.iter().all(|&m| self.is_async_iterable_type(m))
            }
            AsyncIterableTypeKind::Object(shape_id) => {
                // Check if object has a [Symbol.asyncIterator] method
                let shape = self.ctx.types.object_shape(shape_id);
                for prop in &shape.properties {
                    let prop_name = self.ctx.types.resolve_atom_ref(prop.name);
                    if prop_name.as_ref() == "[Symbol.asyncIterator]"
                        && prop.is_method
                        && self.is_callable_with_no_required_args(prop.type_id)
                    {
                        return true;
                    }
                }
                false
            }
            AsyncIterableTypeKind::Readonly(inner) => {
                // Unwrap readonly wrapper and check inner type
                self.is_async_iterable_type(inner)
            }
            AsyncIterableTypeKind::NotAsyncIterable => {
                // Use property access to check for [Symbol.asyncIterator] on types
                // that couldn't be classified (e.g., Application types with Lazy bases).
                use tsz_solver::operations_property::PropertyAccessResult;
                let result =
                    self.resolve_property_access_with_env(type_id, "[Symbol.asyncIterator]");
                match result {
                    PropertyAccessResult::Success { type_id, .. } => {
                        self.is_callable_with_no_required_args(type_id)
                    }
                    _ => false,
                }
            }
        }
    }

    /// Returns true when a callable type can be invoked with zero arguments.
    ///
    /// The async iterable protocol requires `[Symbol.asyncIterator]()` to be callable
    /// without arguments. A required parameter (e.g. `(x: number) => ...`) is invalid.
    fn is_callable_with_no_required_args(&self, callable_type: TypeId) -> bool {
        if callable_type == TypeId::ANY
            || callable_type == TypeId::UNKNOWN
            || callable_type == TypeId::ERROR
        {
            return true;
        }

        if let Some(sig) = function_shape_for_type(self.ctx.types, callable_type) {
            return sig.params.iter().all(|p| p.optional || p.rest);
        }

        if let Some(call_signatures) = call_signatures_for_type(self.ctx.types, callable_type) {
            return call_signatures
                .iter()
                .any(|sig| sig.params.iter().all(|p| p.optional || p.rest));
        }

        false
    }

    // =========================================================================
    // For-Of Element Type Computation
    // =========================================================================

    /// Compute the element type produced by a `for (... of expr)` loop.
    ///
    /// Handles arrays, tuples, unions, strings, and custom iterators via
    /// the `[Symbol.iterator]().next().value` protocol.
    pub fn for_of_element_type(&mut self, iterable_type: TypeId) -> TypeId {
        if iterable_type == TypeId::ANY
            || iterable_type == TypeId::UNKNOWN
            || iterable_type == TypeId::ERROR
        {
            return iterable_type;
        }

        // String iteration yields string
        if iterable_type == TypeId::STRING {
            return TypeId::STRING;
        }

        // Resolve lazy types (type aliases) before computing element type
        let iterable_type = self.resolve_lazy_type(iterable_type);

        self.for_of_element_type_classified(iterable_type, 0)
    }

    /// Internal helper that uses the solver's classification enum to compute element type.
    /// The depth parameter prevents infinite loops from circular readonly types.
    fn for_of_element_type_classified(&mut self, type_id: TypeId, depth: usize) -> TypeId {
        let factory = self.ctx.types.factory();
        if depth > 100 {
            return TypeId::ANY;
        }

        // Handle string types (including string literals)
        if type_id == TypeId::STRING {
            return TypeId::STRING;
        }

        match classify_for_of_element_type(self.ctx.types, type_id) {
            ForOfElementKind::Array(elem) => elem,
            ForOfElementKind::Tuple(elements) => {
                let member_types: Vec<TypeId> = elements.iter().map(|e| e.type_id).collect();
                tsz_solver::utils::union_or_single(self.ctx.types, member_types)
            }
            ForOfElementKind::Union(members) => {
                let mut element_types = Vec::with_capacity(members.len());
                for member in members {
                    element_types.push(self.for_of_element_type_classified(member, depth + 1));
                }
                factory.union(element_types)
            }
            ForOfElementKind::Readonly(inner) => {
                // Unwrap readonly wrapper and compute element type for inner
                self.for_of_element_type_classified(inner, depth + 1)
            }
            ForOfElementKind::String => TypeId::STRING,
            ForOfElementKind::Other => {
                // For custom iterators, Application types (Map, Set), etc.,
                // try to resolve the element type via the iterator protocol:
                // type_id[Symbol.iterator]().next().value
                self.resolve_iterator_element_type(type_id)
            }
        }
    }

    /// Resolve the element type of an iterable via the iterator protocol.
    ///
    /// Follows the chain: type[Symbol.iterator] → call result → .`next()` → .value
    /// Returns ANY as fallback if the protocol cannot be resolved.
    fn resolve_iterator_element_type(&mut self, type_id: TypeId) -> TypeId {
        use tsz_solver::operations_property::PropertyAccessResult;

        // Step 1: Get [Symbol.iterator] property
        let iterator_fn = self.resolve_property_access_with_env(type_id, "[Symbol.iterator]");
        let iterator_fn_type = match &iterator_fn {
            PropertyAccessResult::Success { type_id, .. } => *type_id,
            _ => return TypeId::ANY,
        };

        // Step 2: Get the return type of the iterator function (call it)
        let iterator_type = self.get_call_return_type(iterator_fn_type);

        // If the iterator function returns `any` (e.g., `[Symbol.iterator]() { return this; }`
        // where `this` type inference fails), fall back to using the original object type.
        // This is the common pattern where the object IS the iterator.
        let iterator_type = if iterator_type == TypeId::ANY {
            type_id
        } else {
            iterator_type
        };

        // Step 3: Get .next() on the iterator
        let next_result = self.resolve_property_access_with_env(iterator_type, "next");
        let next_fn_type = match &next_result {
            PropertyAccessResult::Success { type_id, .. } => *type_id,
            _ => return TypeId::ANY,
        };

        // Step 4: Get the return type of next()
        let next_return = self.get_call_return_type(next_fn_type);

        // Step 5: Get .value from the IteratorResult
        let value_result = self.resolve_property_access_with_env(next_return, "value");
        match &value_result {
            PropertyAccessResult::Success { type_id, .. } => *type_id,
            _ => TypeId::ANY,
        }
    }

    /// Get the return type of calling a function type.
    /// Returns ANY if the type is not callable.
    fn get_call_return_type(&self, fn_type: TypeId) -> TypeId {
        if fn_type == TypeId::ANY {
            return TypeId::ANY;
        }
        if let Some(sig) = function_shape_for_type(self.ctx.types, fn_type) {
            return sig.return_type;
        }
        if let Some(call_signatures) = call_signatures_for_type(self.ctx.types, fn_type) {
            return call_signatures
                .first()
                .map_or(TypeId::ANY, |sig| sig.return_type);
        }
        TypeId::ANY
    }

    // =========================================================================
    // For-Of Iterability Checking with Error Reporting
    // =========================================================================

    /// Check iterability of a for-of expression and emit TS2488/TS2495/TS2504 if not iterable.
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

        // Resolve lazy types (type aliases) before checking iterability
        let expr_type = self.resolve_lazy_type(expr_type);

        // Check if the expression is nullish (undefined/null)
        // Emit TS18050 "The value 'undefined'/'null' cannot be used here"
        // when trying to iterate over undefined/null
        if expr_type == TypeId::NULL || expr_type == TypeId::UNDEFINED {
            self.report_nullish_object(expr_idx, expr_type, true);
            return false;
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
                    diagnostic_messages::TYPE_MUST_HAVE_A_SYMBOL_ASYNCITERATOR_METHOD_THAT_RETURNS_AN_ASYNC_ITERATOR,
                    &[&type_str],
                );
                self.error(
                    start,
                    end.saturating_sub(start),
                    message,
                    diagnostic_codes::TYPE_MUST_HAVE_A_SYMBOL_ASYNCITERATOR_METHOD_THAT_RETURNS_AN_ASYNC_ITERATOR,
                );
            }
            return false;
        }

        // In ES5 mode (without downlevelIteration), for-of only works with arrays and strings.
        // - Emit TS2802 if the type has Symbol.iterator (iterable but requires ES2015/downlevelIteration).
        // - Emit TS2461 if the type contains a string constituent but the remaining non-string
        //   type is not array-like (TSC strips strings from union before checking array-likeness).
        // - Emit TS2495 if the type is neither an array nor a string (not iterable at all).
        if self.ctx.compiler_options.target.is_es5() {
            if self.is_array_or_tuple_or_string(expr_type) {
                return true;
            }
            // Mirror TSC's logic: strip string-like members from union types.
            // If there were string members, the "remaining" non-string type still needs to be
            // array-like, and the error message changes from TS2495 → TS2461 (no "or string type"
            // suffix because the string part is already accounted for).
            let has_string_constituent = self.has_string_constituent(expr_type);
            let allows_strings = !has_string_constituent;
            if let Some((start, end)) = self.get_node_span(expr_idx) {
                let type_str = self.format_type(expr_type);
                // Check if the type has Symbol.iterator (iterable but not usable in ES5 for-of
                // without downlevelIteration). These emit TS2802 instead of TS2495/TS2461.
                if self.is_iterable_type(expr_type) {
                    let message = format_message(
                        diagnostic_messages::TYPE_CAN_ONLY_BE_ITERATED_THROUGH_WHEN_USING_THE_DOWNLEVELITERATION_FLAG_OR_WITH,
                        &[&type_str],
                    );
                    self.error(
                        start,
                        end.saturating_sub(start),
                        message,
                        diagnostic_codes::TYPE_CAN_ONLY_BE_ITERATED_THROUGH_WHEN_USING_THE_DOWNLEVELITERATION_FLAG_OR_WITH,
                    );
                } else if allows_strings {
                    // No string in union: "Type is not an array type or a string type" (TS2495)
                    let message = format_message(
                        diagnostic_messages::TYPE_IS_NOT_AN_ARRAY_TYPE_OR_A_STRING_TYPE,
                        &[&type_str],
                    );
                    self.error(
                        start,
                        end.saturating_sub(start),
                        message,
                        diagnostic_codes::TYPE_IS_NOT_AN_ARRAY_TYPE_OR_A_STRING_TYPE,
                    );
                } else {
                    // Has string constituent but non-string part is not array-like: TS2461
                    let message = format_message(
                        diagnostic_messages::TYPE_IS_NOT_AN_ARRAY_TYPE,
                        &[&type_str],
                    );
                    self.error(
                        start,
                        end.saturating_sub(start),
                        message,
                        diagnostic_codes::TYPE_IS_NOT_AN_ARRAY_TYPE,
                    );
                }
            }
            return false;
        }

        // Regular for-of (ES2015+) - check sync iterability
        if self.is_iterable_type(expr_type) {
            return true;
        }

        // Not iterable - emit TS2488

        if let Some((start, end)) = self.get_node_span(expr_idx) {
            let type_str = self.format_type(expr_type);
            let message = format_message(
                diagnostic_messages::TYPE_MUST_HAVE_A_SYMBOL_ITERATOR_METHOD_THAT_RETURNS_AN_ITERATOR,
                &[&type_str],
            );
            self.error(
                start,
                end.saturating_sub(start),
                message,
                diagnostic_codes::TYPE_MUST_HAVE_A_SYMBOL_ITERATOR_METHOD_THAT_RETURNS_AN_ITERATOR,
            );
        }
        false
    }

    /// Check iterability of a spread argument and emit TS2488 if not iterable.
    ///
    /// Used for spread in array literals and function call arguments.
    /// Returns `true` if the type is iterable.
    pub fn check_spread_iterability(&mut self, spread_type: TypeId, expr_idx: NodeIndex) -> bool {
        // In ES5 without downlevel iteration, spread requires an array/tuple source.
        // Match tsc by emitting TS2461 for non-array spread arguments.
        if self.ctx.compiler_options.target.is_es5() {
            if spread_type == TypeId::ANY || spread_type == TypeId::UNKNOWN {
                return true;
            }

            let resolved = self.resolve_lazy_type(spread_type);
            if self.is_array_or_tuple_type(resolved) || self.has_numeric_index_signature(resolved) {
                return true;
            }

            if let Some((start, end)) = self.get_node_span(expr_idx) {
                let type_str = self.format_type(resolved);
                if self.is_iterable_type(resolved) {
                    let message = format_message(
                        diagnostic_messages::TYPE_CAN_ONLY_BE_ITERATED_THROUGH_WHEN_USING_THE_DOWNLEVELITERATION_FLAG_OR_WITH,
                        &[&type_str],
                    );
                    self.error(
                        start,
                        end.saturating_sub(start),
                        message,
                        diagnostic_codes::TYPE_CAN_ONLY_BE_ITERATED_THROUGH_WHEN_USING_THE_DOWNLEVELITERATION_FLAG_OR_WITH,
                    );
                } else {
                    let message = format_message(
                        diagnostic_messages::TYPE_IS_NOT_AN_ARRAY_TYPE,
                        &[&type_str],
                    );
                    self.error(
                        start,
                        end.saturating_sub(start),
                        message,
                        diagnostic_codes::TYPE_IS_NOT_AN_ARRAY_TYPE,
                    );
                }
            }
            return false;
        }

        // Skip error types and any/unknown
        if spread_type == TypeId::ANY
            || spread_type == TypeId::UNKNOWN
            || spread_type == TypeId::ERROR
        {
            return true;
        }

        // Resolve lazy types (type aliases) before checking iterability
        let spread_type = self.resolve_lazy_type(spread_type);

        if self.is_iterable_type(spread_type) {
            return true;
        }

        // Not iterable - emit TS2488

        if let Some((start, end)) = self.get_node_span(expr_idx) {
            let type_str = self.format_type(spread_type);
            let message = format_message(
                diagnostic_messages::TYPE_MUST_HAVE_A_SYMBOL_ITERATOR_METHOD_THAT_RETURNS_AN_ITERATOR,
                &[&type_str],
            );
            self.error(
                start,
                end.saturating_sub(start),
                message,
                diagnostic_codes::TYPE_MUST_HAVE_A_SYMBOL_ITERATOR_METHOD_THAT_RETURNS_AN_ITERATOR,
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
    /// - Checks if `pattern_type` is iterable
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

        // TypeScript allows empty array destructuring patterns on any type (including null/undefined)
        // Example: let [] = null; // No error
        // Skip iterability check if the pattern is empty.
        //
        // Track whether this is an assignment target (`[a] = value`) vs a binding pattern
        // (`let [a] = value`) so ES5-specific TS2461 can stay scoped to declarations.
        let mut is_assignment_array_target = false;
        if let Some(pattern_node) = self.ctx.arena.get(pattern_idx) {
            is_assignment_array_target =
                pattern_node.kind == tsz_parser::parser::syntax_kind_ext::ARRAY_LITERAL_EXPRESSION;
            if let Some(binding_pattern) = self.ctx.arena.get_binding_pattern(pattern_node)
                && binding_pattern.elements.nodes.is_empty()
            {
                return true;
            }
        }

        // Resolve lazy types (type aliases) before checking iterability
        let resolved_type = self.resolve_lazy_type(pattern_type);

        // In array destructuring, TypeScript still reports TS2488 for `never`.
        if resolved_type == TypeId::NEVER {
            let error_idx = if init_expr.is_some() {
                init_expr
            } else {
                pattern_idx
            };
            if let Some((start, end)) = self.get_node_span(error_idx) {
                let type_str = self.format_type(pattern_type);
                let message = format_message(
                    diagnostic_messages::TYPE_MUST_HAVE_A_SYMBOL_ITERATOR_METHOD_THAT_RETURNS_AN_ITERATOR,
                    &[&type_str],
                );
                self.error(
                    start,
                    end.saturating_sub(start),
                    message,
                    diagnostic_codes::TYPE_MUST_HAVE_A_SYMBOL_ITERATOR_METHOD_THAT_RETURNS_AN_ITERATOR,
                );
            }
            return false;
        }

        // In ES5 mode (without downlevelIteration), array destructuring requires actual arrays.
        // - Emit TS2802 if the type has Symbol.iterator (iterable but requires ES2015/downlevelIteration).
        // - Emit TS2461 if the type is not an array type.
        if self.ctx.compiler_options.target.is_es5() && !is_assignment_array_target {
            // Nested binding patterns can be fed an over-widened union from positional
            // destructuring inference (e.g. `[a, [b]] = [1, ["x"]]`). tsc does not report
            // TS2461 for these cases.
            if init_expr.is_none()
                && tsz_solver::type_queries::get_union_members(self.ctx.types, resolved_type)
                    .is_some_and(|members| {
                        members
                            .iter()
                            .any(|&member| self.is_array_or_tuple_type(member))
                    })
            {
                return true;
            }
            if self.is_array_or_tuple_type(resolved_type) {
                return true;
            }
            // Use the initializer expression for error location if available
            let error_idx = if init_expr.is_some() {
                init_expr
            } else {
                pattern_idx
            };
            if let Some((start, end)) = self.get_node_span(error_idx) {
                let type_str = self.format_type(pattern_type);
                // Check if the type has Symbol.iterator (iterable but not usable in ES5
                // without downlevelIteration). These emit TS2802 instead of TS2461.
                if self.is_iterable_type(resolved_type) {
                    let message = format_message(
                        diagnostic_messages::TYPE_CAN_ONLY_BE_ITERATED_THROUGH_WHEN_USING_THE_DOWNLEVELITERATION_FLAG_OR_WITH,
                        &[&type_str],
                    );
                    self.error(
                        start,
                        end.saturating_sub(start),
                        message,
                        diagnostic_codes::TYPE_CAN_ONLY_BE_ITERATED_THROUGH_WHEN_USING_THE_DOWNLEVELITERATION_FLAG_OR_WITH,
                    );
                } else {
                    let message = format_message(
                        diagnostic_messages::TYPE_IS_NOT_AN_ARRAY_TYPE,
                        &[&type_str],
                    );
                    self.error(
                        start,
                        end.saturating_sub(start),
                        message,
                        diagnostic_codes::TYPE_IS_NOT_AN_ARRAY_TYPE,
                    );
                }
            }
            return false;
        }

        // Check if the type is iterable (ES2015+)
        if self.is_iterable_type(resolved_type) {
            return true;
        }

        // TypeScript also allows array destructuring for "array-like" types
        // (types with numeric index signatures) even without [Symbol.iterator]()
        if self.has_numeric_index_signature(resolved_type) {
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
                diagnostic_messages::TYPE_MUST_HAVE_A_SYMBOL_ITERATOR_METHOD_THAT_RETURNS_AN_ITERATOR,
                &[&type_str],
            );
            self.error(
                start,
                end.saturating_sub(start),
                message,
                diagnostic_codes::TYPE_MUST_HAVE_A_SYMBOL_ITERATOR_METHOD_THAT_RETURNS_AN_ITERATOR,
            );
        }
        false
    }

    // =========================================================================
    // ES5 Type Classification Helpers
    // =========================================================================

    /// Check if a type is an array or tuple type (for ES5 destructuring).
    fn is_array_or_tuple_type(&self, type_id: TypeId) -> bool {
        if is_array_type(self.ctx.types, type_id) || is_tuple_type(self.ctx.types, type_id) {
            return true;
        }
        // Check unions: all members must be array/tuple
        if let Some(members) = union_members_for_type(self.ctx.types, type_id) {
            return members
                .iter()
                .all(|&member| self.is_array_or_tuple_type(member));
        }
        false
    }

    /// Check if a type contains a string-like constituent (for ES5 for-of error discrimination).
    ///
    /// This mirrors TSC's `hasStringConstituent` check: when a union type contains a string
    /// member alongside non-array types, the error changes from TS2495 to TS2461.
    fn has_string_constituent(&self, type_id: TypeId) -> bool {
        if type_id == TypeId::STRING || is_string_type(self.ctx.types, type_id) {
            return true;
        }
        if is_string_literal_type(self.ctx.types, type_id) {
            return true;
        }
        if let Some(members) = union_members_for_type(self.ctx.types, type_id) {
            return members.iter().any(|&m| self.has_string_constituent(m));
        }
        false
    }

    /// Check if a type is an array, tuple, or string type (for ES5 for-of).
    fn is_array_or_tuple_or_string(&self, type_id: TypeId) -> bool {
        if type_id == TypeId::STRING || is_string_type(self.ctx.types, type_id) {
            return true;
        }
        if is_array_type(self.ctx.types, type_id) || is_tuple_type(self.ctx.types, type_id) {
            return true;
        }
        // String literals count as string types
        if is_string_literal_type(self.ctx.types, type_id) {
            return true;
        }
        // Check unions: all members must be array/tuple/string
        if let Some(members) = union_members_for_type(self.ctx.types, type_id) {
            return members
                .iter()
                .all(|&member| self.is_array_or_tuple_or_string(member));
        }
        false
    }
}
