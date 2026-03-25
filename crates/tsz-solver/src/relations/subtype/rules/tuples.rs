//! Tuple and array type subtype checking.
//!
//! This module handles subtyping for TypeScript's sequence types:
//! - Tuples: `[number, string, boolean]`
//! - Arrays: `number[]`, `Array<number>`
//! - Variadic tuples: `[number, ...string[]]`
//! - Tuple rest elements and expansion
//! - Array-to-tuple and tuple-to-array compatibility

use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};
use crate::operations::iterators::get_iterator_info;
use crate::types::{TupleElement, TupleListId, TypeData, TypeId};
use crate::utils::{self, TupleRestExpansion};
use crate::visitor::{array_element_type, is_type_parameter, tuple_list_id};

use super::super::{SubtypeChecker, SubtypeResult, TypeResolver};

impl<'a, R: TypeResolver> SubtypeChecker<'a, R> {
    /// Check tuple subtyping.
    ///
    /// Validates structural compatibility between tuple types, handling:
    /// - Required element count matching (source must have ≥ required elements than target)
    /// - Fixed element type compatibility (positional checking)
    /// - Rest element handling (variadic tuples, e.g., [...string[]])
    /// - Optional element compatibility
    /// - Closed tuple constraints (source can't exceed target's length)
    ///
    /// ## Tuple Subtyping Rules:
    /// 1. **Required elements**: Source must have at least as many required (non-optional) elements
    /// 2. **Rest elements**: When target has a rest element, source must match the expanded pattern
    /// 3. **Closed tuples**: If target has no rest, source can't have extra elements
    /// 4. **Type compatibility**: Each element type must be a subtype of the corresponding target
    ///
    /// ## Examples:
    /// - `[number, string]` ≤ `[number, string, boolean]` ✅
    /// - `[number, ...string[]]` ≤ `[number, ...string[]]` ✅
    /// - `[number, string]` ≤ `[number]` ❌ (extra element)
    /// - `[number]` ≤ `[number, string]` ❌ (missing element)
    pub(crate) fn check_tuple_subtype(
        &mut self,
        source: &[TupleElement],
        target: &[TupleElement],
    ) -> SubtypeResult {
        // Fast path: [...S] <: [...T] when both are single-rest-element tuples.
        // tsc treats these as equivalent to S <: T for assignability.
        // This handles variadic tuple identity: [...U] <: [...T] when U extends T.
        if source.len() == 1
            && target.len() == 1
            && source[0].rest
            && target[0].rest
            && is_type_parameter(self.interner, source[0].type_id)
            && is_type_parameter(self.interner, target[0].type_id)
        {
            return self.check_subtype(source[0].type_id, target[0].type_id);
        }

        // Count required elements
        let source_required = crate::utils::required_element_count(source);
        let target_required = crate::utils::required_element_count(target);

        // Source must have at least as many required elements
        if source_required < target_required {
            return SubtypeResult::False;
        }

        // Check each element
        for (i, t_elem) in target.iter().enumerate() {
            if t_elem.rest {
                let expansion = self.expand_tuple_rest(t_elem.type_id);
                let outer_tail = &target[i + 1..];
                // Combined suffix = expansion.tail + outer_tail
                // We need to match these from the end of the source tuple
                let combined_suffix: Vec<_> = expansion
                    .tail
                    .iter()
                    .chain(outer_tail.iter())
                    .cloned()
                    .collect();

                let mut source_end = source.len();
                for tail_elem in combined_suffix.iter().rev() {
                    if source_end <= i {
                        if !tail_elem.optional {
                            return SubtypeResult::False;
                        }
                        break;
                    }
                    // If the tail element is a rest spread of a type parameter
                    // (e.g., ...P from [...T, ...P]), a concrete source element
                    // cannot satisfy it — we need a matching rest in the source.
                    if tail_elem.rest && is_type_parameter(self.interner, tail_elem.type_id) {
                        // Look for a matching rest element from the source end
                        let s_elem = &source[source_end - 1];
                        if s_elem.rest {
                            // Source rest element must be subtype of the target
                            // type parameter's array form
                            let tp_array = self.interner.array(tail_elem.type_id);
                            if !self.check_subtype(s_elem.type_id, tp_array).is_true() {
                                return SubtypeResult::False;
                            }
                            source_end -= 1;
                            continue;
                        }
                        // No source rest element — source can't match this variadic
                        return SubtypeResult::False;
                    }
                    let s_elem = &source[source_end - 1];
                    if s_elem.rest {
                        if !tail_elem.optional {
                            return SubtypeResult::False;
                        }
                        break;
                    }
                    let assignable = self
                        .check_subtype(s_elem.type_id, tail_elem.type_id)
                        .is_true();
                    if tail_elem.optional && !assignable {
                        break;
                    }
                    if !assignable {
                        return SubtypeResult::False;
                    }
                    source_end -= 1;
                }

                let mut source_iter = source.iter().enumerate().take(source_end).skip(i);

                for t_fixed in &expansion.fixed {
                    match source_iter.next() {
                        Some((_, s_elem)) => {
                            if s_elem.rest {
                                return SubtypeResult::False;
                            }
                            if !self
                                .check_subtype(s_elem.type_id, t_fixed.type_id)
                                .is_true()
                            {
                                return SubtypeResult::False;
                            }
                        }
                        None => {
                            if !t_fixed.optional {
                                return SubtypeResult::False;
                            }
                        }
                    }
                }

                if let Some(variadic) = expansion.variadic {
                    // When the variadic element is a type parameter (e.g., ...T where
                    // T extends any[]), concrete source elements cannot match — only
                    // a source rest element can satisfy the type parameter spread.
                    // TSC: "Source provides no match for variadic element at position N
                    //        in target."
                    let variadic_is_type_param = is_type_parameter(self.interner, variadic);
                    let variadic_array = self.interner.array(variadic);
                    for (_, s_elem) in source_iter {
                        if s_elem.rest {
                            // When both source and target rest elements are type parameters,
                            // compare them directly (U <: T) rather than via Array(T).
                            // This handles [...U] <: [...T] when U extends T.
                            if variadic_is_type_param
                                && is_type_parameter(self.interner, s_elem.type_id)
                            {
                                if !self.check_subtype(s_elem.type_id, variadic).is_true() {
                                    return SubtypeResult::False;
                                }
                            } else if !self.check_subtype(s_elem.type_id, variadic_array).is_true()
                            {
                                return SubtypeResult::False;
                            }
                        } else if variadic_is_type_param {
                            // Concrete element cannot match a type parameter variadic
                            return SubtypeResult::False;
                        } else if !self.check_subtype(s_elem.type_id, variadic).is_true() {
                            return SubtypeResult::False;
                        }
                    }
                    return SubtypeResult::True;
                }

                if source_iter.next().is_some() {
                    return SubtypeResult::False;
                }
                return SubtypeResult::True;
            }

            // Target is not rest
            if let Some(s_elem) = source.get(i) {
                if s_elem.rest {
                    // Source has rest but target expects fixed element -> Mismatch
                    // e.g. Target: [number, number], Source: [number, ...number[]]
                    return SubtypeResult::False;
                }

                if !self.check_subtype(s_elem.type_id, t_elem.type_id).is_true() {
                    return SubtypeResult::False;
                }
            } else if !t_elem.optional {
                // Missing required element
                return SubtypeResult::False;
            }
        }

        // If we reached here, target has NO rest element (it is closed).
        // Ensure source has no extra elements.

        // 1. Source length check: Source cannot have more elements than Target
        if source.len() > target.len() {
            return SubtypeResult::False;
        }

        // 2. Source open check: Source cannot have a rest element if Target is closed
        for s_elem in source {
            if s_elem.rest {
                return SubtypeResult::False;
            }
        }

        SubtypeResult::True
    }

