//! Tuple-element contextual typing.
//!
//! Split from `core.rs` to keep that file under the 2000-line cap. Holds
//! [`ContextualTypeContext`]'s tuple-element extraction methods and their
//! homomorphic-mapped-per-index support helpers.

use crate::construction::TypeDatabase;
use crate::contextual::ContextualTypeContext;
use crate::contextual::extractors::{
    TupleElementExtractor, collect_single_or_union, collect_single_or_union_preserve,
};
use crate::types::{TypeData, TypeId};

impl<'a> ContextualTypeContext<'a> {
    /// Get the contextual type for a specific tuple element.
    pub fn get_tuple_element_type(&self, index: usize) -> Option<TypeId> {
        self.get_tuple_element_type_inner(index, None, false)
    }

    /// Get the contextual type for a tuple element, with knowledge of the total element count.
    /// This enables correct mapping for variadic tuple types like `[...T[], U]`.
    pub fn get_tuple_element_type_with_count(
        &self,
        index: usize,
        element_count: usize,
    ) -> Option<TypeId> {
        self.get_tuple_element_type_inner(index, Some(element_count), false)
    }

    /// Like [`Self::get_tuple_element_type_with_count`], but for a *present*
    /// element value under `exactOptionalPropertyTypes`: an optional
    /// element's own declared type rather than the read-side type with
    /// `undefined` unioned in. Mirrors
    /// [`Self::get_property_assignment_type`] for tuples.
    pub fn get_tuple_element_assignment_type_with_count(
        &self,
        index: usize,
        element_count: usize,
    ) -> Option<TypeId> {
        self.get_tuple_element_type_inner(index, Some(element_count), true)
    }

    fn get_tuple_element_type_inner(
        &self,
        index: usize,
        element_count: Option<usize>,
        strip_optional_undefined: bool,
    ) -> Option<TypeId> {
        let expected = self.expected?;

        // `readonly` is transparent for element extraction: `readonly [A, B]`
        // contextually types its elements exactly like `[A, B]`.
        if let Some(TypeData::ReadonlyType(inner)) = self.interner.lookup(expected) {
            let ctx = ContextualTypeContext::with_expected(self.interner, inner);
            return ctx.get_tuple_element_type_inner(
                index,
                element_count,
                strip_optional_undefined,
            );
        }

        // Handle Union explicitly - collect tuple element types from all members,
        // preserving literal arms (see `collect_single_or_union_preserve`) so a
        // fresh literal element keyed by `number | 2` is not widened to `number`.
        if let Some(TypeData::Union(members)) = self.interner.lookup(expected) {
            let members = self.interner.type_list(members);
            let elem_types: Vec<TypeId> = members
                .iter()
                .filter_map(|&m| {
                    let ctx = ContextualTypeContext::with_expected(self.interner, m);
                    ctx.get_tuple_element_type_inner(index, element_count, strip_optional_undefined)
                })
                .collect();
            return collect_single_or_union_preserve(self.interner, elem_types);
        }

        // Handle Intersection explicitly - collect tuple element types from all members
        // and intersect them. This ensures that when the contextual type is an intersection
        // of mapped types like `Results<T> & Errors<E>`, the element contextual type
        // includes properties from ALL members, enabling contextual typing of callbacks
        // in every intersection member.
        if let Some(TypeData::Intersection(members)) = self.interner.lookup(expected) {
            let members = self.interner.type_list(members);
            let elem_types: Vec<TypeId> = members
                .iter()
                .filter_map(|&m| {
                    let ctx = ContextualTypeContext::with_expected(self.interner, m);
                    ctx.get_tuple_element_type_inner(index, element_count, strip_optional_undefined)
                })
                .collect();
            return match elem_types.len() {
                0 => None,
                1 => Some(elem_types[0]),
                _ => Some(self.interner.intersection(elem_types)),
            };
        }

        // Handle Application explicitly - evaluate to resolve type aliases
        if let Some(TypeData::Application(_)) = self.interner.lookup(expected) {
            let evaluated = crate::evaluation::evaluate::evaluate_type(self.interner, expected);
            if evaluated != expected {
                let ctx = ContextualTypeContext::with_expected(self.interner, evaluated);
                return ctx.get_tuple_element_type_inner(
                    index,
                    element_count,
                    strip_optional_undefined,
                );
            }
        }

        // Handle TypeParameter - use its constraint
        if let Some(constraint) =
            crate::type_queries::get_type_parameter_constraint(self.interner, expected)
        {
            let ctx = ContextualTypeContext::with_expected(self.interner, constraint);
            return ctx.get_tuple_element_type_inner(
                index,
                element_count,
                strip_optional_undefined,
            );
        }

        // Handle Mapped, Conditional, and Lazy types by evaluating them first.
        // PERF: Single lookup for guard + Conditional extraction.
        if let Some(expected_key) = self.interner.lookup(expected)
            && matches!(
                expected_key,
                TypeData::Mapped(_) | TypeData::Conditional(_) | TypeData::Lazy(_)
            )
        {
            // Deferred mapped: substitute K with the index literal before
            // evaluation so same-name source/key collisions inside nested
            // templates preserve the source object instead of letting generic
            // evaluation rewrite both sides by name.
            if let TypeData::Mapped(mapped_id) = expected_key
                && let Some(per_index) =
                    try_mapped_per_index_template(self.interner, mapped_id, index)
            {
                return Some(per_index);
            }

            if let TypeData::Conditional(cond_id) = expected_key {
                let cond = self.interner.get_conditional(cond_id);
                let mut branch_elem_types = Vec::with_capacity(2);
                for branch in [cond.true_type, cond.false_type] {
                    // Guard against self-recursive aliases.
                    if branch == expected {
                        continue;
                    }
                    let ctx = ContextualTypeContext::with_expected(self.interner, branch);
                    if let Some(ty) = ctx.get_tuple_element_type_inner(
                        index,
                        element_count,
                        strip_optional_undefined,
                    ) {
                        branch_elem_types.push(ty);
                    }
                }
                if let Some(resolved) = collect_single_or_union(self.interner, branch_elem_types) {
                    return Some(resolved);
                }
            }
            let evaluated = crate::evaluation::evaluate::evaluate_type(self.interner, expected);
            if evaluated != expected {
                let ctx = ContextualTypeContext::with_expected(self.interner, evaluated);
                return ctx.get_tuple_element_type_inner(
                    index,
                    element_count,
                    strip_optional_undefined,
                );
            }
        }

        let mut extractor = if strip_optional_undefined {
            TupleElementExtractor::new_for_assignment(self.interner, index, element_count)
        } else {
            TupleElementExtractor::new(self.interner, index, element_count)
        };
        extractor.extract(expected)
    }
}

