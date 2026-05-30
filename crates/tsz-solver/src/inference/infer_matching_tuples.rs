//! Variadic tuple inference for `InferenceContext`.
//!
//! tsc's `inferFromTupleTypes` aligns fixed elements from the front (prefix)
//! and the back (suffix), then collects the remaining "middle" source elements
//! into a tuple and infers it against the target rest parameter. This module
//! implements that algorithm so `infer_tuples` handles all variadic cases:
//!
//! - `[H, ...Tail]` — fixed prefix, trailing rest
//! - `[...Init, L]` — leading rest, fixed suffix
//! - `[H, ...Mid, L]` — fixed prefix, rest, fixed suffix
//! - Both source and target contain rest elements
//! - Concrete source tuple against concrete array-typed rest element

use crate::types::{InferencePriority, TupleListId, TypeData, TypeId};

use super::infer::{InferenceContext, InferenceError};

impl<'a> InferenceContext<'a> {
    /// Infer from tuple types, handling variadic (rest) elements.
    ///
    /// Structural rule: given source `[s₀,…,sₙ]` and target `[t₀…tₖ, ...R, tₘ…tₙ]`,
    /// `R` is inferred from the tuple `[sₖ₊₁,…,sₙ₋ₘ]` (the middle slice).
    pub(super) fn infer_tuples(
        &mut self,
        source_elems: TupleListId,
        target_elems: TupleListId,
        priority: InferencePriority,
    ) -> Result<(), InferenceError> {
        let source_list = self.interner.tuple_list(source_elems);
        let target_list = self.interner.tuple_list(target_elems);

        // Find the first rest-element index in each side.
        let source_rest_idx = source_list.iter().position(|e| e.rest);
        let target_rest_idx = target_list.iter().position(|e| e.rest);

        // Neither side has a rest element — simple pairwise zip.
        if source_rest_idx.is_none() && target_rest_idx.is_none() {
            for (s, t) in source_list.iter().zip(target_list.iter()) {
                self.infer_from_types(s.type_id, t.type_id, priority)?;
            }
            return Ok(());
        }

        // Fixed-element counts before/after the rest position.
        let source_prefix = source_rest_idx.unwrap_or(source_list.len());
        let target_prefix = target_rest_idx.unwrap_or(target_list.len());

        // How many fixed elements to align from the front.
        let prefix_count = source_prefix.min(target_prefix);

        // How many fixed elements to align from the back.
        // Variadic rest segments after the first rest are still variable-length
        // tuple parts; they must not be treated as one fixed suffix element.
        let source_trailing_fixed = source_list.iter().rev().take_while(|e| !e.rest).count();
        let target_trailing_fixed = target_list.iter().rev().take_while(|e| !e.rest).count();

        // When source has no rest, all elements not consumed by the prefix are
        // available for target's suffix. When source has a rest, only the source
        // fixed elements after the last rest position are available.
        let available_for_suffix = if source_rest_idx.is_some() {
            source_trailing_fixed
        } else {
            source_list.len().saturating_sub(prefix_count)
        };
        let suffix_count = available_for_suffix.min(target_trailing_fixed);

        if let Some(target_rest_pos) = target_rest_idx
            && let Some(next_rest_offset) = target_list[target_rest_pos + 1..]
                .iter()
                .position(|elem| elem.rest)
            && let Some(prefix_len) =
                self.fixed_tuple_candidate_len_for_type(target_list[target_rest_pos].type_id)
        {
            let next_rest_pos = target_rest_pos + 1 + next_rest_offset;
            let prefix_end = target_rest_pos.saturating_add(prefix_len);
            if prefix_end <= source_list.len() {
                for i in 0..target_rest_pos {
                    self.infer_from_types(
                        source_list[i].type_id,
                        target_list[i].type_id,
                        priority,
                    )?;
                }

                let suffix_count = source_trailing_fixed.min(
                    target_list[next_rest_pos + 1..]
                        .iter()
                        .rev()
                        .take_while(|elem| !elem.rest)
                        .count(),
                );
                let end_index = source_list.len().saturating_sub(suffix_count);
                if prefix_end <= end_index {
                    for (s, t) in source_list
                        .iter()
                        .rev()
                        .zip(target_list.iter().rev())
                        .take(suffix_count)
                    {
                        self.infer_from_types(s.type_id, t.type_id, priority)?;
                    }

                    let middle = &source_list[prefix_end..end_index];
                    let next_rest_type = target_list[next_rest_pos].type_id;
                    if middle.len() == 1 && middle[0].rest {
                        self.infer_from_types(middle[0].type_id, next_rest_type, priority)?;
                    } else {
                        let middle_tuple = self.interner.tuple(middle.to_vec());
                        self.infer_from_types(middle_tuple, next_rest_type, priority)?;
                    }
                    return Ok(());
                }
            }
        }

        if let Some(target_rest_pos) = target_rest_idx
            && target_list[target_rest_pos + 1..]
                .iter()
                .any(|elem| elem.rest)
        {
            return Ok(());
        }

        // Prefix inference.
        for i in 0..prefix_count {
            self.infer_from_types(source_list[i].type_id, target_list[i].type_id, priority)?;
        }

        // Suffix inference (working backwards from the end).
        for (s, t) in source_list
            .iter()
            .rev()
            .zip(target_list.iter().rev())
            .take(suffix_count)
        {
            self.infer_from_types(s.type_id, t.type_id, priority)?;
        }

        // Handle the variadic middle portion.
        if let Some(target_rest_pos) = target_rest_idx {
            let target_rest_type = target_list[target_rest_pos].type_id;

            let rest_start = prefix_count;
            let rest_end = source_list.len().saturating_sub(suffix_count);
            let middle = &source_list[rest_start..rest_end];

            // Single source rest element maps directly to the target rest parameter.
            if middle.len() == 1 && middle[0].rest {
                self.infer_from_types(middle[0].type_id, target_rest_type, priority)?;
                return Ok(());
            }

            let is_target_type_param = matches!(
                self.interner.lookup(target_rest_type),
                Some(TypeData::TypeParameter(_) | TypeData::Infer(_))
            );

            if is_target_type_param {
                // Collect middle source elements into a tuple and infer it
                // against the type parameter (empty tuple included — it proves
                // zero-arity and prevents the parameter defaulting to its
                // constraint, which would hide arity mismatches).
                let middle_tuple = self.interner.tuple(middle.to_vec());
                self.infer_from_types(middle_tuple, target_rest_type, priority)?;
            } else {
                // Target rest is a concrete array type (e.g. `Array<X>`).
                // Extract the element type and infer each source middle element
                // individually against it.
                let inner = self.tuple_infer_rest_elem_inner(target_rest_type);
                for elem in middle {
                    self.infer_from_types(elem.type_id, inner, priority)?;
                }
            }
        } else if source_rest_idx.is_some() {
            // Source has a rest element but target does not.
            // Infer the source rest's element type against each target fixed
            // element position that was not covered by the prefix.
            // source_prefix == source_rest_idx.unwrap() when source_rest_idx.is_some().
            let source_rest_type = source_list[source_prefix].type_id;
            let inner = self.tuple_infer_rest_elem_inner(source_rest_type);
            for i in prefix_count..target_prefix {
                self.infer_from_types(inner, target_list[i].type_id, priority)?;
            }
        }

        Ok(())
    }

    /// Extract the array element type from an array-like rest element type.
    ///
    /// For `T[]` / `Array<T>` returns `T`; for `readonly T[]` returns `T`;
    /// for a single-arg generic application returns its argument; for anything
    /// else returns the type itself so the caller falls back gracefully.
    fn tuple_infer_rest_elem_inner(&self, rest_type: TypeId) -> TypeId {
        match self.interner.lookup(rest_type) {
            Some(TypeData::Array(elem)) => elem,
            Some(TypeData::ReadonlyType(inner)) => match self.interner.lookup(inner) {
                Some(TypeData::Array(elem)) => elem,
                _ => rest_type,
            },
            Some(TypeData::Application(app_id)) => {
                let app = self.interner.type_application(app_id);
                if app.args.len() == 1 {
                    app.args[0]
                } else {
                    rest_type
                }
            }
            _ => rest_type,
        }
    }
}