    /// Check if an array type is a subtype of a tuple type.
    ///
    /// TypeScript semantics: Arrays (T[]) are generally NOT assignable to tuple types,
    /// even variadic tuples like [...T[]], because tuples have specific structural
    /// constraints that arrays don't satisfy.
    ///
    /// The ONLY exception is `never[]` which represents an empty array and can be
    /// assigned to any tuple that allows empty (has no required elements).
    ///
    /// Note: `any[]` is NOT assignable to tuples — only `any` itself bypasses
    /// structural checks. `Array<any>.length` is `number`, which is not
    /// assignable to a tuple's literal length type.
    ///
    /// ## Cases:
    /// - `any[]` -> `[string, number]` : No (array, not tuple)
    /// - `never[]` -> `[]` : Yes (empty array to empty tuple)
    /// - `never[]` -> `[string?]` : Yes (empty array to optional-only tuple)
    /// - `never[]` -> `[...string[]]` : Yes (empty array to variadic tuple)
    /// - `never[]` -> `[string]` : No (empty array cannot satisfy required element)
    /// - `string[]` -> `[...string[]]` : No (arrays are not assignable to tuples)
    /// - `string[]` -> `[string?]` : No (arrays are not assignable to tuples)
    pub(crate) fn check_array_to_tuple_subtype(
        &mut self,
        source_elem: TypeId,
        target: &[TupleElement],
    ) -> SubtypeResult {
        // Only never[] can potentially be assigned to tuples (represents empty array)
        // Note: any[] is NOT assignable to tuples in tsc. While each element access
        // on any[] returns any, the structural comparison fails because Array<any>.length
        // (type number) is not assignable to a tuple's literal length type (e.g., 2).
        // The any TYPE (not any[]) is already handled earlier in the subtype check
        // and bypasses all structural checks.
        if source_elem != TypeId::NEVER {
            return SubtypeResult::False;
        }

        // never[] can be assigned to a tuple if and only if the tuple allows empty
        if self.tuple_allows_empty(target) {
            SubtypeResult::True
        } else {
            SubtypeResult::False
        }
    }

