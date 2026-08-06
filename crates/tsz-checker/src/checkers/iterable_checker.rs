//! Iterable/iterator protocol checking and for-of element type computation.

use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
use crate::query_boundaries::checkers::call::spread_type_parameter_constraint_is_array_or_tuple_like_for_call;
use crate::query_boundaries::checkers::iterable::{
    AsyncIterableTypeKind, ForOfElementKind, FullIterableTypeKind, IterableProtocolMethodStatus,
    IteratorReturnPropertyStatus, NumericIndexSignatureFact, async_iterable_protocol_lookup_type,
    callable_accepts_no_required_args, callable_return_type, callable_type_is_callable,
    classify_async_iterable_type, classify_for_of_element_type, classify_full_iterable_type,
    evaluated_iterator_result_type, evaluated_iterator_result_value_types,
    intersection_element_type, is_array_type, is_string_literal_type, is_string_type, is_this_type,
    is_tuple_type, iterator_info_yield_type, iterator_method_status,
    iterator_return_property_status, numeric_index_signature_fact, promise_like_awaited_type,
    tuple_element_union_type, type_has_next_method, union_element_type, union_members_for_type,
};
use crate::query_boundaries::common;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

// =============================================================================
// Iterable Type Checking Methods
// =============================================================================

impl<'a> CheckerState<'a> {
    const fn requires_array_like_iteration_for_es5_target(&self) -> bool {
        self.ctx.compiler_options.target.is_es5() && !self.ctx.compiler_options.downlevel_iteration
    }

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
                // Check if object has a [Symbol.iterator] method in its shape.
                // If found (Some), verify the full iterator protocol (the return
                // value of [Symbol.iterator]() must have a `next()` method).
                // If not found (None), fall back to property access resolution for
                // computed properties from lib types or inherited properties.
                match self.object_has_iterator_method(shape_id) {
                    IterableProtocolMethodStatus::Valid => {
                        // [Symbol.iterator] exists and is callable. Verify the
                        // iterator protocol, but also accept cases where the return
                        // type is `undefined`/`void` — these occur when the method
                        // has a circular return type that resolves to implicit `any`
                        // (e.g., `for (var v of new C) { }` where `C.[Symbol.iterator]()`
                        // returns `v`). TypeScript does not emit TS2488 in these cases.
                        self.type_has_symbol_iterator_via_property_access_lenient(type_id)
                    }
                    IterableProtocolMethodStatus::Invalid => false,
                    IterableProtocolMethodStatus::NeedsPropertyAccess => {
                        self.type_has_symbol_iterator_via_property_access(type_id)
                    }
                }
            }
            FullIterableTypeKind::Application { .. } => {
                // Application types (Set<T>, Map<K,V>, Iterable<T>, etc.) may have
                // Lazy(DefId) bases that can't be resolved through the type classification.
                // Use the full property access resolution which handles all the complex
                // resolution paths including Application types with Lazy bases from lib files.
                if self.type_has_symbol_iterator_via_property_access(type_id) {
                    return true;
                }
                // A mapped-type alias application such as `DeepReadonlyObject<Iterable<T>>`
                // (the homomorphic branch a recursive conditional like `DeepReadonly`
                // selects) does not expose `[Symbol.iterator]` until its mapped body is
                // instantiated and evaluated. `tsc` checks iterability against the
                // apparent type, so resolve the alias application the same way a
                // property-access receiver is resolved, then re-classify. The
                // `resolved != type_id` guard avoids looping when it stays opaque.
                let resolved = self.resolve_type_for_property_access(type_id);
                resolved != type_id && self.is_iterable_type(resolved)
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
            // Index access, Conditional, Mapped: not iterable in their deferred
            // form, but `tsc` checks iterability against the *apparent* type. A
            // mapped type like `{ [K in keyof Iterable<T>]: ... }` resolves to an
            // object that keeps the `[Symbol.iterator]` method, so for-of must see
            // through it. Resolve the receiver the same way property access does
            // and re-classify; the `resolved != type_id` guard prevents looping
            // on a form that stays deferred (e.g. a conditional that genuinely
            // depends on a free type parameter).
            FullIterableTypeKind::ComplexType => {
                let resolved = self.resolve_type_for_property_access(type_id);
                resolved != type_id && self.is_iterable_type(resolved)
            }
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
    /// Returns `Some(true)` if found and valid, `Some(false)` if found but invalid
    /// (optional or has required params), `None` if not found in the shape.
    fn object_has_iterator_method(
        &self,
        shape_id: tsz_solver::ObjectShapeId,
    ) -> IterableProtocolMethodStatus {
        iterator_method_status(self.ctx.types, shape_id)
    }

    /// Check if a type has [Symbol.iterator] using the full property access resolution,
    /// AND that calling it returns an iterator (something with a `next()` method).
    ///
    /// This handles Application types (Set<T>, Map<K,V>) with Lazy(DefId) bases from lib
    /// files, Callable types with iterator properties, and other complex cases where simple
    /// shape inspection fails but the full checker resolution machinery can find the property.
    fn type_has_symbol_iterator_via_property_access(&mut self, type_id: TypeId) -> bool {
        use crate::query_boundaries::common::PropertyAccessResult;
        let result = self.resolve_property_access_with_env(type_id, "[Symbol.iterator]");
        match result {
            PropertyAccessResult::Success {
                type_id: iterator_fn_type,
                ..
            } => {
                // Verify the full iterator protocol: calling [Symbol.iterator]()
                // must return something with a `next()` method.
                self.iterator_fn_returns_valid_iterator(type_id, iterator_fn_type)
            }
            _ => false,
        }
    }

    /// Like `type_has_symbol_iterator_via_property_access`, but also accepts cases
    /// where the iterator factory returns `undefined` or `void`. This handles
    /// circular reference scenarios where the method return type wasn't updated to
    /// `any` after circular detection (e.g., for-of self-referencing variables).
    fn type_has_symbol_iterator_via_property_access_lenient(&mut self, type_id: TypeId) -> bool {
        use crate::query_boundaries::common::PropertyAccessResult;
        let result = self.resolve_property_access_with_env(type_id, "[Symbol.iterator]");
        match result {
            PropertyAccessResult::Success {
                type_id: iterator_fn_type,
                ..
            } => {
                let iterator_type = callable_return_type(self.ctx.types, iterator_fn_type);
                // Accept undefined/void as they typically result from circular
                // reference resolution where the type should really be `any`.
                if iterator_type == TypeId::UNDEFINED || iterator_type == TypeId::VOID {
                    return true;
                }
                self.iterator_fn_returns_valid_iterator(type_id, iterator_fn_type)
            }
            _ => false,
        }
    }

    /// Verify that calling an iterator factory function returns a valid iterator
    /// (i.e., an object with a `next()` method).
    ///
    /// This catches cases like:
    /// ```ts
    /// class Bad { [Symbol.iterator]() { return this; } }
    /// // Bad has [Symbol.iterator] but no next() → NOT a valid iterable
    /// ```
    fn iterator_fn_returns_valid_iterator(
        &mut self,
        iterable_type: TypeId,
        iterator_fn_type: TypeId,
    ) -> bool {
        // Get the return type of calling [Symbol.iterator]()
        let iterator_type = callable_return_type(self.ctx.types, iterator_fn_type);

        // If the return type is any/unknown/error, accept it (don't flag)
        if iterator_type == TypeId::ANY
            || iterator_type == TypeId::UNKNOWN
            || iterator_type == TypeId::ERROR
        {
            return true;
        }

        // If the iterator function returns `ThisType` (polymorphic `this` from
        // `return this` in class methods), substitute with the iterable type itself.
        let iterator_type = if is_this_type(self.ctx.types, iterator_type) {
            iterable_type
        } else {
            iterator_type
        };

        // Check if the iterator type has a `next` property by inspecting
        // the object shape directly, rather than using property access resolution
        // which may return `any` as a fallback for missing properties.
        type_has_next_method(self.ctx.types, iterator_type)
            || (iterator_type != iterable_type
                && type_has_next_method(self.ctx.types, iterable_type))
    }

    /// Check if a type has a `next` method by examining its object shape directly.
    ///
    /// This is more precise than `resolve_property_access_with_env` because it
    /// doesn't fall back to `any` for missing properties. Used to verify the
    /// iterator protocol: the return value of `[Symbol.iterator]()` must have `next()`.
    /// Check if a type has a numeric index signature, making it "array-like".
    /// TypeScript allows array destructuring of array-like types without [Symbol.iterator]().
    pub(crate) fn has_numeric_index_signature(&mut self, type_id: TypeId) -> bool {
        // Resolve lazy types first
        let type_id = self.resolve_lazy_type(type_id);
        match numeric_index_signature_fact(self.ctx.types, type_id) {
            NumericIndexSignatureFact::Present => true,
            NumericIndexSignatureFact::Recurse(inner) => self.has_numeric_index_signature(inner),
            NumericIndexSignatureFact::Union(members) => members
                .iter()
                .all(|&m| self.is_iterable_type(m) || self.has_numeric_index_signature(m)),
            NumericIndexSignatureFact::Absent => false,
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
            AsyncIterableTypeKind::Intersection(members) => {
                // An intersection is async iterable if at least one member is,
                // mirroring the sync `is_iterable_type` intersection rule.
                members.iter().any(|&m| self.is_async_iterable_type(m))
            }
            AsyncIterableTypeKind::TypeParameter { constraint } => {
                // `for await ... of` resolves a type parameter to its apparent
                // type (the constraint). Recurse into the constraint instead of
                // probing `[Symbol.asyncIterator]` on the bare parameter, which
                // cannot see through a generic `Application` constraint such as
                // `AsyncIterableIterator<T>`. An unconstrained parameter is not
                // async iterable here; the caller still falls back to the sync
                // iterable check before reporting TS2504.
                constraint.is_some_and(|c| self.is_async_iterable_type(c))
            }
            AsyncIterableTypeKind::Object(shape_id) => {
                // Check if object has a [Symbol.asyncIterator] method or callable property.
                // Both `{ [Symbol.asyncIterator]() { ... } }` (method) and
                // `{ [Symbol.asyncIterator]: generatorFn }` (callable value) are valid.
                match self.object_has_async_iterator_method(shape_id) {
                    IterableProtocolMethodStatus::Valid => true,
                    IterableProtocolMethodStatus::Invalid => false,
                    IterableProtocolMethodStatus::NeedsPropertyAccess => {
                        self.type_has_symbol_async_iterator_via_property_access(type_id)
                    }
                }
            }
            AsyncIterableTypeKind::Readonly(inner) => {
                // Unwrap readonly wrapper and check inner type
                self.is_async_iterable_type(inner)
            }
            AsyncIterableTypeKind::NotAsyncIterable => {
                // Use property access to check for [Symbol.asyncIterator] on types
                // that couldn't be classified (e.g., Application types with Lazy bases).
                self.type_has_symbol_async_iterator_via_property_access(type_id)
            }
        }
    }

    fn object_has_async_iterator_method(
        &self,
        shape_id: tsz_solver::ObjectShapeId,
    ) -> IterableProtocolMethodStatus {
        crate::query_boundaries::checkers::iterable::async_iterator_method_status(
            self.ctx.types,
            shape_id,
        )
    }

    fn type_has_symbol_async_iterator_via_property_access(&mut self, type_id: TypeId) -> bool {
        use crate::query_boundaries::common::PropertyAccessResult;
        let lookup_type = async_iterable_protocol_lookup_type(self.ctx.types, type_id);
        match self.resolve_property_access_with_env(lookup_type, "[Symbol.asyncIterator]") {
            PropertyAccessResult::Success {
                type_id: iterator_fn_type,
                ..
            } => callable_accepts_no_required_args(self.ctx.types, iterator_fn_type),
            _ => false,
        }
    }

    /// Returns true when an async iterator's `next()` result is thenable but not a
    /// valid promise/thenable shape that can be awaited safely.
    pub fn async_iterator_has_invalid_thenable_next_result(&mut self, type_id: TypeId) -> bool {
        use crate::query_boundaries::common::PropertyAccessResult;

        let type_id = self.resolve_lazy_type(type_id);

        // Distribute over union delegates: `tsc` accepts a union `yield*`
        // delegate as long as every member's async iterator has a valid
        // `next()` result, so the union as a whole is invalid only if some
        // member is. Resolving `[Symbol.asyncIterator]`/`next()` on the union
        // `type_id` as a single receiver (the pre-fix behavior) reads an
        // inconsistent cross-member result and reports a spurious TS1320 even
        // when each member alone is clean.
        if let Some(members) = union_members_for_type(self.ctx.types, type_id) {
            return members
                .iter()
                .any(|&member| self.async_iterator_has_invalid_thenable_next_result(member));
        }

        let iterator_fn = self.resolve_property_access_with_env(type_id, "[Symbol.asyncIterator]");
        let iterator_fn_type = match iterator_fn {
            PropertyAccessResult::Success { type_id, .. } => type_id,
            _ => return false,
        };

        let iterator_type = callable_return_type(self.ctx.types, iterator_fn_type);
        let iterator_type =
            if iterator_type == TypeId::ANY || is_this_type(self.ctx.types, iterator_type) {
                type_id
            } else {
                iterator_type
            };

        let next_result = self.resolve_property_access_with_env(iterator_type, "next");
        let mut next_fn_type = match next_result {
            PropertyAccessResult::Success { type_id, .. } => type_id,
            _ => return false,
        };

        if next_fn_type == TypeId::ANY && iterator_type != type_id {
            let fallback_next = self.resolve_property_access_with_env(type_id, "next");
            if let PropertyAccessResult::Success { type_id: fb, .. } = fallback_next
                && fb != TypeId::ANY
            {
                next_fn_type = fb;
            }
        }

        let next_return = callable_return_type(self.ctx.types, next_fn_type);
        crate::query_boundaries::flow_analysis::is_promise_like_type(self.ctx.types, next_return)
            && self
                .promise_like_return_type_argument(next_return)
                .is_none()
    }

    // =========================================================================
    // For-Of Element Type Computation
    // =========================================================================

    /// Compute the element type produced by a `for (... of expr)` or
    /// `for await (... of expr)` loop.
    ///
    /// Handles arrays, tuples, unions, strings, and custom iterators via
    /// the `[Symbol.iterator]().next().value` protocol.
    ///
    /// When `is_async` is true (`for await...of`), the element type is awaited,
    /// so `Iterable<Promise<T>>` yields `T` instead of `Promise<T>`.
    pub fn for_of_element_type(&mut self, iterable_type: TypeId, is_async: bool) -> TypeId {
        self.for_of_element_type_with_depth(iterable_type, is_async, 0)
    }

    fn for_of_element_type_with_depth(
        &mut self,
        iterable_type: TypeId,
        is_async: bool,
        depth: usize,
    ) -> TypeId {
        if depth > 100 {
            return TypeId::ANY;
        }

        if iterable_type == TypeId::ANY || iterable_type == TypeId::ERROR {
            return iterable_type;
        }
        if iterable_type == TypeId::UNKNOWN {
            return TypeId::ANY;
        }

        // String iteration yields string
        if iterable_type == TypeId::STRING {
            return TypeId::STRING;
        }

        // Resolve lazy types (type aliases) before computing element type
        let iterable_type = self.resolve_lazy_type(iterable_type);
        if iterable_type == TypeId::UNKNOWN {
            return TypeId::ANY;
        }

        match classify_for_of_element_type(self.ctx.types, iterable_type) {
            ForOfElementKind::TypeParameter { constraint } => {
                return constraint
                    .map(|constraint| {
                        self.for_of_element_type_with_depth(constraint, is_async, depth + 1)
                    })
                    .unwrap_or(TypeId::ANY);
            }
            // Distribute async iteration over a union/intersection delegate
            // member-by-member. The sync `for_of_element_type_classified`
            // fallback below walks `[Symbol.iterator]`, so an async-only member
            // (`AsyncGenerator<T>`, `AsyncIterable<T>`) inside a union would
            // collapse to `ANY` there; resolving each constituent through this
            // async-aware entry recovers it. tsc's iteration type over a
            // union/intersection is the union/intersection of the members'.
            // (The sync arm already distributes inside
            // `for_of_element_type_classified`, so this is gated on `is_async`.)
            ForOfElementKind::Union(members) if is_async => {
                let element_types = members
                    .into_iter()
                    .map(|member| self.for_of_element_type_with_depth(member, is_async, depth + 1))
                    .collect();
                return union_element_type(self.ctx.types, element_types);
            }
            ForOfElementKind::Intersection(members) if is_async => {
                let element_types = members
                    .into_iter()
                    .map(|member| self.for_of_element_type_with_depth(member, is_async, depth + 1))
                    .collect();
                return intersection_element_type(self.ctx.types, element_types);
            }
            _ => {}
        }

        if is_async {
            // For for-await-of: try async iterator protocol first (AsyncIterable<T> -> T),
            // then fall back to sync iterator + Promise unwrapping (Iterable<Promise<T>> -> T).
            if let Some(yield_type) = iterator_info_yield_type(self.ctx.types, iterable_type, true)
                && yield_type != TypeId::ANY
            {
                // The solver iterator-info helper fast-paths Array/Tuple as sync iterators
                // regardless of is_async, so for-await-of must additionally await their
                // element type.
                if matches!(
                    classify_for_of_element_type(self.ctx.types, iterable_type),
                    ForOfElementKind::Array(_) | ForOfElementKind::Tuple(_)
                ) {
                    return self.apply_awaited(yield_type);
                }
                return yield_type;
            }
            // Solver-level resolution can return ANY (or None) for Application
            // receivers — e.g. `AsyncIterable<number>` viewed through a
            // `Lazy(DefId)` base — because its property-access evaluator does
            // not always evaluate through the alias body, and because the
            // naive `IteratorResult.value` read collapses the discriminated
            // union to `T | TReturn = any`. Fall back to the checker's
            // property-access chain plus the `done`-partitioning helper, both
            // of which handle Application receivers and recover the true `T`.
            if self.is_async_iterable_type(iterable_type) {
                let async_yield =
                    self.resolve_async_iterator_element_type_via_property_access(iterable_type);
                if async_yield != TypeId::ANY {
                    return async_yield;
                }
            }
            // Fall back to sync iterator protocol + Promise unwrapping.
            let elem_type = self.for_of_element_type_classified(iterable_type, 0);
            self.apply_awaited(elem_type)
        } else {
            self.for_of_element_type_classified(iterable_type, 0)
        }
    }

    /// Follow the async-iterator protocol chain via checker property access.
    ///
    /// Mirrors `resolve_iterator_element_type_via_property_access` but for
    /// `[Symbol.asyncIterator]` / `Promise<IteratorResult<T>>`. Used when the
    /// solver-level iterator-info resolution cannot resolve through an
    /// `Application(Lazy(DefId), [..])` receiver but the checker's
    /// `resolve_property_access_with_env` (which evaluates the alias body)
    /// can.
    fn resolve_async_iterator_element_type_via_property_access(
        &mut self,
        type_id: TypeId,
    ) -> TypeId {
        use crate::query_boundaries::common::PropertyAccessResult;

        // Step 1: Get [Symbol.asyncIterator] property.
        let iterator_fn = self.resolve_property_access_with_env(type_id, "[Symbol.asyncIterator]");
        let iterator_fn_type = match &iterator_fn {
            PropertyAccessResult::Success { type_id, .. } => *type_id,
            _ => return TypeId::ANY,
        };

        // Step 2: Call [Symbol.asyncIterator]() to get the AsyncIterator type.
        let iterator_type = callable_return_type(self.ctx.types, iterator_fn_type);
        let iterator_type = if iterator_type == TypeId::ANY
            || crate::query_boundaries::common::is_this_type(self.ctx.types, iterator_type)
        {
            type_id
        } else {
            iterator_type
        };

        // Step 3: Get next() method on the async iterator.
        let next_fn = self.resolve_property_access_with_env(iterator_type, "next");
        let next_fn_type = match next_fn {
            PropertyAccessResult::Success { type_id, .. } => type_id,
            _ => return TypeId::ANY,
        };

        // Step 4: Call next() to get Promise<IteratorResult<T>>, then await it
        // to get IteratorResult<T>, then extract the yield value type.
        let next_return = callable_return_type(self.ctx.types, next_fn_type);
        let awaited_next = self.apply_awaited(next_return);

        // Extract the yield type from the IteratorResult discriminated union by
        // partitioning on `done` — naive `.value` access would conflate the
        // yield value (`done:false` branch) with the `TReturn` value
        // (`done:true` branch) and produce `T | TReturn = any`.
        //
        // Mirror the sync counterpart at line 762: evaluate Lazy/Conditional
        // wrappers before partitioning so Application-receiver shapes (the
        // very case this method targets) actually expand into the
        // `IteratorResult` discriminated union before we look for `done:true`
        // branches. Skipping `evaluate_type` previously caused
        // iterator-result extraction to fall through to its
        // ANY-ANY fallback for the common case.
        let resolved_awaited_next = evaluated_iterator_result_type(self.ctx.types, awaited_next);
        let (yield_type, _return_type) =
            crate::query_boundaries::checkers::iterable::iterator_result_value_types(
                self.ctx.types,
                resolved_awaited_next,
            );
        // Treat ANY as extraction failure (the operation returns
        // `(ANY, ANY)` for unresolved shapes) so we fall through to the
        // `.value` access fallback below — matching the sync version's
        // `yield_type != TypeId::ANY` gate. Without this, ANY short-
        // circuited the success path and the fallback was dead code.
        if yield_type != TypeId::ANY && yield_type != TypeId::NEVER && yield_type != TypeId::ERROR {
            return yield_type;
        }
        // Fallback: naive `value` access if partitioning yielded nothing.
        let value_access = self.resolve_property_access_with_env(resolved_awaited_next, "value");
        match value_access {
            PropertyAccessResult::Success { type_id, .. } => type_id,
            _ => TypeId::ANY,
        }
    }

    /// Internal helper that uses the solver's classification enum to compute element type.
    /// The depth parameter prevents infinite loops from circular readonly types.
    fn for_of_element_type_classified(&mut self, type_id: TypeId, depth: usize) -> TypeId {
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
                // A rest element (`...T[]`) contributes the iterated ELEMENT type of
                // its array, not the array type itself; fixed (non-rest) elements
                // contribute their own type. Iterating `[Base, ...Base[]]` yields
                // `Base`, not `Base | Base[]`. Computing the rest's element type via
                // the same iterable element-type query keeps it a solver-backed
                // decision and resolves concrete arrays, readonly arrays, and generic
                // `T extends U[]` rests uniformly.
                let member_types: Vec<TypeId> = elements
                    .iter()
                    .map(|e| {
                        if e.rest {
                            self.for_of_element_type_classified(e.type_id, depth + 1)
                        } else {
                            e.type_id
                        }
                    })
                    .collect();
                tuple_element_union_type(self.ctx.types, member_types)
            }
            ForOfElementKind::Union(members) => {
                let mut element_types = Vec::with_capacity(members.len());
                for member in members {
                    element_types.push(self.for_of_element_type_classified(member, depth + 1));
                }
                union_element_type(self.ctx.types, element_types)
            }
            ForOfElementKind::Intersection(members) => {
                // For an intersection of iterables (e.g. X[] & Y[]),
                // the element type is the intersection of each member's element type.
                let mut element_types = Vec::with_capacity(members.len());
                for member in members {
                    element_types.push(self.for_of_element_type_classified(member, depth + 1));
                }
                intersection_element_type(self.ctx.types, element_types)
            }
            ForOfElementKind::Readonly(inner) => {
                // Unwrap readonly wrapper and compute element type for inner
                self.for_of_element_type_classified(inner, depth + 1)
            }
            ForOfElementKind::TypeParameter { constraint } => constraint
                .map(|constraint| self.for_of_element_type_classified(constraint, depth + 1))
                .unwrap_or(TypeId::ANY),
            ForOfElementKind::String => TypeId::STRING,
            ForOfElementKind::Other => {
                // For custom iterators, Application types (Map, Set), etc.,
                // use the solver's iterator protocol resolution which properly
                // handles Application types and type parameter substitution.
                self.resolve_iterator_element_type(type_id)
            }
        }
    }

    /// Unwrap `Promise<T>` → `T`; returns `ty` unchanged for non-promise types.
    fn apply_awaited(&mut self, ty: TypeId) -> TypeId {
        if ty.is_intrinsic() {
            return ty;
        }
        self.unwrap_promise_type(ty)
            .or_else(|| promise_like_awaited_type(self.ctx.types, ty))
            .unwrap_or(ty)
    }

    /// Resolve the element type of an iterable via the iterator protocol.
    ///
    /// Uses a hybrid approach:
    /// 1. First tries the solver iterator-info helper, which properly handles
    ///    Application types (`IterableIterator`<T>, `IteratorResult`<T>).
    /// 2. Falls back to checker-level property access chain which handles
    ///    merged declarations (`IArguments`) and custom iterator classes.
    ///
    /// Returns ANY as fallback if the protocol cannot be resolved.
    pub(crate) fn resolve_iterator_element_type(&mut self, type_id: TypeId) -> TypeId {
        // Try solver-level iterator resolution first (handles Application types correctly)
        // `ANY` is also the solver's unresolved-iterator sentinel. Let the
        // environment-aware property chain distinguish that from an explicit
        // `Generator<any>` yield, mirroring the async iterator path above.
        if let Some(yield_type) = iterator_info_yield_type(self.ctx.types, type_id, false)
            && yield_type != TypeId::ANY
        {
            return yield_type;
        }

        // Fall back to checker-level property access chain which handles
        // merged declarations and custom iterator classes
        self.resolve_iterator_element_type_via_property_access(type_id)
    }

    /// Follow the iterator protocol chain via checker property access.
    ///
    /// Follows: type[Symbol.iterator] → call → .`next()` → call → extract yield from `IteratorResult`
    ///
    /// The `IteratorResult` type is a discriminated union:
    ///   { done?: false, value: T } | { done: true, value: `TReturn` }
    /// For for-of loops, only the yield type T matters (from done:false branches).
    /// We use the iterator-result boundary helper to properly partition by `done`
    /// instead of naively reading `.value` (which would give T | `TReturn`).
    fn resolve_iterator_element_type_via_property_access(&mut self, type_id: TypeId) -> TypeId {
        use crate::query_boundaries::common::PropertyAccessResult;

        // Step 1: Get [Symbol.iterator] property
        let iterator_fn = self.resolve_property_access_with_env(type_id, "[Symbol.iterator]");
        let iterator_fn_type = match &iterator_fn {
            PropertyAccessResult::Success { type_id, .. } => *type_id,
            _ => return TypeId::ANY,
        };

        // Step 2: Get the return type of the iterator function (call it)
        let iterator_type = callable_return_type(self.ctx.types, iterator_fn_type);

        // If the iterator function returns `any` (e.g., `[Symbol.iterator]() { return this; }`
        // where `this` type inference fails), or `ThisType` (polymorphic `this` from
        // `return this` in class methods), fall back to the original iterable type.
        // For `ThisType`, `this` in `[Symbol.iterator]()` refers to the iterable itself,
        // so substituting with `type_id` gives us the concrete class instance type.
        let iterator_type =
            if iterator_type == TypeId::ANY || is_this_type(self.ctx.types, iterator_type) {
                type_id
            } else {
                iterator_type
            };

        // Step 3: Get .next() on the iterator
        let next_result = self.resolve_property_access_with_env(iterator_type, "next");
        let mut next_fn_type = match &next_result {
            PropertyAccessResult::Success { type_id, .. } => *type_id,
            _ => return TypeId::ANY,
        };

        // If next() resolves to `any` but the iterator type differs from the
        // original iterable, retry on the original iterable.  This handles
        // classes where `[Symbol.iterator]()` returns `this` — the call return
        // type may resolve to an intermediate representation that doesn't
        // expose method signatures, while the original class type does.
        if next_fn_type == TypeId::ANY && iterator_type != type_id {
            let fallback_next = self.resolve_property_access_with_env(type_id, "next");
            if let PropertyAccessResult::Success { type_id: fb, .. } = &fallback_next
                && *fb != TypeId::ANY
            {
                next_fn_type = *fb;
            }
        }

        // Step 4: Get the return type of next() — this is the IteratorResult type
        let next_return = callable_return_type(self.ctx.types, next_fn_type);

        // Step 5: Extract the yield type from IteratorResult.
        //
        // IteratorResult<T, TReturn> = { done?: false, value: T } | { done: true, value: TReturn }
        // For for-of loops, only the yield type T matters (from done:false branches).
        //
        // First try the solver's discriminant-aware extraction on the evaluated type.
        let (yield_type, _return_type) =
            evaluated_iterator_result_value_types(self.ctx.types, next_return);

        if yield_type != TypeId::ANY {
            return yield_type;
        }

        // Fallback: read .value directly (gives T | TReturn, which is less precise
        // but works for non-standard iterator shapes)
        let value_result = self.resolve_property_access_with_env(next_return, "value");
        let value_type = match &value_result {
            PropertyAccessResult::Success { type_id, .. } => *type_id,
            _ => return TypeId::ANY,
        };

        // If .value resolved to `unknown` (unresolved Application type),
        // try the solver's iterator info on the iterator object itself
        if value_type == TypeId::UNKNOWN {
            if let Some(yield_type) = iterator_info_yield_type(self.ctx.types, iterator_type, false)
            {
                return yield_type;
            }
            return TypeId::ANY;
        }

        value_type
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
        // Skip error/any types to prevent false positives.
        if expr_type == TypeId::ANY || expr_type == TypeId::ERROR {
            return true;
        }
        if expr_type == TypeId::UNKNOWN {
            return !self.error_is_of_type_unknown(expr_idx);
        }

        // Resolve lazy types (type aliases) before checking iterability
        let expr_type = self.resolve_lazy_type(expr_type);
        if expr_type == TypeId::UNKNOWN {
            return !self.error_is_of_type_unknown(expr_idx);
        }

        // Check if the expression is nullish (undefined/null)
        // Emit TS18050 "The value 'undefined'/'null' cannot be used here"
        // when trying to iterate over undefined/null
        if expr_type == TypeId::NULL || expr_type == TypeId::UNDEFINED {
            self.report_nullish_object(expr_idx, expr_type, true);
            return false;
        }

        // For async for-of, first check async iterable, then fall back to sync iterable.
        // For union types like `Iterable<T> | AsyncIterable<T>`, tsc checks each member
        // individually — each member must be EITHER async iterable OR sync iterable.
        if is_async {
            if self.is_async_iterable_type(expr_type) || self.is_iterable_type(expr_type) {
                // Check that undefined is assignable to the iterator's TNext (TS2763).
                // for-await-of always sends undefined to next().
                self.check_iterator_next_type_assignability(
                    expr_type,
                    TypeId::UNDEFINED,
                    expr_idx,
                    IterationUseKind::ForOf,
                );
                return true;
            }
            // For unions, check if each member is individually async- or sync-iterable
            if let Some(members) = union_members_for_type(self.ctx.types, expr_type)
                && members
                    .iter()
                    .all(|&m| self.is_async_iterable_type(m) || self.is_iterable_type(m))
            {
                // Check next type assignability for each member
                self.check_iterator_next_type_assignability(
                    expr_type,
                    TypeId::UNDEFINED,
                    expr_idx,
                    IterationUseKind::ForOf,
                );
                return true;
            }
            // If the AsyncIterator/AsyncIterable globals aren't in scope (e.g. `@lib: es5`
            // with no es2018.asynciterable lib entry), tsc can't talk about
            // `[Symbol.asyncIterator]()` at all and falls back to the ES5-style
            // "not an array type or a string type" check. Mirror that: if neither
            // AsyncIterator nor AsyncIterable resolves in the current program, treat
            // the `for await ... of` like a plain `for ... of` in ES5 mode.
            let async_iter_available = self.resolve_lib_type_by_name("AsyncIterator").is_some()
                || self.resolve_lib_type_by_name("AsyncIterable").is_some();
            if !async_iter_available {
                if self.is_array_or_tuple_or_string(expr_type) {
                    return true;
                }
                let allows_strings = !self.has_string_constituent(expr_type);
                self.emit_es5_not_iterable_error(expr_type, expr_type, expr_idx, allows_strings);
                return false;
            }
            // Not async iterable - emit TS2504
            if let Some((start, end)) = self.get_node_span(expr_idx) {
                let display_id = self.iterand_display_type(expr_idx, expr_type);
                let type_str = self.format_type(display_id);
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
        if self.requires_array_like_iteration_for_es5_target() {
            if self.is_array_or_tuple_or_string(expr_type)
                || self.es5_constrained_generic_is_array_like(expr_type)
            {
                return true;
            }
            // Mirror TSC's logic: strip string-like members from union types.
            // If there were string members, the "remaining" non-string type still needs to be
            // array-like, and the error message changes from TS2495 → TS2461 (no "or string type"
            // suffix because the string part is already accounted for).
            let allows_strings = !self.has_string_constituent(expr_type);
            self.emit_es5_not_iterable_error(expr_type, expr_type, expr_idx, allows_strings);
            return false;
        }

        // Regular for-of (ES2015+) - check sync iterability
        if self.is_iterable_type(expr_type) {
            // Additional check: verify the iterator protocol is complete.
            // The type returned by next() must have a 'value' property (TS2490).
            // This catches custom iterator classes where next() returns the wrong type.
            if !self.check_iterator_next_returns_value(expr_type, expr_idx) {
                return false;
            }
            // Check that 'return' property (if present) is a method, not a non-callable value (TS2767).
            self.check_iterator_return_is_method(expr_type, expr_idx);
            // Check that undefined is assignable to the iterator's TNext (TS2763).
            // for-of always sends undefined to next(), so if TNext != undefined, that's an error.
            self.check_iterator_next_type_assignability(
                expr_type,
                TypeId::UNDEFINED,
                expr_idx,
                IterationUseKind::ForOf,
            );
            return true;
        }

        // Not iterable - emit TS2488 (the emitter preserves a literal iterand).
        self.emit_ts2488_not_iterable(expr_type, expr_idx, false);
        false
    }

    /// Extra ES5 array-like recognition for the iterability checks: a **readonly**
    /// array (`readonly T[]` / `ReadonlyArray<T>`) iterates as an array in ES5 —
    /// no `--downlevelIteration` is needed — and a generic type parameter
    /// constrained to an array/tuple (e.g. `A extends ReadonlyArray<number>`)
    /// does too, so tsc emits no TS2802 for either. `is_array_or_tuple_type` does
    /// not see readonly arrays or a parameter's apparent type, so check the
    /// element type (which resolves readonly/array/tuple) and follow a type
    /// parameter to its constraint.
    fn es5_constrained_generic_is_array_like(&mut self, type_id: TypeId) -> bool {
        let mut t = self.resolve_lazy_type(type_id);
        let mut hops = 0;
        loop {
            if self.is_array_or_tuple_type(t)
                || crate::query_boundaries::type_computation::complex::is_readonly_type(
                    self.ctx.types,
                    t,
                )
            {
                return true;
            }
            hops += 1;
            if hops > 16 {
                return false;
            }
            let Some(constraint) = common::type_parameter_constraint(self.ctx.types, t) else {
                return false;
            };
            let resolved = self.resolve_lazy_type(constraint);
            if resolved == t {
                return false;
            }
            t = resolved;
        }
    }

    /// Check iterability of a spread argument and emit TS2488 if not iterable.
    ///
    /// Used for spread in array literals and function call arguments.
    /// Returns `true` if the type is iterable.
    pub fn check_spread_iterability(&mut self, spread_type: TypeId, expr_idx: NodeIndex) -> bool {
        // In ES5 without downlevel iteration, spread requires an array/tuple source.
        // Match tsc by emitting TS2461 for non-array spread arguments.
        if self.requires_array_like_iteration_for_es5_target() {
            if spread_type == TypeId::ANY || spread_type == TypeId::UNKNOWN {
                return true;
            }

            if self.es5_constrained_generic_is_array_like(spread_type) {
                return true;
            }
            let resolved = self.resolve_lazy_type(spread_type);
            if self.is_array_or_tuple_type(resolved) || self.has_numeric_index_signature(resolved) {
                return true;
            }

            // Spread never uses the "or a string type" variant (allows_strings = false).
            self.emit_es5_not_iterable_error(resolved, resolved, expr_idx, false);
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
            // Check that undefined is assignable to the iterator's TNext (TS2764).
            // Array spread always sends undefined to next().
            self.check_iterator_next_type_assignability(
                spread_type,
                TypeId::UNDEFINED,
                expr_idx,
                IterationUseKind::Spread,
            );
            return true;
        }

        // Generic mapped tuple/object forms (`{ [K in keyof T]: ... }`) are used
        // as spread sources in variadic generic flows. tsc does not report TS2488
        // for these unresolved generic mapped types at this point.
        if common::is_generic_mapped_type(self.ctx.types, spread_type) {
            let anchor = self.spread_iterability_error_anchor(expr_idx);
            if anchor != expr_idx {
                self.emit_ts2589_spread_instantiation_depth(anchor);
                return false;
            }
            return true;
        }

        // Conditional/IndexAccess/Application types containing free type parameters
        // cannot have iterability proven at the generic instantiation boundary —
        // tsc defers TS2488 to instantiation time for these deferred generic types.
        if (common::is_conditional_type(self.ctx.types, spread_type)
            || common::is_index_access_type(self.ctx.types, spread_type)
            || common::is_generic_type(self.ctx.types, spread_type))
            && common::contains_free_type_parameters(self.ctx.types, spread_type)
        {
            return true;
        }

        // A type parameter whose constraint resolves to an array/tuple — including
        // a deferred conditional utility like `Parameters<F>` or a user conditional
        // `T extends … ? unknown[] : never` — is iterable (its values are
        // array-like at runtime). The plain `is_iterable_type` probe above misses
        // this because the constraint stays a deferred conditional whose array base
        // surfaces only after tsc's `getConstraintFromConditionalType` resolution.
        // Mirrors the spread-of-type-parameter gate in `candidate_collection`.
        if spread_type_parameter_constraint_is_array_or_tuple_like_for_call(
            self.ctx.types,
            spread_type,
            |ty| self.evaluate_type_with_env(ty),
        ) {
            return true;
        }

        // Some recursive generic mapped/tuple spreads overflow type instantiation
        // depth during assignability-style evaluation (tsc reports TS2589 in these
        // cases instead of a follow-on TS2488 at the spread operand).
        let prev_depth_exceeded = self.ctx.depth_exceeded.get();
        self.ctx.depth_exceeded.set(false);
        self.evaluate_type_for_assignability(spread_type);
        let depth_exceeded = self.ctx.depth_exceeded.get();
        self.ctx
            .depth_exceeded
            .set(prev_depth_exceeded || depth_exceeded);
        if depth_exceeded {
            self.emit_ts2589_spread_instantiation_depth(
                self.spread_iterability_error_anchor(expr_idx),
            );
            return false;
        }

        // Not iterable - emit TS2488
        self.emit_ts2488_not_iterable(spread_type, expr_idx, false);
        false
    }

    pub(crate) fn spread_iterability_error_anchor(&self, expr_idx: NodeIndex) -> NodeIndex {
        let mut current = self.ctx.arena.parent_of(expr_idx);
        while let Some(parent_idx) = current {
            let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
                break;
            };
            if parent_node.kind == tsz_parser::parser::syntax_kind_ext::CALL_EXPRESSION
                || parent_node.kind == tsz_parser::parser::syntax_kind_ext::NEW_EXPRESSION
            {
                return parent_idx;
            }
            current = self
                .ctx
                .arena
                .get_extended(parent_idx)
                .map(|ext| ext.parent);
        }
        expr_idx
    }

    pub(crate) fn emit_ts2589_spread_instantiation_depth(&mut self, error_node: NodeIndex) {
        self.error_at_node(
            error_node,
            diagnostic_messages::TYPE_INSTANTIATION_IS_EXCESSIVELY_DEEP_AND_POSSIBLY_INFINITE,
            diagnostic_codes::TYPE_INSTANTIATION_IS_EXCESSIVELY_DEEP_AND_POSSIBLY_INFINITE,
        );
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
    /// - Skips check for ANY and ERROR types (defer to other checks)
    /// - Reports TS2488 for `unknown`, matching TypeScript's array destructuring behavior
    pub fn check_destructuring_iterability(
        &mut self,
        pattern_idx: NodeIndex,
        pattern_type: TypeId,
        init_expr: NodeIndex,
    ) -> bool {
        // Skip check for types that defer to other validation
        if pattern_type == TypeId::ANY || pattern_type == TypeId::ERROR {
            return true;
        }

        // Resolve lazy types (type aliases) before checking iterability
        let resolved_type = self.resolve_lazy_type(pattern_type);

        // TypeScript allows empty array destructuring patterns on most types
        // (including null/undefined), but still reports on `unknown`.
        //
        // Track whether this is an assignment target (`[a] = value`) vs a binding pattern
        // (`let [a] = value`) so ES5-specific TS2461 can stay scoped to declarations.
        let mut is_assignment_array_target = false;
        if let Some(pattern_node) = self.ctx.arena.get(pattern_idx) {
            is_assignment_array_target =
                pattern_node.kind == tsz_parser::parser::syntax_kind_ext::ARRAY_LITERAL_EXPRESSION;
        }

        if resolved_type == TypeId::UNKNOWN {
            // tsc emits TS2571 ("Object is of type 'unknown'") for empty array
            // binding patterns (`const [] = f()`), together with TS2488.
            // For non-empty patterns (`const [a, b] = f()`), only TS2488 is emitted.
            // For catch clause destructuring, tsc does NOT emit TS2571.
            let ts2571_span = if !self.is_binding_pattern_in_catch_clause(pattern_idx)
                && let Some(pattern_node) = self.ctx.arena.get(pattern_idx)
                && let Some(binding_pattern) = self.ctx.arena.get_binding_pattern(pattern_node)
                && binding_pattern.elements.nodes.is_empty()
            {
                self.get_node_span(pattern_idx)
            } else {
                None
            };

            // tsc reports TS2488 before TS2571 for this path.
            self.emit_ts2488_not_iterable(pattern_type, pattern_idx, is_assignment_array_target);
            if let Some((start, end)) = ts2571_span {
                self.error(
                    start,
                    end.saturating_sub(start),
                    "Object is of type 'unknown'.".to_string(),
                    diagnostic_codes::OBJECT_IS_OF_TYPE_UNKNOWN,
                );
            }
            return false;
        }

        // In array destructuring, TypeScript still reports TS2488 for `never`.
        if resolved_type == TypeId::NEVER {
            self.emit_ts2488_not_iterable(pattern_type, pattern_idx, is_assignment_array_target);
            return false;
        }

        // In ES5 mode (without downlevelIteration), array destructuring requires actual arrays.
        // - Emit TS2802 if the type has Symbol.iterator (iterable but requires ES2015/downlevelIteration).
        // - Emit TS2461 if the type is not an array type.
        if self.requires_array_like_iteration_for_es5_target() && !is_assignment_array_target {
            // Nested binding patterns can be fed an over-widened union from positional
            // destructuring inference (e.g. `[a, [b]] = [1, ["x"]]`). tsc does not report
            // TS2461 for these cases.
            if init_expr.is_none()
                && union_members_for_type(self.ctx.types, resolved_type).is_some_and(|members| {
                    members
                        .iter()
                        .any(|&member| self.is_array_or_tuple_type(member))
                })
            {
                return true;
            }
            if self.is_array_or_tuple_type(resolved_type)
                || self.es5_constrained_generic_is_array_like(resolved_type)
            {
                return true;
            }
            // Destructuring never uses the "or a string type" variant (allows_strings = false).
            self.emit_es5_not_iterable_error(resolved_type, pattern_type, pattern_idx, false);
            return false;
        }

        // Check if the type is iterable (ES2015+)
        if self.is_iterable_type(resolved_type) {
            // Check that undefined is assignable to the iterator's TNext (TS2765).
            // Array destructuring always sends undefined to next().
            self.check_iterator_next_type_assignability(
                resolved_type,
                TypeId::UNDEFINED,
                pattern_idx,
                IterationUseKind::Destructuring,
            );
            return true;
        }

        // Nested binding patterns can be fed an over-widened union from positional
        // destructuring inference (e.g. `var [, , [, b, ]] = [3,5,[0, 1]]`).
        // The array `[3,5,[0, 1]]` is inferred as `(number | number[])[]` instead of
        // a tuple, so the inner pattern receives `number | number[]`. tsc uses contextual
        // typing to infer a tuple, but until we do the same, suppress the false TS2488
        // when the union contains at least one array/tuple/string member.
        // NOTE: We use a side-effect-free check (classify_full_iterable_type) instead of
        // is_iterable_type to avoid polluting checker state via property access resolution.
        if init_expr.is_none()
            && union_members_for_type(self.ctx.types, resolved_type).is_some_and(|members| {
                members.iter().any(|&member| {
                    matches!(
                        classify_full_iterable_type(self.ctx.types, member),
                        FullIterableTypeKind::Array(_)
                            | FullIterableTypeKind::Tuple(_)
                            | FullIterableTypeKind::StringLiteral(_)
                    )
                })
            })
        {
            return true;
        }

        // ES2015+ destructuring requires actual Symbol.iterator support. A
        // numeric index signature alone (e.g. `interface F { [idx: number]: boolean }`)
        // is not enough — tsc emits TS2488 for those types in es2015+ mode. Only
        // ES5 with `downlevelIteration=false` reads the numeric index signature
        // path, and that case is already handled above when `target.is_es5()`.

        // Conditional/IndexAccess/Application types containing free type parameters
        // cannot have iterability proven at the generic boundary — tsc defers TS2488
        // to instantiation time for these deferred generic types. For a rest binding
        // pattern typed by e.g. `{} extends T ? [a?: string] : [a: string]`, both
        // branches are tuples, so no error is ever produced. Mirrors the guard in
        // `check_spread_argument_iterability`.
        if (common::is_conditional_type(self.ctx.types, resolved_type)
            || common::is_index_access_type(self.ctx.types, resolved_type)
            || common::is_generic_type(self.ctx.types, resolved_type))
            && common::contains_free_type_parameters(self.ctx.types, resolved_type)
        {
            return true;
        }

        // Not iterable - emit TS2488
        self.emit_ts2488_not_iterable(pattern_type, pattern_idx, is_assignment_array_target);
        false
    }

    // =========================================================================
    // Shared Diagnostic Helpers
    // =========================================================================

    /// Check if a binding pattern is the direct child of a catch clause variable declaration.
    ///
    /// Used to suppress TS2571 for catch clause array destructuring: tsc only emits
    /// TS2488 (not iterable) for `catch ([ x ]) {}`, not TS2571 (is of type 'unknown').
    fn is_binding_pattern_in_catch_clause(&self, pattern_idx: NodeIndex) -> bool {
        // binding pattern → variable declaration → catch clause
        let Some(pattern_ext) = self.ctx.arena.get_extended(pattern_idx) else {
            return false;
        };
        let var_decl_idx = pattern_ext.parent;
        let Some(var_decl_ext) = self.ctx.arena.get_extended(var_decl_idx) else {
            return false;
        };
        let catch_idx = var_decl_ext.parent;
        let Some(catch_node) = self.ctx.arena.get(catch_idx) else {
            return false;
        };
        catch_node.kind == tsz_parser::parser::syntax_kind_ext::CATCH_CLAUSE
    }

    /// Check that the iterator protocol's `next()` method returns a type with a `value` property.
    ///
    /// This follows the chain: `type[Symbol.iterator]()` -> iterator -> `.next()` -> check `.value`
    /// If `next()` returns a type without `value`, emits TS2490 and returns `false`.
    /// Returns `true` if the protocol is valid or if we can't resolve the chain
    /// (in which case we don't want to emit a false positive).
    fn check_iterator_next_returns_value(
        &mut self,
        iterable_type: TypeId,
        error_node: NodeIndex,
    ) -> bool {
        use crate::query_boundaries::common::PropertyAccessResult;

        // Skip for primitive/built-in types that are always valid iterables
        if iterable_type == TypeId::ANY
            || iterable_type == TypeId::UNKNOWN
            || iterable_type == TypeId::ERROR
            || iterable_type == TypeId::STRING
        {
            return true;
        }

        // Skip for arrays and tuples - they are built-in iterables with correct protocol.
        // This avoids false positives from property resolution issues with lib generic types.
        if self.is_array_or_tuple_or_string(iterable_type) {
            return true;
        }

        // Step 1: Get [Symbol.iterator] property
        let iterator_fn = self.resolve_property_access_with_env(iterable_type, "[Symbol.iterator]");
        let iterator_fn_type = match &iterator_fn {
            PropertyAccessResult::Success { type_id, .. } => *type_id,
            _ => return true, // Can't resolve - don't emit false positive
        };

        // Step 2: Get the return type of calling [Symbol.iterator]()
        let iterator_type = callable_return_type(self.ctx.types, iterator_fn_type);
        if iterator_type == TypeId::ANY
            || iterator_type == TypeId::UNKNOWN
            || iterator_type == TypeId::ERROR
        {
            return true;
        }

        // Handle ThisType - substitute with the iterable type itself
        let iterator_type = if is_this_type(self.ctx.types, iterator_type) {
            iterable_type
        } else {
            iterator_type
        };

        // Step 3: Get .next() on the iterator
        let next_result = self.resolve_property_access_with_env(iterator_type, "next");
        let next_fn_type = match &next_result {
            PropertyAccessResult::Success { type_id, .. } => *type_id,
            _ => return true, // Can't resolve - don't emit false positive
        };

        // If next() resolves to any, try fallback on original iterable
        let next_fn_type = if next_fn_type == TypeId::ANY && iterator_type != iterable_type {
            let fallback_next = self.resolve_property_access_with_env(iterable_type, "next");
            match &fallback_next {
                PropertyAccessResult::Success { type_id, .. } if *type_id != TypeId::ANY => {
                    *type_id
                }
                _ => return true,
            }
        } else {
            next_fn_type
        };

        // Step 4: Get the return type of next()
        let next_return = callable_return_type(self.ctx.types, next_fn_type);
        if next_return == TypeId::ANY
            || next_return == TypeId::UNKNOWN
            || next_return == TypeId::ERROR
        {
            return true;
        }

        // Step 5: Check if next()'s return type has a 'value' property
        let value_result = self.resolve_property_access_with_env(next_return, "value");
        match &value_result {
            PropertyAccessResult::Success { .. } => true, // Has 'value' - protocol is valid
            _ => {
                // No 'value' property on next()'s return type - emit TS2490
                if let Some((start, end)) = self.get_node_span(error_node) {
                    let message = format_message(
                        diagnostic_messages::THE_TYPE_RETURNED_BY_THE_METHOD_OF_AN_ITERATOR_MUST_HAVE_A_VALUE_PROPERTY,
                        &["next"],
                    );
                    self.error(
                        start,
                        end.saturating_sub(start),
                        message,
                        diagnostic_codes::THE_TYPE_RETURNED_BY_THE_METHOD_OF_AN_ITERATOR_MUST_HAVE_A_VALUE_PROPERTY,
                    );
                }
                false
            }
        }
    }

    /// Check that the iterator's `return` property (if present) is a callable method.
    ///
    /// This checks whether the iterator type (obtained via the iterable protocol) has
    /// a `return` property, and if so, whether that property is a method. If `return`
    /// exists but is not callable, emits TS2767.
    ///
    /// Uses a two-phase approach:
    /// 1. Try to find the `return` property in the object shape (direct structural check).
    /// 2. Fall back to the property access chain for types without a direct shape.
    fn check_iterator_return_is_method(&mut self, iterable_type: TypeId, error_node: NodeIndex) {
        // Skip for primitive/built-in types that are always valid iterables
        if iterable_type == TypeId::ANY
            || iterable_type == TypeId::UNKNOWN
            || iterable_type == TypeId::ERROR
            || iterable_type == TypeId::STRING
        {
            return;
        }

        // Get the iterator type via the iterable protocol chain.
        // First, determine what type the [Symbol.iterator]() method returns.
        // If it returns `this`, the iterator IS the iterable itself.
        let iterator_type = self.resolve_iterator_type_for_return_check(iterable_type);
        if iterator_type == TypeId::ANY
            || iterator_type == TypeId::UNKNOWN
            || iterator_type == TypeId::ERROR
        {
            return;
        }

        // Check iterator members for a non-method `return` property.
        // We check both the resolved iterator type AND the original iterable type,
        // since for classes that `return this`, the iterator type may be either.
        let types_to_check = if iterator_type != iterable_type {
            vec![iterator_type, iterable_type]
        } else {
            vec![iterator_type]
        };

        for check_type in types_to_check {
            if self.check_return_property_on_type(check_type, error_node) {
                return; // Found and checked the return property
            }
        }
    }

    /// Resolve the iterator type from an iterable for the TS2767 return-method check.
    fn resolve_iterator_type_for_return_check(&mut self, iterable_type: TypeId) -> TypeId {
        use crate::query_boundaries::common::PropertyAccessResult;

        let iterator_fn = self.resolve_property_access_with_env(iterable_type, "[Symbol.iterator]");
        let iterator_fn_type = match &iterator_fn {
            PropertyAccessResult::Success { type_id, .. } => *type_id,
            _ => return TypeId::ANY,
        };

        let iterator_type = callable_return_type(self.ctx.types, iterator_fn_type);
        if is_this_type(self.ctx.types, iterator_type) {
            iterable_type
        } else {
            iterator_type
        }
    }

    /// Check if a type has a `return` property that is NOT a method.
    /// Returns true if the property was found and checked (either valid or error emitted).
    /// Returns false if the property wasn't found (should try next candidate type).
    fn check_return_property_on_type(&mut self, type_id: TypeId, error_node: NodeIndex) -> bool {
        // Only check via object shape — this is the reliable path for class instances
        // where the return property is declared directly (e.g., `return = 0`).
        // Property access can return confusing results for built-in iterator types
        // where `return` is a valid method but not recognized as callable by our queries.
        match iterator_return_property_status(self.ctx.types, type_id) {
            IteratorReturnPropertyStatus::Absent => false,
            IteratorReturnPropertyStatus::Valid => true,
            IteratorReturnPropertyStatus::NeedsResolvedCallability(prop_type) => {
                let resolved = self.resolve_lazy_type(prop_type);
                if resolved != prop_type && callable_type_is_callable(self.ctx.types, resolved) {
                    return true;
                }
                self.emit_ts2767_return_not_method(error_node);
                true
            }
        }
    }

    /// Emit TS2767: "The 'return' property of an iterator must be a method."
    fn emit_ts2767_return_not_method(&mut self, error_node: NodeIndex) {
        if let Some((start, end)) = self.get_node_span(error_node) {
            let message = format_message(
                diagnostic_messages::THE_PROPERTY_OF_AN_ITERATOR_MUST_BE_A_METHOD,
                &["return"],
            );
            self.error(
                start,
                end.saturating_sub(start),
                message,
                diagnostic_codes::THE_PROPERTY_OF_AN_ITERATOR_MUST_BE_A_METHOD,
            );
        }
    }

    /// The type-id to display for an iterability diagnostic whose operand is
    /// `operand_node`. `tsc` renders the operand's own *unwidened* checked type,
    /// so a fresh primitive-literal operand (`for (… of 42)`, `yield* -5`,
    /// `[...123n]`, `(true)`, `42 as const`) shows its literal type (`42`, `-5`,
    /// `123n`, `true`) rather than the widened base (`number`, `bigint`,
    /// `boolean`). Falls back to `widened` for every non-literal operand
    /// (variables, unions such as `0 ? 1 : 2`, calls), which tsz already renders
    /// in agreement with `tsc`. This mirrors the ES2015+ spread path, which
    /// already preserves the literal through `literal_type_from_initializer`.
    pub(crate) fn iterand_display_type(&self, operand_node: NodeIndex, widened: TypeId) -> TypeId {
        self.literal_type_from_initializer(operand_node)
            .unwrap_or(widened)
    }

    /// Emit TS2488: "Type '...' must have a '[Symbol.iterator]()' method that returns an iterator."
    ///
    /// Shared by `check_for_of_iterability`, `check_spread_iterability`, and
    /// `check_destructuring_iterability` for non-iterable types in ES2015+ mode.
    fn emit_ts2488_not_iterable(
        &mut self,
        type_id: TypeId,
        error_node: NodeIndex,
        is_assignment_target: bool,
    ) {
        if let Some((start, end)) = self.get_node_span(error_node) {
            // tsc renders the operand's own unwidened type, so a fresh
            // primitive-literal operand keeps its literal (`42`, not `number`).
            // Recovered from the operand node; `None` for every non-literal
            // (including binding-pattern nodes), which fall through to the widened
            // display below. See `iterand_display_type` for the shared rationale.
            let literal_display_type = self.literal_type_from_initializer(error_node);
            let evaluated_type = self.evaluate_type_for_assignability(type_id);
            // tsc preserves boolean literals in TS2488 messages for assignment
            // targets (where variables are already declared with types), but
            // widens them in variable declarations:
            //   `[a, b] = { 0: "", 1: true }`  → `{ 0: string; 1: true; }`
            //   `var [a, b] = { 0: "", 1: true }` → `{ 0: string; 1: boolean; }`
            let display_type = if is_assignment_target {
                self.restore_boolean_display_properties(evaluated_type, type_id)
            } else {
                evaluated_type
            };
            let type_str = if let Some(literal_display_type) = literal_display_type {
                self.format_type(literal_display_type)
            } else {
                self.format_type_diagnostic_widened(display_type)
            };
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
    }

    /// Emit the appropriate ES5 non-iterable error:
    /// - TS2802 if the type has `[Symbol.iterator]` (iterable but needs downlevelIteration)
    /// - TS2461 if the type is not an array type (when `allows_strings` is false, or for
    ///   spread/destructuring)
    /// - TS2495 if the type is not an array type or a string type (when `allows_strings` is true,
    ///   only used in for-of)
    fn emit_es5_not_iterable_error(
        &mut self,
        resolved_type: TypeId,
        display_type: TypeId,
        error_node: NodeIndex,
        allows_strings: bool,
    ) {
        if let Some((start, end)) = self.get_node_span(error_node) {
            // Preserve a literal operand unwidened in the ES5 messages
            // (TS2495/TS2461); non-literal operands fall back to the widened
            // display. See `iterand_display_type`.
            let display_id = self.iterand_display_type(error_node, display_type);
            let type_str = self.format_type(display_id);
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
            } else if allows_strings {
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
                let message =
                    format_message(diagnostic_messages::TYPE_IS_NOT_AN_ARRAY_TYPE, &[&type_str]);
                self.error(
                    start,
                    end.saturating_sub(start),
                    message,
                    diagnostic_codes::TYPE_IS_NOT_AN_ARRAY_TYPE,
                );
            }
        }
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

    // =========================================================================
    // Iterator Next Type Compatibility Checking (TS2763-2766)
    // =========================================================================

    /// Check that the iterator's `next()` parameter type is compatible with what
    /// will be sent to it during iteration.
    ///
    /// For for-of, spread, and destructuring, the sent type is always `undefined`.
    /// For `yield*`, the sent type is the containing generator's `TNext`.
    ///
    /// If incompatible, emits:
    /// - TS2763 for for-of
    /// - TS2764 for array spread
    /// - TS2765 for array destructuring
    /// - TS2766 for yield* delegation
    ///
    /// Returns `true` if compatible or if we can't determine (to avoid false positives).
    pub fn check_iterator_next_type_assignability(
        &mut self,
        iterable_type: TypeId,
        sent_type: TypeId,
        error_node: NodeIndex,
        use_kind: IterationUseKind,
    ) -> bool {
        // Skip for types that can't have meaningful next type checks
        if iterable_type == TypeId::ANY
            || iterable_type == TypeId::UNKNOWN
            || iterable_type == TypeId::ERROR
            || iterable_type == TypeId::STRING
        {
            return true;
        }

        // Try to extract TNext from the Generator/AsyncGenerator/Iterator type directly
        let next_type = self.get_generator_next_type_argument(iterable_type);

        let next_type = match next_type {
            Some(t) => t,
            None => return true, // Can't determine - don't emit false positive
        };

        // If either side is any/unknown, or the iterator accepts undefined, avoid
        // a false positive. `yield*` commonly delegates from generators whose
        // containing TNext is explicitly `unknown`.
        if sent_type == TypeId::ANY || sent_type == TypeId::UNKNOWN {
            return true;
        }

        // If TNext is any, unknown, or undefined, the sent type is always compatible
        if next_type == TypeId::ANY
            || next_type == TypeId::UNKNOWN
            || next_type == TypeId::UNDEFINED
            || common::is_type_parameter_like(self.ctx.types, next_type)
            || common::contains_free_type_parameters(self.ctx.types, next_type)
        {
            return true;
        }

        // A generic or inference-bearing TNext cannot be compared reliably from
        // the declaration alone. Defer rather than reporting TS2763-TS2766 false
        // positives before instantiation supplies the concrete sent type.
        if crate::query_boundaries::common::contains_type_parameters(self.ctx.types, next_type)
            || crate::query_boundaries::common::contains_infer_types(self.ctx.types, next_type)
        {
            return true;
        }

        // Check if the sent type is assignable to the iterator's next type.
        if self.call_arg_relation_outcome(sent_type, next_type).related {
            return true;
        }

        // Not assignable - emit the appropriate diagnostic
        let sent_str = self.format_type(sent_type);
        let next_str = self.format_type(next_type);

        let (message_template, code) = match use_kind {
            IterationUseKind::ForOf => (
                diagnostic_messages::CANNOT_ITERATE_VALUE_BECAUSE_THE_NEXT_METHOD_OF_ITS_ITERATOR_EXPECTS_TYPE_BUT_FO,
                diagnostic_codes::CANNOT_ITERATE_VALUE_BECAUSE_THE_NEXT_METHOD_OF_ITS_ITERATOR_EXPECTS_TYPE_BUT_FO,
            ),
            IterationUseKind::Spread => (
                diagnostic_messages::CANNOT_ITERATE_VALUE_BECAUSE_THE_NEXT_METHOD_OF_ITS_ITERATOR_EXPECTS_TYPE_BUT_AR,
                diagnostic_codes::CANNOT_ITERATE_VALUE_BECAUSE_THE_NEXT_METHOD_OF_ITS_ITERATOR_EXPECTS_TYPE_BUT_AR,
            ),
            IterationUseKind::Destructuring => (
                diagnostic_messages::CANNOT_ITERATE_VALUE_BECAUSE_THE_NEXT_METHOD_OF_ITS_ITERATOR_EXPECTS_TYPE_BUT_AR_2,
                diagnostic_codes::CANNOT_ITERATE_VALUE_BECAUSE_THE_NEXT_METHOD_OF_ITS_ITERATOR_EXPECTS_TYPE_BUT_AR_2,
            ),
            IterationUseKind::YieldStar => (
                diagnostic_messages::CANNOT_DELEGATE_ITERATION_TO_VALUE_BECAUSE_THE_NEXT_METHOD_OF_ITS_ITERATOR_EXPEC,
                diagnostic_codes::CANNOT_DELEGATE_ITERATION_TO_VALUE_BECAUSE_THE_NEXT_METHOD_OF_ITS_ITERATOR_EXPEC,
            ),
        };

        let message = format_message(message_template, &[&sent_str, &next_str]);
        if let Some((start, end)) = self.get_node_span(error_node) {
            self.error(start, end.saturating_sub(start), message, code);
        }

        false
    }
}

/// The kind of iteration use, determining which diagnostic to emit
/// when the iterator's `next()` parameter type is incompatible.
pub enum IterationUseKind {
    /// `for (... of expr)` - emits TS2763
    ForOf,
    /// `[...expr]` - emits TS2764
    Spread,
    /// `let [x] = expr` or `[x] = expr` - emits TS2765
    Destructuring,
    /// `yield* expr` - emits TS2766
    YieldStar,
}
