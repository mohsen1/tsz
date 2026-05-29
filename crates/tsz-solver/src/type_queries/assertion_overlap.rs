//! Structural property/element overlap checks for type-assertion (`as`)
//! comparability (TS2352).
//!
//! These helpers back [`super::flow::types_are_comparable_for_assertion`]: once
//! the primitive/union/intersection fast paths have been exhausted, the object
//! and tuple/array shapes are compared structurally here. The recursion always
//! re-enters `types_are_comparable_for_assertion_inner` with `nested = true`,
//! because property and element types are not subject to tsc's top-level
//! `getWidenedType` and therefore require strict literal overlap.

use super::flow::types_are_comparable_for_assertion_inner;
use crate::construction::TypeDatabase;
use crate::types::{TypeData, TypeId};
use rustc_hash::FxHashMap;
use tsz_common::Atom;

/// Relaxed version of `types_have_common_properties` for TS2352.
/// Only requires that shared properties have comparable types.
/// Missing target properties in the source are allowed.
pub(super) fn types_have_common_properties_relaxed(
    db: &dyn TypeDatabase,
    source: TypeId,
    target: TypeId,
    depth: u32,
) -> bool {
    fn get_properties(db: &dyn TypeDatabase, type_id: TypeId) -> Vec<(Atom, TypeId, bool)> {
        if type_id.is_intrinsic() {
            return Vec::new();
        }
        match db.lookup(type_id) {
            Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
                let shape = db.object_shape(shape_id);
                shape
                    .properties
                    .iter()
                    .map(|p| (p.name, p.type_id, p.optional))
                    .collect()
            }
            Some(TypeData::Callable(callable_id)) => {
                let shape = db.callable_shape(callable_id);
                shape
                    .properties
                    .iter()
                    .map(|p| (p.name, p.type_id, p.optional))
                    .collect()
            }
            Some(TypeData::Intersection(list_id)) => {
                let members = db.type_list(list_id);
                let mut props = Vec::new();
                for &member in members.iter() {
                    props.extend(get_properties(db, member));
                }
                props
            }
            // Arrays have no named properties for overlap checking - element types
            // are compared separately in types_are_comparable_for_assertion_inner.
            // Returning empty ensures we don't short-circuit array↔object comparisons.
            _ => Vec::new(),
        }
    }

    // Handle array↔array comparability: check element types directly
    if let (Some(TypeData::Array(src_elem)), Some(TypeData::Array(tgt_elem))) =
        (db.lookup(source), db.lookup(target))
    {
        return types_are_comparable_for_assertion_inner(db, src_elem, tgt_elem, depth + 1, true);
    }

    // Handle array↔tuple comparability: array element vs any tuple element
    if let (Some(TypeData::Array(arr_elem)), Some(TypeData::Tuple(tuple_id))) =
        (db.lookup(source), db.lookup(target))
    {
        let tuple_elements = db.tuple_list(tuple_id);
        return tuple_elements.iter().any(|elem| {
            types_are_comparable_for_assertion_inner(db, arr_elem, elem.type_id, depth + 1, true)
        });
    }
    if let (Some(TypeData::Tuple(tuple_id)), Some(TypeData::Array(arr_elem))) =
        (db.lookup(source), db.lookup(target))
    {
        let tuple_elements = db.tuple_list(tuple_id);
        return tuple_elements.iter().any(|elem| {
            types_are_comparable_for_assertion_inner(db, elem.type_id, arr_elem, depth + 1, true)
        });
    }

    // Handle tuple↔tuple comparability: check element types pairwise.
    // tsc's isTypeComparableTo checks tuples structurally: each element at
    // position i must be comparable to the element at position i in the other
    // tuple. Different-length tuples are not comparable (neither is assignable
    // to the other), so TS2352 should fire.
    if let (Some(TypeData::Tuple(src_tuple)), Some(TypeData::Tuple(tgt_tuple))) =
        (db.lookup(source), db.lookup(target))
    {
        let src_elements = db.tuple_list(src_tuple);
        let tgt_elements = db.tuple_list(tgt_tuple);
        // Different-length tuples are not comparable
        if src_elements.len() != tgt_elements.len() {
            return false;
        }
        // All corresponding elements must be comparable
        return src_elements.iter().zip(tgt_elements.iter()).all(|(s, t)| {
            types_are_comparable_for_assertion_inner(db, s.type_id, t.type_id, depth + 1, true)
        });
    }

    let source_props = get_properties(db, source);
    let target_props = get_properties(db, target);

    // If both sides have no properties and aren't arrays/tuples, they don't overlap
    if source_props.is_empty() && target_props.is_empty() {
        return false;
    }

    let mut source_by_name: FxHashMap<Atom, Vec<(TypeId, bool)>> = FxHashMap::default();
    for (name, ty, optional) in &source_props {
        source_by_name
            .entry(*name)
            .or_default()
            .push((*ty, *optional));
    }

    // For TS2352: only check that shared properties are comparable.
    // Missing target properties are allowed.
    let mut found_common = false;
    for (target_name, target_ty, target_optional) in &target_props {
        if let Some(source_entries) = source_by_name.get(target_name) {
            found_common = true;
            let any_comparable = source_entries.iter().any(|(source_ty, source_optional)| {
                if (*source_optional || *target_optional)
                    && (*source_ty == TypeId::UNDEFINED || *target_ty == TypeId::UNDEFINED)
                {
                    return true;
                }
                // Property values are nested, so distinct literals (e.g. `"a"` vs
                // `"b"`) are rejected by the `nested` guard inside the recursive
                // call rather than a separate value-equality check here.
                types_are_comparable_for_assertion_inner(
                    db,
                    *source_ty,
                    *target_ty,
                    depth + 1,
                    true,
                )
            });
            if !any_comparable {
                return false;
            }
        }
        // Intentionally NOT returning false for missing target properties
    }

    if !found_common {
        // Weak type overlap: if either side has ONLY optional properties (a "weak type"),
        // it overlaps with any object that has at least one property. Every object
        // structurally satisfies a weak type (optional properties can all be missing).
        // This matches tsc's `isTypeComparableTo` which bypasses weak type detection
        // (TS2559) in comparable contexts like type assertions.
        let source_is_weak =
            !source_props.is_empty() && source_props.iter().all(|(_, _, opt)| *opt);
        let target_is_weak =
            !target_props.is_empty() && target_props.iter().all(|(_, _, opt)| *opt);
        if (source_is_weak && !target_props.is_empty())
            || (target_is_weak && !source_props.is_empty())
        {
            return true;
        }
    }

    found_common
}