    /// Check if a tuple type allows empty arrays.
    ///
    /// Determines whether `never[]` (empty array) can be assigned to a tuple type.
    /// A tuple allows empty if ALL of its elements are optional or it has a rest element
    /// with no required trailing elements.
    ///
    /// ## Examples:
    /// - `[]` ✅ - Empty tuple allows empty array
    /// - `[string?]` ✅ - Only optional element
    /// - `[string]` ❌ - Required element
    /// - `[...string[]]` ✅ - Rest element allows any number including zero
    /// - `[...string[], number]` ❌ - Required trailing element after rest
    ///
    /// ## Nested Tuple Spreads:
    /// When a rest element contains a nested tuple spread, we recursively check
    /// both the fixed elements and tail elements of the expansion.
    pub(crate) fn tuple_allows_empty(&self, target: &[TupleElement]) -> bool {
        for (index, elem) in target.iter().enumerate() {
            if elem.rest {
                // Check if there are any REQUIRED elements after the rest element
                // e.g., [...string[], number] has a required trailing element
                // but [...string[], number?] only has optional trailing elements
                let tail = &target[index + 1..];
                if tail.iter().any(|tail_elem| !tail_elem.optional) {
                    return false;
                }

                // Check the expanded rest element for required fixed elements
                let expansion = self.expand_tuple_rest(elem.type_id);
                if expansion.fixed.iter().any(|fixed| !fixed.optional) {
                    return false;
                }

                // Check tail elements from nested tuple spreads
                if expansion.tail.iter().any(|tail_elem| !tail_elem.optional) {
                    return false;
                }

                // Tuple with rest element allows empty if:
                // 1. No required trailing elements after the rest
                // 2. The rest expansion has no required fixed elements
                // 3. The expansion has no required tail elements
                return true;
            }

            if !elem.optional {
                return false;
            }
        }

        true
    }

    /// Check if a tuple type is a subtype of an array type.
    ///
    /// Tuple is subtype of array if all tuple elements are subtypes of the array element type.
    /// Handles both regular elements and rest elements (with expansion).
    ///
    /// ## Examples:
    /// - `[number, number]` <: `number[]` ✅
    /// - `[number, string]` <: `number[]` ❌ (string is not subtype of number)
    /// - `[number, ...string[]]` <: `(number | string)[]` ✅
    pub(crate) fn check_tuple_to_array_subtype(
        &mut self,
        elems: TupleListId,
        t_elem: TypeId,
    ) -> SubtypeResult {
        let elems = self.interner.tuple_list(elems);
        for elem in elems.iter() {
            if elem.rest {
                let expansion = self.expand_tuple_rest(elem.type_id);
                for fixed in expansion.fixed {
                    if !self.check_subtype(fixed.type_id, t_elem).is_true() {
                        return SubtypeResult::False;
                    }
                }
                if let Some(variadic) = expansion.variadic
                    && !self.check_subtype(variadic, t_elem).is_true()
                {
                    return SubtypeResult::False;
                }
                // Check tail elements from nested tuple spreads
                for tail_elem in expansion.tail {
                    if !self.check_subtype(tail_elem.type_id, t_elem).is_true() {
                        return SubtypeResult::False;
                    }
                }
            } else {
                // Regular element: T <: U
                if !self.check_subtype(elem.type_id, t_elem).is_true() {
                    return SubtypeResult::False;
                }
            }
        }
        SubtypeResult::True
    }

    /// Expand a tuple rest element into its constituent parts.
    ///
    /// Tuples can have rest elements like `[A, B, ...C[]]` which need to be expanded
    /// for subtype checking. This function recursively expands rest elements to produce:
    /// - `fixed`: Elements before the rest
    /// - `variadic`: The rest element's type (e.g., C for ...C[])
    /// - `tail`: Elements after the rest (rare, but valid in some TypeScript patterns)
    ///
    /// ## Examples:
    pub(crate) fn expand_tuple_rest(&self, type_id: TypeId) -> TupleRestExpansion {
        utils::expand_tuple_rest(self.interner, type_id)
    }

