//! Variadic tuple inference for `InferenceContext`.
//!
//! Faithful port of tsc's `inferFromTupleTypes` (checker.ts). tsc aligns the
//! leading fixed elements (the "start"), the trailing fixed elements (the
//! "end"), and then distributes the remaining "middle" source elements among
//! the target's variadic/rest elements. This module reproduces that algorithm
//! so `infer_tuples` preserves rest length and element positions the same way
//! `tsc` does across aliases, conditional wrappers, and call signatures:
//!
//! - `[H, ...Tail]` — fixed prefix, trailing variadic/rest
//! - `[...Init, L]` — leading variadic/rest, fixed suffix
//! - `[H, ...Mid, L]` — fixed prefix, single variadic/rest, fixed suffix
//! - `[...A, ...B]` — two adjacent variadic elements (distributed by the
//!   implied arity of `A`, or `A`'s constraint when `B` is a plain rest array)
//! - Concrete source tuple against a concrete array-typed rest element
//!
//! tsc distinguishes `Variadic` (`...T` for a generic/tuple `T`) from `Rest`
//! (`...E[]`, an array spread). tsz collapses both into `TupleElement::rest`,
//! so the distinction is recovered structurally: a `rest` element whose type is
//! array-like is a `Rest`, anything else is a `Variadic`.

use crate::types::{InferencePriority, TupleElement, TupleListId, TypeData, TypeId};

use super::infer::{InferenceContext, InferenceError};

