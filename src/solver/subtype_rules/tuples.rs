//! Tuple and array type subtype checking.
//!
//! This module handles subtyping for TypeScript's sequence types:
//! - Tuples: `[number, string, boolean]`
//! - Arrays: `number[]`, `Array<number>`
//! - Variadic tuples: `[number, ...string[]]`
//! - Tuple rest elements and expansion
//! - Array-to-tuple and tuple-to-array compatibility

use crate::solver::types::*;
use crate::solver::visitor::{array_element_type, tuple_list_id};

use super::super::{SubtypeChecker, SubtypeResult, TypeResolver};

/// Expansion of a tuple rest element into its constituent parts.
///
/// Used to normalize variadic tuples for subtype checking.
pub(crate) struct TupleRestExpansion {
    /// Fixed elements before the variadic portion (prefix)
    pub fixed: Vec<TupleElement>,
    /// The variadic element type (e.g., T for ...T[])
    pub variadic: Option<TypeId>,
    /// Fixed elements after the variadic portion (suffix/tail)
    pub tail: Vec<TupleElement>,
}

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
        // Count required elements
        let source_required = source.iter().filter(|e| !e.optional && !e.rest).count();
        let target_required = target.iter().filter(|e| !e.optional && !e.rest).count();

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
                    let variadic_array = self.interner.array(variadic);
                    for (_, s_elem) in source_iter {
                        if s_elem.rest {
                            if !self.check_subtype(s_elem.type_id, variadic_array).is_true() {
                                return SubtypeResult::False;
                            }
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
    /// ## Cases:
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
        // Only never[] can potentially be assigned to tuples
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
    /// - `[number, string]` → fixed: [number, string], variadic: None, tail: []
    /// - `[number, ...string[]]` → fixed: [number], variadic: Some(string), tail: []
    /// - `[...T[], number]` → fixed: [], variadic: Some(T), tail: [number]
    ///
    /// ## Recursive Expansion:
    /// Nested rest elements are recursively expanded, so:
    /// - `[A, ...[...B[], C]]` → fixed: [A], variadic: Some(B), tail: [C]
    pub(crate) fn expand_tuple_rest(&self, type_id: TypeId) -> TupleRestExpansion {
        if let Some(elem) = array_element_type(self.interner, type_id) {
            return TupleRestExpansion {
                fixed: Vec::new(),
                variadic: Some(elem),
                tail: Vec::new(),
            };
        }

        if let Some(elements) = tuple_list_id(self.interner, type_id) {
            let elements = self.interner.tuple_list(elements);
            let mut fixed = Vec::new();
            for (i, elem) in elements.iter().enumerate() {
                if elem.rest {
                    let inner = self.expand_tuple_rest(elem.type_id);
                    fixed.extend(inner.fixed);
                    // Capture tail elements: inner.tail + elements after the rest
                    let mut tail = inner.tail;
                    tail.extend(elements[i + 1..].iter().cloned());
                    return TupleRestExpansion {
                        fixed,
                        variadic: inner.variadic,
                        tail,
                    };
                }
                fixed.push(elem.clone());
            }
            return TupleRestExpansion {
                fixed,
                variadic: None,
                tail: Vec::new(),
            };
        }

        TupleRestExpansion {
            fixed: Vec::new(),
            variadic: Some(type_id),
            tail: Vec::new(),
        }
    }

    /// Get the element type of an array type, or return the type itself for any[].
    ///
    /// Used for extracting the element type when checking rest parameters.
    pub(crate) fn get_array_element_type(&self, type_id: TypeId) -> TypeId {
        if type_id == TypeId::ANY {
            return TypeId::ANY;
        }
        array_element_type(self.interner, type_id).unwrap_or(type_id)
    }
}