    /// Check if Array<`element_type`> (the interface) is a subtype of the target.
    ///
    /// This is analogous to `is_boxed_primitive_subtype` — when a T[] is checked
    /// against a structural type (e.g., `{ length: number; toString(): string }`),
    /// we instantiate the Array<T> interface with the concrete element type and
    /// check whether that interface type is a subtype of the target.
    ///
    /// Returns `Some(result)` if the Array interface was available and the check was
    /// performed, or `None` if the Array base type is not registered (e.g., in tests
    /// without lib.d.ts).
    pub(crate) fn check_array_interface_subtype(
        &mut self,
        element_type: TypeId,
        target: TypeId,
    ) -> Option<SubtypeResult> {
        let array_base = self.resolver.get_array_base_type()?;
        let params = self.resolver.get_array_base_type_params();
        let instantiated = if params.is_empty() {
            array_base
        } else {
            // Instantiate Array<T> → Array<element_type>
            let subst = TypeSubstitution::from_args(self.interner, params, &[element_type]);
            instantiate_type(self.interner, array_base, &subst)
        };

        let direct = self.check_subtype(instantiated, target);
        if direct.is_true() {
            return Some(direct);
        }

        // Fallback: iterator protocol compatibility.
        // If target is iterable and source array elements are assignable to the
        // yielded type, accept the assignment.
        let Some(query_db) = self.query_db else {
            // No query_db — try direct iterable yield type extraction
            if let Some(yield_type) = self.extract_iterable_yield_type(target) {
                return Some(self.check_subtype(element_type, yield_type));
            }
            return Some(direct);
        };
        if let Some(iter_info) = get_iterator_info(query_db, target, false) {
            return Some(self.check_subtype(element_type, iter_info.yield_type));
        }

        // get_iterator_info failed (e.g., can't resolve `next()` on an Application
        // type like Iterator<T>). Fall back to extracting the yield type directly
        // from the target's [Symbol.iterator] return type arguments.
        if let Some(yield_type) = self.extract_iterable_yield_type(target) {
            return Some(self.check_subtype(element_type, yield_type));
        }

        Some(direct)
    }

    /// Extract the yield type from an Iterable-like target type.
    ///
    /// When the target is an object with a `[Symbol.iterator]` method whose return
    /// type is a generic Application (e.g., `Iterator<T, TReturn, TNext>`), extract
    /// the first type argument as the yield type.
    ///
    /// This is used as a fallback when full iterator protocol resolution fails
    /// (e.g., because `next()` can't be resolved on an unexpanded Application type).
    fn extract_iterable_yield_type(&self, target: TypeId) -> Option<TypeId> {
        use crate::visitor::{
            application_id, callable_shape_id, object_shape_id, object_with_index_shape_id,
        };

        // Get the target's object shape (either Object or ObjectWithIndex)
        let shape_id = object_shape_id(self.interner, target)
            .or_else(|| object_with_index_shape_id(self.interner, target))?;
        let shape = self.interner.object_shape(shape_id);

        // Find [Symbol.iterator] property
        let sym_iter_atom = self.interner.intern_string("[Symbol.iterator]");
        let iter_prop = shape
            .properties
            .binary_search_by_key(&sym_iter_atom, |p| p.name)
            .ok()
            .map(|idx| &shape.properties[idx])?;

        // Get the Callable shape to extract the return type
        let callable_id = callable_shape_id(self.interner, iter_prop.type_id)?;
        let callable = self.interner.callable_shape(callable_id);

        // Get the first call signature's return type
        let return_type = callable.call_signatures.first()?.return_type;

        // If the return type is an Application (e.g., Iterator<T, TReturn, TNext>),
        // the first type argument is the yield type
        let app_id = application_id(self.interner, return_type)?;
        let app = self.interner.type_application(app_id);

        // The yield type is the first type argument
        app.args.first().copied()
    }

    /// Get the element type of an array type, or return the type itself for any[].
    ///
    /// Used for extracting the element type when checking rest parameters.
    /// For tuples used as rest parameters (e.g., [...args: [any]]), extracts the first element's type.
    pub(crate) fn get_array_element_type(&self, type_id: TypeId) -> TypeId {
        if type_id == TypeId::ANY {
            return TypeId::ANY;
        }

        if let Some(TypeData::ReadonlyType(inner)) = self.interner.lookup(type_id) {
            return self.get_array_element_type(inner);
        }

        // First try array element type
        if let Some(elem) = array_element_type(self.interner, type_id) {
            return elem;
        }

        // Handle generic array applications like Array<T> / ReadonlyArray<T>
        // which are represented as TypeData::Application with a single type arg.
        if let Some(TypeData::Application(app_id)) = self.interner.lookup(type_id) {
            let app = self.interner.type_application(app_id);
            if let Some(&first_arg) = app.args.first() {
                return first_arg;
            }
        }

        // For tuples used as rest parameters, extract the first element's type
        // This handles cases like [...args: [any]] being compatible with [...args: any[]]
        if let Some(list_id) = tuple_list_id(self.interner, type_id) {
            let elements = self.interner.tuple_list(list_id);
            if let Some(first) = elements.first() {
                return first.type_id;
            }
        }

        type_id
    }
}