impl InferenceContext<'_> {
    /// Infer from tuple types, handling variadic (rest) elements.
    pub(super) fn infer_tuples(
        &mut self,
        source_elems: TupleListId,
        target_elems: TupleListId,
        priority: InferencePriority,
    ) -> Result<(), InferenceError> {
        // Copy the elements (they are `Copy`) so we can call `&mut self` inference
        // methods without holding a borrow of the interner's tuple storage.
        let source: Vec<TupleElement> = self.interner.tuple_list(source_elems).to_vec();
        let target: Vec<TupleElement> = self.interner.tuple_list(target_elems).to_vec();

        let source_arity = source.len();
        let target_arity = target.len();
        let target_has_rest = target.iter().any(|e| e.rest);

        // Same structure (same arity and matching variable/fixed positions): infer
        // element-wise. Mirrors tsc's `isTupleTypeStructureMatching` fast path.
        if source_arity == target_arity
            && source
                .iter()
                .zip(target.iter())
                .all(|(s, t)| s.rest == t.rest)
        {
            for (s, t) in source.iter().zip(target.iter()) {
                self.infer_from_types(s.type_id, t.type_id, priority)?;
            }
            return Ok(());
        }

        // Leading fixed count (tsc `fixedLength`): elements before the first
        // variadic/rest. Trailing fixed count (tsc `getEndElementCount(_, Fixed)`):
        // elements after the last variadic/rest.
        let source_fixed = source.iter().position(|e| e.rest).unwrap_or(source_arity);
        let target_fixed = target.iter().position(|e| e.rest).unwrap_or(target_arity);
        let start_length = source_fixed.min(target_fixed);

        let source_end_fixed = source.iter().rev().take_while(|e| !e.rest).count();
        let target_end_fixed = target.iter().rev().take_while(|e| !e.rest).count();
        let end_length = source_end_fixed.min(if target_has_rest { target_end_fixed } else { 0 });

        // Infer between the leading fixed elements.
        for i in 0..start_length {
            self.infer_from_types(source[i].type_id, target[i].type_id, priority)?;
        }

        // When a single source `Rest` (array spread) covers the entire middle,
        // spread its element type over every target middle element. A `Variadic`
        // target element receives the element type wrapped back in an array.
        let single_source_rest_inner = (source_arity - start_length - end_length == 1
            && source[start_length].rest)
            .then(|| self.tuple_rest_array_inner(source[start_length].type_id))
            .flatten();

        if let Some(rest_inner) = single_source_rest_inner {
            for t in &target[start_length..target_arity - end_length] {
                let inferred = if self.tuple_element_is_variadic(*t) {
                    self.interner.array(rest_inner)
                } else {
                    rest_inner
                };
                self.infer_from_types(inferred, t.type_id, priority)?;
            }
        } else {
            let middle_length = target_arity - start_length - end_length;
            if middle_length == 2 {
                self.infer_tuple_middle_pair(
                    &source,
                    &target,
                    start_length,
                    end_length,
                    target_end_fixed,
                    priority,
                )?;
            } else if middle_length == 1 {
                self.infer_tuple_middle_single(
                    &source,
                    &target,
                    start_length,
                    end_length,
                    priority,
                )?;
            }
        }

        // Infer between the trailing fixed elements.
        for i in 0..end_length {
            self.infer_from_types(
                source[source_arity - i - 1].type_id,
                target[target_arity - i - 1].type_id,
                priority,
            )?;
        }

        Ok(())
    }

    /// Infer a function's remaining source parameters against a tuple-typed rest
    /// parameter (`...args: [...T, ...U]`, `[H, ...T]`, …). The source params are
    /// packed into a tuple and inferred in the normal direction so the type
    /// parameters inside the target tuple receive candidates with correct arity.
    pub(crate) fn infer_source_params_against_rest_tuple(
        &mut self,
        source_params: &[crate::types::ParamInfo],
        target_tuple: TypeId,
        priority: InferencePriority,
    ) -> Result<(), InferenceError> {
        let elements: Vec<TupleElement> = source_params
            .iter()
            .map(|p| TupleElement {
                type_id: p.type_id,
                name: p.name,
                optional: p.optional,
                rest: p.rest,
            })
            .collect();
        let source_tuple = self.interner.tuple(elements);
        self.infer_from_types(source_tuple, target_tuple, priority)
    }

    /// Single variadic/rest target element in the middle.
    fn infer_tuple_middle_single(
        &mut self,
        source: &[TupleElement],
        target: &[TupleElement],
        start_length: usize,
        end_length: usize,
        priority: InferencePriority,
    ) -> Result<(), InferenceError> {
        let mid = target[start_length];
        if !mid.rest {
            return Ok(());
        }
        let end = source.len() - end_length;
        let slice = &source[start_length..end];

        if let Some(inner) = self.tuple_rest_array_inner(mid.type_id) {
            // `...E[]`: infer the element type of the source middle slice against `E`.
            if let Some(rest_type) = self.element_type_of_slice(slice) {
                self.infer_from_types(rest_type, inner, priority)?;
            }
        } else if slice.len() == 1 && slice[0].rest {
            // A single source rest element maps directly to the variadic target,
            // avoiding a spurious `[...X]` wrapper (so `[...U]` infers `T = U`).
            self.infer_from_types(slice[0].type_id, mid.type_id, priority)?;
        } else {
            // `...T`: infer the source middle slice (as a tuple) against `T`.
            let slice_tuple = self.interner.tuple(slice.to_vec());
            self.infer_from_types(slice_tuple, mid.type_id, priority)?;
        }
        Ok(())
    }

    /// Two adjacent variadic/rest target elements in the middle. The split point
    /// is determined by an implied arity, mirroring tsc exactly:
    /// - `(variadic, variadic)`: the first element's recorded implied arity.
    /// - `(variadic, rest)`: the fixed arity of the first element's constraint.
    /// - `(rest, variadic)`: the fixed arity of the second element's constraint.
    ///
    /// When no implied arity is available, neither element is inferred (the type
    /// parameters fall back to their constraints), matching tsc.
    fn infer_tuple_middle_pair(
        &mut self,
        source: &[TupleElement],
        target: &[TupleElement],
        start_length: usize,
        end_length: usize,
        target_end_fixed: usize,
        priority: InferencePriority,
    ) -> Result<(), InferenceError> {
        let source_arity = source.len();
        let first = target[start_length];
        let second = target[start_length + 1];
        let first_variadic = self.tuple_element_is_variadic(first);
        let second_variadic = self.tuple_element_is_variadic(second);
        let first_rest = first.rest && !first_variadic;
        let second_rest = second.rest && !second_variadic;

        if first_variadic && second_variadic {
            let Some(implied_arity) = self.implied_arity_for_type(first.type_id) else {
                return Ok(());
            };
            // tsc: sliceTupleType(source, startLength, endLength + sourceArity - impliedArity)
            let end_skip = end_length + source_arity.saturating_sub(implied_arity);
            let first_slice = self.slice_tuple(source, start_length, end_skip);
            self.infer_from_types(first_slice, first.type_id, priority)?;
            let second_slice = self.slice_tuple(source, start_length + implied_arity, end_length);
            self.infer_from_types(second_slice, second.type_id, priority)?;
        } else if first_variadic && second_rest {
            let Some(implied_arity) = self.constraint_fixed_arity_for_type(first.type_id) else {
                return Ok(());
            };
            // tsc: sliceTupleType(source, startLength, sourceArity - (startLength + impliedArity))
            let end_skip = source_arity.saturating_sub(start_length + implied_arity);
            let first_slice = self.slice_tuple(source, start_length, end_skip);
            self.infer_from_types(first_slice, first.type_id, priority)?;
            let lo = start_length + implied_arity;
            let hi = source_arity.saturating_sub(end_length);
            self.infer_rest_array_from_slice(second.type_id, source, lo, hi, priority)?;
        } else if first_rest && second_variadic {
            let Some(implied_arity) = self.constraint_fixed_arity_for_type(second.type_id) else {
                return Ok(());
            };
            let end_index = source_arity.saturating_sub(target_end_fixed);
            let start_index = end_index.saturating_sub(implied_arity);
            let hi = source_arity.saturating_sub(end_length + implied_arity);
            self.infer_rest_array_from_slice(first.type_id, source, start_length, hi, priority)?;
            if start_index <= end_index {
                let trailing = self.interner.tuple(source[start_index..end_index].to_vec());
                self.infer_from_types(trailing, second.type_id, priority)?;
            }
        }
        Ok(())
    }

    /// If `rest_elem` is an array-like `Rest` element (`...E[]`), infer the
    /// element type of the source slice `source[lo..hi]` against its element type
    /// `E`. A no-op when `rest_elem` is not array-like or the slice is empty.
    fn infer_rest_array_from_slice(
        &mut self,
        rest_elem: TypeId,
        source: &[TupleElement],
        lo: usize,
        hi: usize,
        priority: InferencePriority,
    ) -> Result<(), InferenceError> {
        let hi = hi.min(source.len());
        let lo = lo.min(hi);
        if let Some(inner) = self.tuple_rest_array_inner(rest_elem)
            && let Some(rest_type) = self.element_type_of_slice(&source[lo..hi])
        {
            self.infer_from_types(rest_type, inner, priority)?;
        }
        Ok(())
    }

    /// Build a tuple `TypeId` from `elements[index .. len - end_skip]`,
    /// preserving each element's flags. Empty when the range is degenerate.
    fn slice_tuple(&self, elements: &[TupleElement], index: usize, end_skip: usize) -> TypeId {
        let len = elements.len();
        let end = len.saturating_sub(end_skip);
        let slice = if index < end {
            &elements[index..end]
        } else {
            &[]
        };
        self.interner.tuple(slice.to_vec())
    }

    /// Union of the element types of a tuple slice (tsc
    /// `getElementTypeOfSliceOfTupleType`). A `Variadic` element contributes its
    /// number-indexed element type; a `Rest` (array spread) contributes the array
    /// element type; a fixed element contributes its type. `None` for an empty
    /// slice.
    fn element_type_of_slice(&self, slice: &[TupleElement]) -> Option<TypeId> {
        if slice.is_empty() {
            return None;
        }
        let element_types: Vec<TypeId> = slice
            .iter()
            .map(|elem| {
                if elem.rest {
                    self.tuple_rest_array_inner(elem.type_id)
                        .unwrap_or_else(|| self.interner.index_access(elem.type_id, TypeId::NUMBER))
                } else {
                    elem.type_id
                }
            })
            .collect();
        Some(self.interner.union(element_types))
    }

    /// A `Variadic` element (`...T`, where `T` is a generic/tuple): a rest
    /// element whose type is not array-like. Array-like rest elements (`...E[]`)
    /// are tsc's `Rest` flavor instead.
    fn tuple_element_is_variadic(&self, elem: TupleElement) -> bool {
        elem.rest && self.tuple_rest_array_inner(elem.type_id).is_none()
    }

    /// Element type of an array-like rest element type (`E[]`, `readonly E[]`, or
    /// a single-argument generic application). `None` for non-array-like types,
    /// which marks the element as a `Variadic` (`...T`) rather than a `Rest`.
    fn tuple_rest_array_inner(&self, rest_type: TypeId) -> Option<TypeId> {
        match self.interner.lookup(rest_type) {
            Some(TypeData::Array(elem)) => Some(elem),
            Some(TypeData::ReadonlyType(inner)) => match self.interner.lookup(inner) {
                Some(TypeData::Array(elem)) => Some(elem),
                _ => None,
            },
            Some(TypeData::Application(app_id)) => {
                let app = self.interner.type_application(app_id);
                (app.args.len() == 1).then(|| app.args[0])
            }
            _ => None,
        }
    }
}
