//! Iterable/iterator protocol checking and for-of element type computation.

use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
use crate::query_boundaries::checkers::iterable::{
    AsyncIterableTypeKind, ForOfElementKind, FullIterableTypeKind, call_signatures_for_type,
    classify_async_iterable_type, classify_for_of_element_type, classify_full_iterable_type,
    function_shape_for_type, is_array_type, is_string_literal_type, is_string_type, is_this_type,
    is_tuple_type, union_members_for_type,
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
                // Check if object has a [Symbol.iterator] method in its shape.
                // If found (Some), verify the full iterator protocol (the return
                // value of [Symbol.iterator]() must have a `next()` method).
                // If not found (None), fall back to property access resolution for
                // computed properties from lib types or inherited properties.
                match self.object_has_iterator_method(shape_id) {
                    Some(true) => {
                        // [Symbol.iterator] exists and is callable, but we must also
                        // verify that calling it returns a valid iterator (has next()).
                        // Use the full property access chain to verify.
                        self.type_has_symbol_iterator_via_property_access(type_id)
                    }
                    Some(false) => false,
                    None => self.type_has_symbol_iterator_via_property_access(type_id),
                }
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
    /// Returns `Some(true)` if found and valid, `Some(false)` if found but invalid
    /// (optional or has required params), `None` if not found in the shape.
    fn object_has_iterator_method(&self, shape_id: tsz_solver::ObjectShapeId) -> Option<bool> {
        let shape = self.ctx.types.object_shape(shape_id);

        // Check for [Symbol.iterator] method (iterable protocol)
        for prop in &shape.properties {
            let prop_name = self.ctx.types.resolve_atom_ref(prop.name);
            if prop_name.as_ref() == "[Symbol.iterator]" {
                // Optional [Symbol.iterator] is not a valid iterable
                if prop.optional {
                    return Some(false);
                }
                if prop.is_method {
                    // Must be callable with zero arguments
                    return Some(self.is_callable_with_no_required_args(prop.type_id));
                }
                // Non-method properties: check if their type is callable.
                // This handles `declare [Symbol.iterator]: this["entries"]`
                // where the property type resolves to a callable function type.
                // If the type is callable, return true. If not directly
                // resolvable (e.g., complex type expression), return None
                // to fall through to the full property access resolution.
                if prop.type_id == TypeId::ANY {
                    return Some(true);
                }
                if self.is_callable_with_no_required_args(prop.type_id) {
                    return Some(true);
                }
                // Can't determine callability from the shape alone — let
                // the full property access resolution handle it.
                return None;
            }
        }

        None
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
        let iterator_type = self.get_call_return_type(iterator_fn_type);

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
        self.type_has_next_method(iterator_type)
            || (iterator_type != iterable_type && self.type_has_next_method(iterable_type))
    }

    /// Check if a type has a `next` method by examining its object shape directly.
    ///
    /// This is more precise than `resolve_property_access_with_env` because it
    /// doesn't fall back to `any` for missing properties. Used to verify the
    /// iterator protocol: the return value of `[Symbol.iterator]()` must have `next()`.
    fn type_has_next_method(&self, type_id: TypeId) -> bool {
        // For any/unknown/error, accept
        if type_id == TypeId::ANY || type_id == TypeId::UNKNOWN || type_id == TypeId::ERROR {
            return true;
        }

        let kind = classify_full_iterable_type(self.ctx.types, type_id);
        match kind {
            FullIterableTypeKind::Object(shape_id) => {
                let shape = self.ctx.types.object_shape(shape_id);
                shape.properties.iter().any(|prop| {
                    let name = self.ctx.types.resolve_atom_ref(prop.name);
                    name.as_ref() == "next"
                })
            }
            FullIterableTypeKind::Union(members) => {
                // All union members must have next()
                members.iter().all(|&m| self.type_has_next_method(m))
            }
            FullIterableTypeKind::Intersection(members) => {
                // At least one intersection member must have next()
                members.iter().any(|&m| self.type_has_next_method(m))
            }
            FullIterableTypeKind::Readonly(inner) => self.type_has_next_method(inner),
            FullIterableTypeKind::Application { .. }
            | FullIterableTypeKind::TypeParameter { .. }
            | FullIterableTypeKind::ComplexType
            | FullIterableTypeKind::Array(_)
            | FullIterableTypeKind::Tuple(_)
            | FullIterableTypeKind::StringLiteral(_) => {
                // Application types (IterableIterator<T>, etc.), type parameters,
                // complex types, arrays, tuples, and string literals all have
                // next() via their iterator protocol or resolve to lib types.
                true
            }
            FullIterableTypeKind::FunctionOrCallable | FullIterableTypeKind::NotIterable => {
                // Functions and NotIterable (Lazy/DefId types that couldn't be resolved,
                // or truly non-iterable types) do NOT have next().
                false
            }
        }
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
                use crate::query_boundaries::common::PropertyAccessResult;
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

    /// Compute the element type produced by a `for (... of expr)` or
    /// `for await (... of expr)` loop.
    ///
    /// Handles arrays, tuples, unions, strings, and custom iterators via
    /// the `[Symbol.iterator]().next().value` protocol.
    ///
    /// When `is_async` is true (`for await...of`), the element type is awaited,
    /// so `Iterable<Promise<T>>` yields `T` instead of `Promise<T>`.
    pub fn for_of_element_type(&mut self, iterable_type: TypeId, is_async: bool) -> TypeId {
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

        if is_async {
            // For for-await-of: try async iterator protocol first (AsyncIterable<T> → T),
            // then fall back to sync iterator + Promise unwrapping (Iterable<Promise<T>> → T)
            if let Some(info) =
                tsz_solver::operations::get_iterator_info(self.ctx.types, iterable_type, true)
            {
                return info.yield_type;
            }
            // Fall back to sync iterator protocol + Promise unwrapping.
            let elem_type = self.for_of_element_type_classified(iterable_type, 0);
            if let Some(unwrapped) = self.unwrap_promise_type(elem_type) {
                return unwrapped;
            }
            // unwrap_promise_type can fail when the element type has been resolved to
            // an Object shape (e.g. Promise<number> from lib files).  Fall back to the
            // tsc approach: if the element is promise-like (has a callable `then`),
            // extract the fulfillment type from `then`'s onfulfilled callback parameter.
            if let Some(awaited) = self.get_awaited_type_of_promise_like(elem_type) {
                return awaited;
            }
            elem_type
        } else {
            self.for_of_element_type_classified(iterable_type, 0)
        }
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
            ForOfElementKind::Intersection(members) => {
                // For an intersection of iterables (e.g. X[] & Y[]),
                // the element type is the intersection of each member's element type.
                let mut element_types = Vec::with_capacity(members.len());
                for member in members {
                    element_types.push(self.for_of_element_type_classified(member, depth + 1));
                }
                factory.intersection(element_types)
            }
            ForOfElementKind::Readonly(inner) => {
                // Unwrap readonly wrapper and compute element type for inner
                self.for_of_element_type_classified(inner, depth + 1)
            }
            ForOfElementKind::String => TypeId::STRING,
            ForOfElementKind::Other => {
                // For custom iterators, Application types (Map, Set), etc.,
                // use the solver's iterator protocol resolution which properly
                // handles Application types and type parameter substitution.
                self.resolve_iterator_element_type(type_id)
            }
        }
    }

    /// Extract the fulfillment type from a promise-like type by following the
    /// `then` method's onfulfilled callback parameter.
    ///
    /// For `Promise<T>` (resolved to an Object shape from lib files), the `then`
    /// method signature is: `then(onfulfilled: (value: T) => ...) => ...`
    /// This extracts `T` by reading the first parameter type of the first
    /// call signature's first parameter.
    fn get_awaited_type_of_promise_like(&mut self, type_id: TypeId) -> Option<TypeId> {
        use tsz_solver::operations::property::PropertyAccessEvaluator;

        let evaluator = PropertyAccessEvaluator::new(self.ctx.types);
        let then_type = evaluator
            .resolve_property_access(type_id, "then")
            .success_type()?;

        // Get call signatures of `then`
        let sigs = call_signatures_for_type(self.ctx.types, then_type)?;
        let first_sig = sigs.first()?;

        // The first parameter is `onfulfilled?: ((value: T) => ...) | null | undefined`.
        // It may be a union — extract the callable member from it.
        let onfulfilled_type = first_sig.params.first().map(|p| p.type_id)?;

        // Extract the first parameter of the onfulfilled callback.
        // The callback may be:
        //   - Callable type (has call_signatures)
        //   - Function type (has params directly)
        //   - Union member (fn | null | undefined)
        self.extract_first_param_type(onfulfilled_type)
    }

    /// Extract the first parameter type from a callable/function type,
    /// handling unions of `(fn | null | undefined)`.
    fn extract_first_param_type(&self, type_id: TypeId) -> Option<TypeId> {
        // Direct Callable
        if let Some(sigs) = call_signatures_for_type(self.ctx.types, type_id) {
            return sigs.first()?.params.first().map(|p| p.type_id);
        }
        // Direct Function
        if let Some(shape) = function_shape_for_type(self.ctx.types, type_id) {
            return shape.params.first().map(|p| p.type_id);
        }
        // Union: find first callable/function member
        let members = union_members_for_type(self.ctx.types, type_id)?;
        for member in &members {
            if let Some(sigs) = call_signatures_for_type(self.ctx.types, *member)
                && let Some(first) = sigs.first()
            {
                return first.params.first().map(|p| p.type_id);
            }
            if let Some(shape) = function_shape_for_type(self.ctx.types, *member) {
                return shape.params.first().map(|p| p.type_id);
            }
        }
        None
    }

    /// Resolve the element type of an iterable via the iterator protocol.
    ///
    /// Uses a hybrid approach:
    /// 1. First tries the solver's `get_iterator_info` which properly handles
    ///    Application types (`IterableIterator`<T>, `IteratorResult`<T>).
    /// 2. Falls back to checker-level property access chain which handles
    ///    merged declarations (`IArguments`) and custom iterator classes.
    ///
    /// Returns ANY as fallback if the protocol cannot be resolved.
    fn resolve_iterator_element_type(&mut self, type_id: TypeId) -> TypeId {
        // Try solver-level iterator resolution first (handles Application types correctly)
        if let Some(info) =
            tsz_solver::operations::get_iterator_info(self.ctx.types, type_id, false)
        {
            return info.yield_type;
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
    /// We use the solver's `extract_iterator_result_value_types` to properly partition
    /// by `done` instead of naively reading `.value` (which would give T | `TReturn`).
    fn resolve_iterator_element_type_via_property_access(&mut self, type_id: TypeId) -> TypeId {
        use crate::query_boundaries::common::PropertyAccessResult;

        // Step 1: Get [Symbol.iterator] property
        let iterator_fn = self.resolve_property_access_with_env(type_id, "[Symbol.iterator]");
        let iterator_fn_type = match &iterator_fn {
            PropertyAccessResult::Success { type_id, .. } => *type_id,
            _ => return TypeId::ANY,
        };

        // Step 2: Get the return type of the iterator function (call it)
        let iterator_type = self.get_call_return_type(iterator_fn_type);

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
        let next_return = self.get_call_return_type(next_fn_type);

        // Step 5: Extract the yield type from IteratorResult.
        //
        // IteratorResult<T, TReturn> = { done?: false, value: T } | { done: true, value: TReturn }
        // For for-of loops, only the yield type T matters (from done:false branches).
        //
        // First try the solver's discriminant-aware extraction on the evaluated type.
        let resolved_result = self.ctx.types.evaluate_type(next_return);
        let (yield_type, _return_type) =
            tsz_solver::operations::extract_iterator_result_value_types(
                self.ctx.types,
                resolved_result,
            );

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
            if let Some(info) =
                tsz_solver::operations::get_iterator_info(self.ctx.types, iterator_type, false)
            {
                return info.yield_type;
            }
            return TypeId::ANY;
        }

        value_type
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

        // For async for-of, first check async iterable, then fall back to sync iterable.
        // For union types like `Iterable<T> | AsyncIterable<T>`, tsc checks each member
        // individually — each member must be EITHER async iterable OR sync iterable.
        if is_async {
            if self.is_async_iterable_type(expr_type) || self.is_iterable_type(expr_type) {
                return true;
            }
            // For unions, check if each member is individually async- or sync-iterable
            if let Some(members) = union_members_for_type(self.ctx.types, expr_type)
                && members
                    .iter()
                    .all(|&m| self.is_async_iterable_type(m) || self.is_iterable_type(m))
            {
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
            return true;
        }

        // Not iterable - emit TS2488
        self.emit_ts2488_not_iterable(expr_type, expr_idx);
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
            return true;
        }

        // Not iterable - emit TS2488
        self.emit_ts2488_not_iterable(spread_type, expr_idx);
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
            if let Some(binding_pattern) = self.ctx.arena.get_binding_pattern(pattern_node)
                && binding_pattern.elements.nodes.is_empty()
                && resolved_type != TypeId::UNKNOWN
            {
                return true;
            }
        }

        if resolved_type == TypeId::UNKNOWN {
            // tsc emits TS2571 ("Object is of type 'unknown'") before TS2488 when
            // the array binding pattern has elements (e.g. `const [a, b] = f()`).
            // For empty patterns (`const [] = f()`), only TS2488 is emitted.
            // For catch clause destructuring, tsc does NOT emit TS2571 — only TS2488.
            let is_catch_clause = self.is_binding_pattern_in_catch_clause(pattern_idx);
            if !is_catch_clause
                && let Some(pattern_node) = self.ctx.arena.get(pattern_idx)
                && let Some(binding_pattern) = self.ctx.arena.get_binding_pattern(pattern_node)
                && !binding_pattern.elements.nodes.is_empty()
                && let Some((start, end)) = self.get_node_span(pattern_idx)
            {
                self.error(
                    start,
                    end.saturating_sub(start),
                    "Object is of type 'unknown'.".to_string(),
                    diagnostic_codes::OBJECT_IS_OF_TYPE_UNKNOWN,
                );
            }
            self.emit_ts2488_not_iterable(pattern_type, pattern_idx);
            return false;
        }

        // In array destructuring, TypeScript still reports TS2488 for `never`.
        if resolved_type == TypeId::NEVER {
            self.emit_ts2488_not_iterable(pattern_type, pattern_idx);
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
                && union_members_for_type(self.ctx.types, resolved_type).is_some_and(|members| {
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
            // Destructuring never uses the "or a string type" variant (allows_strings = false).
            self.emit_es5_not_iterable_error(resolved_type, pattern_type, pattern_idx, false);
            return false;
        }

        // Check if the type is iterable (ES2015+)
        if self.is_iterable_type(resolved_type) {
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

        // TypeScript also allows array destructuring for "array-like" types
        // (types with numeric index signatures) even without [Symbol.iterator]()
        if self.has_numeric_index_signature(resolved_type) {
            return true;
        }

        // Not iterable - emit TS2488
        self.emit_ts2488_not_iterable(pattern_type, pattern_idx);
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

        // Step 1: Get [Symbol.iterator] property
        let iterator_fn = self.resolve_property_access_with_env(iterable_type, "[Symbol.iterator]");
        let iterator_fn_type = match &iterator_fn {
            PropertyAccessResult::Success { type_id, .. } => *type_id,
            _ => return true, // Can't resolve - don't emit false positive
        };

        // Step 2: Get the return type of calling [Symbol.iterator]()
        let iterator_type = self.get_call_return_type(iterator_fn_type);
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
        let next_return = self.get_call_return_type(next_fn_type);
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

        let iterator_type = self.get_call_return_type(iterator_fn_type);
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
        use crate::query_boundaries::common::PropertyAccessResult;

        // Use property access to find the `return` property
        let return_result = self.resolve_property_access_with_env(type_id, "return");
        let return_type = match &return_result {
            PropertyAccessResult::Success { type_id, .. } => *type_id,
            PropertyAccessResult::PossiblyNullOrUndefined {
                property_type: Some(t),
                ..
            } => *t,
            _ => return false, // No `return` property found
        };

        // If property access returns any/unknown/error, we can't determine callability
        if return_type == TypeId::ANY
            || return_type == TypeId::UNKNOWN
            || return_type == TypeId::ERROR
        {
            return false;
        }

        // Check if the return property is callable (has function shape or call signatures)
        if function_shape_for_type(self.ctx.types, return_type).is_some() {
            return true; // Callable - valid
        }
        if let Some(sigs) = call_signatures_for_type(self.ctx.types, return_type)
            && !sigs.is_empty()
        {
            return true; // Callable - valid
        }

        // Check if the type is a number/string/boolean literal — definitely not callable
        let resolved = self.resolve_lazy_type(return_type);
        if function_shape_for_type(self.ctx.types, resolved).is_some() {
            return true;
        }

        // `return` exists but is not callable - emit TS2767
        self.emit_ts2767_return_not_method(error_node);
        true
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

    /// Emit TS2488: "Type '...' must have a '[Symbol.iterator]()' method that returns an iterator."
    ///
    /// Shared by `check_for_of_iterability`, `check_spread_iterability`, and
    /// `check_destructuring_iterability` for non-iterable types in ES2015+ mode.
    fn emit_ts2488_not_iterable(&mut self, type_id: TypeId, error_node: NodeIndex) {
        if let Some((start, end)) = self.get_node_span(error_node) {
            let type_str = self.format_type(type_id);
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
            let type_str = self.format_type(display_type);
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
}