/// Substitute K with the index literal in a homomorphic mapped type's
/// template, recovering per-element contextual info when evaluation cannot
/// reduce the mapped to a concrete tuple (e.g., source X is still generic).
/// Refuses when key remapping or constraint shape would misalign positional
/// indices with the mapped's key domain.
fn try_mapped_per_index_template(
    db: &dyn TypeDatabase,
    mapped_id: crate::types::MappedTypeId,
    index: usize,
) -> Option<TypeId> {
    let mapped = db.mapped_type(mapped_id);

    if !crate::type_queries::is_identity_name_mapping(db, &mapped) {
        return None;
    }
    if !constraint_iterates_positional_keys(db, mapped.constraint) {
        return None;
    }
    if !crate::type_queries::template_references_iter_param(
        db,
        mapped.template,
        mapped.type_param.name,
    ) {
        return None;
    }
    if template_has_nested_same_name_source_key_collision(
        db,
        mapped.template,
        mapped.type_param.name,
    ) {
        return None;
    }

    let key_literal = db.literal_number(index as f64);
    Some(
        crate::type_queries::instantiate_mapped_template_for_property(
            db,
            mapped.template,
            mapped.type_param.name,
            key_literal,
        ),
    )
}

/// Per-index contextual typing substitutes by the mapped key name. A direct
/// `T[K]` template has a structural fast path in
/// `instantiate_mapped_template_for_property`, but nested shapes such as
/// `(v: P[P]) => void` can otherwise replace an outer source `P` as well as
/// the mapped key `P`. Refuse those nested collisions so callers fall back to
/// the existing non-positional contextual path instead of producing a wrong
/// per-element type.
fn template_has_nested_same_name_source_key_collision(
    db: &dyn TypeDatabase,
    template: TypeId,
    iter_name: tsz_common::Atom,
) -> bool {
    if template.is_intrinsic() {
        return false;
    }
    if matches!(db.lookup(template), Some(TypeData::IndexAccess(_, _))) {
        return false;
    }

    crate::contains_type_matching(db, template, |key| match key {
        TypeData::IndexAccess(object, index) => {
            crate::contains_type_parameter_named_shallow(db, *object, iter_name)
                && crate::contains_type_parameter_named_shallow(db, *index, iter_name)
        }
        _ => false,
    })
}

/// Whether the mapped's iteration domain includes positional numeric keys —
/// `keyof X`, the `number` intrinsic, or an intersection of those. Intersections
/// are canonicalized/flattened so a single level of recursion is sufficient.
fn constraint_iterates_positional_keys(db: &dyn TypeDatabase, constraint: TypeId) -> bool {
    if constraint == TypeId::NUMBER {
        return true;
    }
    if constraint.is_intrinsic() {
        return false;
    }
    match db.lookup(constraint) {
        Some(TypeData::KeyOf(_)) => true,
        Some(TypeData::Intersection(members)) => db
            .type_list(members)
            .iter()
            .any(|&m| constraint_iterates_positional_keys(db, m)),
        _ => false,
    }
}
