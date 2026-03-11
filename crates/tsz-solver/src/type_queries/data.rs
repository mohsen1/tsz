//! Type Content Queries and Data Extraction Helpers
//!
//! This module provides functions for extracting type data and checking type content.
//! These functions abstract away the internal `TypeData` representation and provide
//! a stable API for querying type properties without matching on `TypeData` directly.

use crate::TypeDatabase;
use crate::evaluation::evaluate::TypeEvaluator;
use crate::relations::subtype::SubtypeChecker;
use crate::types::{IntrinsicKind, LiteralValue, MappedModifier, PropertyInfo, TypeData, TypeId};
use crate::visitors::visitor_predicates::contains_type_matching;
use rustc_hash::FxHashSet;
use tsz_common::Atom;

// =============================================================================
// Type Content Queries
// =============================================================================

/// Check if a type contains any type parameters.
///
/// Unlike the solver-internal `visitor::contains_type_parameters`, this version
/// also treats `ThisType` (polymorphic `this`) and `BoundParameter` (generic
/// signature-index parameters) as type parameters. This is the correct semantic
/// for checker use cases that need to decide whether a type requires instantiation.
pub fn contains_type_parameters_db(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    contains_type_matching(db, type_id, |key| {
        matches!(
            key,
            TypeData::TypeParameter(_)
                | TypeData::Infer(_)
                | TypeData::ThisType
                | TypeData::BoundParameter(_)
        )
    })
}

/// Check if a type contains any `infer` types.
///
/// Delegates to `visitor_predicates::contains_type_matching` with an `Infer`-only
/// predicate.
pub fn contains_infer_types_db(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    contains_type_matching(db, type_id, |key| match key {
        TypeData::Infer(_) => true,
        TypeData::TypeParameter(tp) => {
            let name = db.resolve_atom(tp.name);
            name.starts_with("__infer_") || name.starts_with("__infer_src_")
        }
        _ => false,
    })
}

/// Check if a type contains unresolved type parameters other than tsz's internal
/// `__infer_*` placeholders.
///
/// This is useful when a structural contextual type like `[__infer_0, __infer_1]`
/// should still be allowed to guide recontextualization, while real generic
/// type parameters (`T`, `U`, `this`, bound params) should still block it.
pub fn contains_non_infer_type_parameters_db(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    contains_type_matching(db, type_id, |key| match key {
        TypeData::TypeParameter(tp) => {
            let name = db.resolve_atom(tp.name);
            !(name.starts_with("__infer_") || name.starts_with("__infer_src_"))
        }
        TypeData::Infer(_) | TypeData::ThisType | TypeData::BoundParameter(_) => true,
        _ => false,
    })
}

/// Check whether a type is itself a bare unresolved infer placeholder, not a
/// larger structural type that merely contains placeholders.
pub fn is_bare_infer_placeholder_db(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    match db.lookup(type_id) {
        Some(TypeData::Infer(_)) => true,
        Some(TypeData::TypeParameter(tp)) => {
            let name = db.resolve_atom(tp.name);
            name.starts_with("__infer_") || name.starts_with("__infer_src_")
        }
        _ => false,
    }
}

/// Check if a type contains the error type.
///
/// Delegates to `visitor_predicates::contains_type_matching` with an `Error`-only
/// predicate, plus a fast path for the well-known `TypeId::ERROR`.
pub fn contains_error_type_db(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id == TypeId::ERROR {
        return true;
    }
    contains_type_matching(db, type_id, |key| matches!(key, TypeData::Error))
}

/// Check if a type contains the `never` intrinsic.
///
/// Delegates to `visitor_predicates::contains_type_matching` with a `Never`-only
/// predicate, plus a fast path for the well-known `TypeId::NEVER`.
pub fn contains_never_type_db(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id == TypeId::NEVER {
        return true;
    }
    contains_type_matching(db, type_id, |key| {
        matches!(key, TypeData::Intrinsic(IntrinsicKind::Never))
    })
}

// =============================================================================
// Type Extraction Helpers
// =============================================================================
// These functions extract data from types, avoiding the need for checker code
// to match on TypeData directly.
//
// ## Usage Pattern
//
// These are SHALLOW queries that do NOT resolve Lazy/Ref automatically.
// Checker code must resolve types before calling these:
//
// ```rust,ignore
// // 1. Resolve the type first
// let resolved_id = self.solver.resolve_type(type_id);
//
// // 2. Then use the extractor
// if let Some(members) = get_union_members(self.db, resolved_id) {
//     // ...
// }
// ```
//
// ## Available Extractors
//
// - Unions: get_union_members
// - Intersections: get_intersection_members
// - Objects: get_object_shape_id, get_object_shape
// - Arrays: get_array_element_type
// - Tuples: get_tuple_elements
//
// These helpers cover 90%+ of structural extraction needs in the Checker.

/// Get the members of a union type.
///
/// Returns None if the type is not a union.
pub fn get_union_members(db: &dyn TypeDatabase, type_id: TypeId) -> Option<Vec<TypeId>> {
    match db.lookup(type_id) {
        Some(TypeData::Union(list_id)) => {
            let members = db.type_list(list_id);
            Some(members.to_vec())
        }
        _ => None,
    }
}

/// Get the members of an intersection type.
///
/// Returns None if the type is not an intersection.
pub fn get_intersection_members(db: &dyn TypeDatabase, type_id: TypeId) -> Option<Vec<TypeId>> {
    match db.lookup(type_id) {
        Some(TypeData::Intersection(list_id)) => {
            let members = db.type_list(list_id);
            Some(members.to_vec())
        }
        _ => None,
    }
}

/// Apply a mapping function to each member of a union or intersection type,
/// reconstructing the compound type from the mapped results.
///
/// If the type is a union, maps each member and rebuilds a union.
/// If the type is an intersection, maps each member and rebuilds an intersection.
/// If the type is neither, returns `None` (the caller should handle the non-compound case).
///
/// This eliminates the common checker anti-pattern of:
/// ```text
/// if let Some(members) = get_union_members(db, ty) {
///     let mapped: Vec<_> = members.into_iter().map(|m| transform(m)).collect();
///     factory.union(mapped)
/// } else if let Some(members) = get_intersection_members(db, ty) {
///     let mapped: Vec<_> = members.into_iter().map(|m| transform(m)).collect();
///     factory.intersection(mapped)
/// }
/// ```
pub fn map_compound_members(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    mut f: impl FnMut(TypeId) -> TypeId,
) -> Option<TypeId> {
    match db.lookup(type_id) {
        Some(TypeData::Union(list_id)) => {
            let members = db.type_list(list_id);
            let mapped: Vec<TypeId> = members.iter().map(|&m| f(m)).collect();
            Some(db.union(mapped))
        }
        Some(TypeData::Intersection(list_id)) => {
            let members = db.type_list(list_id);
            let mapped: Vec<TypeId> = members.iter().map(|&m| f(m)).collect();
            Some(db.intersection(mapped))
        }
        _ => None,
    }
}

/// Like [`map_compound_members`], but only reconstructs the compound type if at least
/// one member was changed by the mapping function. Returns the original `type_id`
/// unchanged if all mapped members are identical to the originals.
///
/// Returns `None` if the type is not a union or intersection.
pub fn map_compound_members_if_changed(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    mut f: impl FnMut(TypeId) -> TypeId,
) -> Option<TypeId> {
    match db.lookup(type_id) {
        Some(TypeData::Union(list_id)) => {
            let members = db.type_list(list_id);
            let mapped: Vec<TypeId> = members.iter().map(|&m| f(m)).collect();
            if mapped.iter().eq(members.iter()) {
                Some(type_id)
            } else {
                Some(db.union(mapped))
            }
        }
        Some(TypeData::Intersection(list_id)) => {
            let members = db.type_list(list_id);
            let mapped: Vec<TypeId> = members.iter().map(|&m| f(m)).collect();
            if mapped.iter().eq(members.iter()) {
                Some(type_id)
            } else {
                Some(db.intersection(mapped))
            }
        }
        _ => None,
    }
}

/// Get the element type of an array.
///
/// Returns None if the type is not an array.
pub fn get_array_element_type(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    match db.lookup(type_id) {
        Some(TypeData::Array(element_type)) => Some(element_type),
        // `readonly T[]` wraps the array in ReadonlyType — unwrap and retry.
        Some(TypeData::ReadonlyType(inner)) => get_array_element_type(db, inner),
        _ => None,
    }
}

/// Get the elements of a tuple type.
///
/// Returns None if the type is not a tuple.
/// Returns a vector of (`TypeId`, optional, rest, name) tuples.
pub fn get_tuple_elements(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<Vec<crate::types::TupleElement>> {
    match db.lookup(type_id) {
        Some(TypeData::Tuple(list_id)) => {
            let elements = db.tuple_list(list_id);
            Some(elements.to_vec())
        }
        // `readonly [A, B]` is wrapped in ReadonlyType — unwrap and retry.
        Some(TypeData::ReadonlyType(inner)) => get_tuple_elements(db, inner),
        // Intersection of tuples: pick the tuple member with the most specific elements.
        // e.g., `[any] & [1]` should provide tuple context from `[1]` (more specific).
        // If multiple tuple members exist, prefer the one whose elements are not `any`.
        Some(TypeData::Intersection(list_id)) => {
            let members = db.type_list(list_id);
            let mut best: Option<Vec<crate::types::TupleElement>> = None;
            for &m in members.iter() {
                if let Some(elems) = get_tuple_elements(db, m)
                    && (best.is_none() || elems.iter().any(|e| e.type_id != TypeId::ANY))
                {
                    best = Some(elems);
                }
            }
            best
        }
        _ => None,
    }
}

/// Check if a type is a union containing at least one tuple member.
///
/// This detects the `T extends readonly unknown[] | []` pattern where `| []`
/// is a deliberate hint in TypeScript to infer tuple types from array literals.
/// Used by `Promise.all`, `Promise.allSettled`, and similar APIs.
pub fn union_contains_tuple(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    match db.lookup(type_id) {
        Some(TypeData::Union(list_id)) => {
            let members = db.type_list(list_id);
            members.iter().any(|&m| get_tuple_elements(db, m).is_some())
        }
        _ => false,
    }
}

/// Check if a type is or evaluates to a homomorphic mapped type.
///
/// A homomorphic mapped type has constraint `keyof T` for some type parameter T,
/// e.g., `{ [K in keyof T]: F<T[K]> }`. This includes type aliases that expand
/// to homomorphic mapped types, like `Definition<T> = { [K in keyof T]: ... }`.
///
/// This is used by the checker to determine when array literals should be typed
/// as tuples: homomorphic mapped types preserve array/tuple structure, so the
/// array literal input should maintain per-element type information.
pub fn is_homomorphic_mapped_type_context(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    match db.lookup(type_id) {
        Some(TypeData::Mapped(mapped_id)) => {
            let mapped = db.mapped_type(mapped_id);
            is_keyof_type_parameter(db, mapped.constraint)
        }
        Some(TypeData::Application(_) | TypeData::Lazy(_)) => {
            let evaluated = crate::evaluation::evaluate::evaluate_type(db, type_id);
            if evaluated != type_id {
                return is_homomorphic_mapped_type_context(db, evaluated);
            }
            false
        }
        _ => false,
    }
}

/// Check if a type is `keyof T` where T is a type parameter (possibly intersected).
fn is_keyof_type_parameter(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    match db.lookup(type_id) {
        Some(TypeData::KeyOf(target)) => {
            matches!(db.lookup(target), Some(TypeData::TypeParameter(_)))
        }
        Some(TypeData::Intersection(members)) => {
            let member_list = db.type_list(members);
            member_list.iter().any(|&m| is_keyof_type_parameter(db, m))
        }
        _ => false,
    }
}

/// Get the union of all element types in a tuple.
///
/// For each element: rest elements are unwrapped to their array element type,
/// and optional elements include `undefined` in the result. Returns the union
/// of all resulting types, or `None` if the type is not a tuple.
///
/// This encapsulates the common checker pattern of iterating tuple elements
/// and rebuilding a union from their types.
pub fn get_tuple_element_type_union(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    let elems = get_tuple_elements(db, type_id)?;
    let mut members = Vec::with_capacity(elems.len());
    for elem in elems {
        let mut ty = if elem.rest {
            get_array_element_type(db, elem.type_id).unwrap_or(elem.type_id)
        } else {
            elem.type_id
        };
        if elem.optional {
            ty = db.union(vec![ty, TypeId::UNDEFINED]);
        }
        members.push(ty);
    }
    Some(db.union(members))
}

/// Compute the `keyof` type for an object shape.
///
/// Returns the union of string literal types for all property names in the object.
/// Returns `TypeId::NEVER` if the object has no properties, or `None` if the type
/// is not an object type.
///
/// This is the type-computation portion of `keyof T` when T is an object.
pub fn keyof_object_properties(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    let shape = get_object_shape(db, type_id)?;
    if shape.properties.is_empty() {
        return Some(TypeId::NEVER);
    }
    let key_types: Vec<TypeId> = shape
        .properties
        .iter()
        .filter(|p| p.visibility == crate::Visibility::Public)
        .map(|p| db.literal_string_atom(p.name))
        .collect();
    Some(crate::utils::union_or_single(db, key_types))
}

/// Get the applicable contextual type for an array literal from a (possibly union) type.
///
/// When the contextual type is a union like `[number] | string`, this extracts only
/// the array/tuple constituents that are applicable to an array literal expression.
/// If the type is already a tuple or array, returns it directly.
/// If the type is a union, filters to only tuple/array members and returns their union.
/// Returns None if no array/tuple constituents are found.
pub fn get_array_applicable_type(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    match db.lookup(type_id) {
        Some(TypeData::Tuple(_) | TypeData::Array(_)) => Some(type_id),
        // `readonly T[]` and `readonly [A, B]` are wrapped in ReadonlyType — unwrap and retry.
        Some(TypeData::ReadonlyType(inner)) => get_array_applicable_type(db, inner),
        Some(
            TypeData::Application(_)
            | TypeData::Mapped(_)
            | TypeData::Conditional(_)
            | TypeData::Lazy(_),
        ) => {
            // Try evaluating deferred/generic wrappers first so tuple/array shape
            // becomes visible to contextual typing (e.g. conditional true branch
            // reducing to `[A, B, C]`).
            let evaluated = crate::evaluation::evaluate::evaluate_type(db, type_id);
            if evaluated != type_id {
                return get_array_applicable_type(db, evaluated);
            }
            if let Some(TypeData::Conditional(cond_id)) = db.lookup(type_id) {
                let cond = db.conditional_type(cond_id);
                let mut applicable = Vec::new();
                for branch in [cond.true_type, cond.false_type] {
                    if branch == type_id {
                        continue;
                    }
                    if let Some(branch_applicable) = get_array_applicable_type(db, branch) {
                        applicable.push(branch_applicable);
                    }
                }
                return match applicable.len() {
                    0 => None,
                    1 => Some(applicable[0]),
                    _ => Some(db.union(applicable)),
                };
            }
            None
        }
        Some(TypeData::TypeParameter(info)) => info
            .constraint
            .and_then(|constraint| get_array_applicable_type(db, constraint)),
        Some(TypeData::Union(list_id)) => {
            let members = db.type_list(list_id);
            let applicable: Vec<TypeId> = members
                .iter()
                .filter_map(|&m| get_array_applicable_type(db, m))
                .collect();
            match applicable.len() {
                0 => None,
                1 => Some(applicable[0]),
                _ => Some(db.union(applicable)),
            }
        }
        // Intersection of tuples/arrays: if any member is array-applicable, preserve it.
        // e.g., `[any] & [1]` should be recognized as a tuple context.
        Some(TypeData::Intersection(list_id)) => {
            let members = db.type_list(list_id);
            // Return the first tuple/array member — it provides the structural context
            for &m in members.iter() {
                if get_array_applicable_type(db, m).is_some() {
                    return Some(type_id);
                }
            }
            None
        }
        _ => None,
    }
}

/// Unpack a rest parameter with tuple type into individual fixed parameters.
///
/// In TypeScript, `(...args: [A, B, C]) => R` is equivalent to `(a: A, b: B, c: C) => R`.
/// This function handles the unpacking:
///
/// # Examples
///
/// - Input: `...args: [string, number]`
///   Output: `[ParamInfo { type_id: string, optional: false, rest: false },
///            ParamInfo { type_id: number, optional: false, rest: false }]`
///
/// - Input: `...args: [string, number?]`
///   Output: `[ParamInfo { type_id: string, optional: false, rest: false },
///            ParamInfo { type_id: number, optional: true, rest: false }]`
///
/// - Input: `...args: [string, ...number[]]`
///   Output: `[ParamInfo { type_id: string, optional: false, rest: false },
///            ParamInfo { type_id: number[], optional: false, rest: true }]`
///
/// - Input: `x: string` (non-rest parameter)
///   Output: `[ParamInfo { type_id: string, ... }]` (unchanged)
///
/// - Input: `...args: string[]` (array rest, not tuple)
///   Output: `[ParamInfo { type_id: string[], rest: true }]` (unchanged)
///
/// This enables proper function type compatibility and generic inference for patterns like:
/// - `pipe<A extends any[], B>(ab: (...args: A) => B): (...args: A) => B`
/// - Where `A = [T]` should be inferred from a single-parameter function
pub fn unpack_tuple_rest_parameter(
    db: &dyn TypeDatabase,
    param: &crate::types::ParamInfo,
) -> Vec<crate::types::ParamInfo> {
    // Non-rest parameters pass through unchanged
    if !param.rest {
        return vec![param.clone()];
    }

    // Check if the rest parameter type is a tuple
    if let Some(tuple_elements) = get_tuple_elements(db, param.type_id) {
        // Convert tuple elements to individual parameters
        tuple_elements
            .into_iter()
            .map(|elem| crate::types::ParamInfo {
                name: elem.name, // Preserve tuple element names if present
                type_id: elem.type_id,
                optional: elem.optional,
                rest: elem.rest, // Preserve rest flag for trailing ...T[] in tuple
            })
            .collect()
    } else {
        // Not a tuple - keep the rest parameter as-is
        // This handles cases like `...args: string[]` which should remain a rest parameter
        vec![param.clone()]
    }
}

/// Get the object shape ID for an object type.
///
/// Returns None if the type is not an object type.
pub fn get_object_shape_id(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<crate::types::ObjectShapeId> {
    match db.lookup(type_id) {
        Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => Some(shape_id),
        _ => None,
    }
}

/// Get the object shape for an object type.
///
/// Returns None if the type is not an object type.
pub fn get_object_shape(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<crate::types::ObjectShape>> {
    match db.lookup(type_id) {
        Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
            Some(db.object_shape(shape_id))
        }
        _ => None,
    }
}

/// Find a named property in an object type by its atom name.
///
/// Returns `Some(PropertyInfo)` if the object has a property with the given name,
/// or `None` if the type is not an object or the property is not found.
/// This encapsulates the common checker pattern of getting an object shape
/// and iterating its properties to find a match.
pub fn find_property_in_object(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    name: Atom,
) -> Option<crate::types::PropertyInfo> {
    let shape = get_object_shape(db, type_id)?;
    PropertyInfo::find_in_slice(&shape.properties, name).cloned()
}

/// Find a named property in an object type by string name.
///
/// Like [`find_property_in_object`] but resolves the atom to compare by string value.
/// Useful when the caller has a `&str` rather than an `Atom`.
pub fn find_property_in_object_by_str(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    name: &str,
) -> Option<crate::types::PropertyInfo> {
    let shape = get_object_shape(db, type_id)?;
    shape
        .properties
        .iter()
        .find(|p| db.resolve_atom_ref(p.name).as_ref() == name)
        .cloned()
}

/// Check if a type that is a numeric literal (or union of numeric literals) is
/// a valid index for `object_type` by matching numeric values against named
/// properties.
///
/// TypeScript represents `keyof { 0: T; 1: U }` as `0 | 1` (numeric literal
/// types). Our `evaluate_keyof` uses string-atom literals for property names,
/// so `is_assignable_to(0 | 1, "0" | "1")` fails even when `0` and `1` are
/// valid property names. This function bridges that gap by explicitly checking
/// each numeric member of `index_type` against the object's named properties.
///
/// Returns `true` if and only if:
/// 1. `index_type` is a numeric literal or union of numeric literals, AND
/// 2. Every numeric value corresponds to a named property of `object_type`.
///
/// Returns `false` if `index_type` contains any non-numeric member, if the
/// union is empty, or if any numeric value has no matching property.
pub fn numeric_literal_index_valid_for_object(
    db: &dyn TypeDatabase,
    index_type: TypeId,
    object_type: TypeId,
) -> bool {
    // Collect union members; treat a non-union as a single-element slice.
    let members = match get_union_members(db, index_type) {
        Some(ms) => ms,
        None => vec![index_type],
    };
    if members.is_empty() {
        return false;
    }
    for &member in &members {
        // Each member must be a numeric literal.
        let num_val = match db.lookup(member) {
            Some(TypeData::Literal(LiteralValue::Number(n))) => n.0,
            _ => return false,
        };
        // Convert the numeric value to its canonical JS property-name string.
        // For non-negative integers this is simply the decimal representation.
        let prop_name = numeric_value_to_property_name(num_val);
        // Check if the object has a property with that name.
        if find_property_in_object_by_str(db, object_type, &prop_name).is_none() {
            return false;
        }
    }
    true
}

/// Convert an `f64` numeric literal value to its canonical JavaScript property
/// name string (matching `Number.prototype.toString()` for the common cases).
fn numeric_value_to_property_name(value: f64) -> String {
    // For non-negative integers representable exactly as u64, use integer format.
    // This covers 0, 1, 2, … which are the typical numeric property name cases.
    if value.is_finite() && value >= 0.0 && value.fract() == 0.0 && value < 1e15 {
        return format!("{}", value as u64);
    }
    // Fall back to canonicalize_numeric_name for edge cases.
    crate::utils::canonicalize_numeric_name(&format!("{value}"))
        .unwrap_or_else(|| format!("{value}"))
}

/// Find a named property in any type shape (object or callable) by string name.
///
/// Like [`find_property_in_object_by_str`] but also searches callable shapes.
/// This handles types where properties may be attached to function/class types
/// (e.g., namespace-merged functions or classes with static properties).
pub fn find_property_in_type_by_str(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    name: &str,
) -> Option<crate::types::PropertyInfo> {
    match db.lookup(type_id)? {
        TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
            let shape = db.object_shape(shape_id);
            shape
                .properties
                .iter()
                .find(|p| db.resolve_atom_ref(p.name).as_ref() == name)
                .cloned()
        }
        TypeData::Callable(shape_id) => {
            let shape = db.callable_shape(shape_id);
            shape
                .properties
                .iter()
                .find(|p| db.resolve_atom_ref(p.name).as_ref() == name)
                .cloned()
        }
        _ => None,
    }
}

/// Check if a type has a named property accessible on all branches.
///
/// For object types, checks if the property exists in the shape.
/// For union types, returns `true` only if ALL members have the property
/// (matching tsc's TS2713 vs TS2702 distinction).
/// For intersection types, returns `true` if ANY member has the property.
pub fn type_has_property_by_str(db: &dyn TypeDatabase, type_id: TypeId, name: &str) -> bool {
    fn member_has_property(db: &dyn TypeDatabase, type_id: TypeId, name: &str) -> bool {
        match db.lookup(type_id) {
            Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
                let shape = db.object_shape(shape_id);
                shape
                    .properties
                    .iter()
                    .any(|p| db.resolve_atom_ref(p.name).as_ref() == name)
            }
            Some(TypeData::Intersection(list_id)) => {
                let members = db.type_list(list_id).to_vec();
                members.iter().any(|&m| member_has_property(db, m, name))
            }
            _ => false,
        }
    }

    match db.lookup(type_id) {
        Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
            let shape = db.object_shape(shape_id);
            shape
                .properties
                .iter()
                .any(|p| db.resolve_atom_ref(p.name).as_ref() == name)
        }
        Some(TypeData::Union(list_id)) => {
            let members = db.type_list(list_id).to_vec();
            !members.is_empty() && members.iter().all(|&m| member_has_property(db, m, name))
        }
        Some(TypeData::Intersection(list_id)) => {
            let members = db.type_list(list_id).to_vec();
            members.iter().any(|&m| member_has_property(db, m, name))
        }
        _ => false,
    }
}

/// Unwrap readonly type wrappers.
///
/// Returns the inner type if this is a `ReadonlyType`, otherwise returns the original type.
/// Does not recurse - call repeatedly to fully unwrap.
pub fn unwrap_readonly(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    match db.lookup(type_id) {
        Some(TypeData::ReadonlyType(inner)) => inner,
        _ => type_id,
    }
}

/// Unwrap all readonly type wrappers recursively.
///
/// Keeps unwrapping until the type is no longer a `ReadonlyType`.
pub fn unwrap_readonly_deep(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    let mut current = type_id;
    let mut depth = 0;
    const MAX_DEPTH: usize = 100;

    while let Some(TypeData::ReadonlyType(inner)) = db.lookup(current) {
        depth += 1;
        if depth > MAX_DEPTH {
            break;
        }
        current = inner;
    }
    current
}

/// Get the type parameter info if this is a type parameter.
///
/// Returns None if not a type parameter.
pub fn get_type_parameter_info(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<crate::types::TypeParamInfo> {
    match db.lookup(type_id) {
        Some(TypeData::TypeParameter(info) | TypeData::Infer(info)) => Some(info),
        _ => None,
    }
}

/// Get the constraint of a type parameter.
///
/// Returns None if not a type parameter or has no constraint.
pub fn get_type_parameter_constraint(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    match db.lookup(type_id) {
        Some(TypeData::TypeParameter(info) | TypeData::Infer(info)) => info.constraint,
        _ => None,
    }
}

/// Resolve a type parameter to its base constraint for TS2344 checking.
///
/// If the type IS a `TypeParameter` with a constraint, returns the constraint.
/// If it IS a `TypeParameter` without a constraint, returns `unknown`.
/// Returns the type unchanged for anything else (including `Infer` types,
/// composite types, etc.).
///
/// This is used for TS2344 constraint checking: when a type parameter `U extends number`
/// is used as `T extends string`, tsc resolves `U` to `number` and checks `number <: string`.
/// `Infer` types inside conditional types should NOT be resolved here — they are checked
/// during conditional type evaluation, not at type argument validation time.
pub fn get_base_constraint_of_type(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    match db.lookup(type_id) {
        Some(TypeData::TypeParameter(info)) => info.constraint.unwrap_or(TypeId::UNKNOWN),
        _ => type_id,
    }
}

/// Get the callable shape for a callable type.
///
/// Returns None if the type is not a Callable.
pub fn get_callable_shape(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<crate::types::CallableShape>> {
    match db.lookup(type_id) {
        Some(TypeData::Callable(shape_id)) => Some(db.callable_shape(shape_id)),
        _ => None,
    }
}

/// Get call signatures from a type.
///
/// For `Callable` types, returns their call signatures directly.
/// For intersection types, collects call signatures from all callable members.
/// Returns None if no call signatures are found.
pub fn get_call_signatures(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<Vec<crate::CallSignature>> {
    if let Some(shape) = get_callable_shape(db, type_id) {
        return Some(shape.call_signatures.clone());
    }
    // For intersection types, collect call signatures from all members
    if let Some(members) = get_intersection_members(db, type_id) {
        let mut all_sigs = Vec::new();
        for member in &members {
            if let Some(shape) = get_callable_shape(db, *member) {
                all_sigs.extend(shape.call_signatures.iter().cloned());
            }
        }
        if !all_sigs.is_empty() {
            return Some(all_sigs);
        }
    }
    None
}

/// Get construct signatures from a type.
///
/// For `Callable` types, returns their construct signatures directly.
/// For intersection types, collects construct signatures from all callable members.
/// Returns None if no construct signatures are found.
pub fn get_construct_signatures(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<Vec<crate::CallSignature>> {
    if let Some(shape) = get_callable_shape(db, type_id) {
        return Some(shape.construct_signatures.clone());
    }
    // For intersection types, collect construct signatures from all members
    if let Some(members) = get_intersection_members(db, type_id) {
        let mut all_sigs = Vec::new();
        for member in &members {
            if let Some(shape) = get_callable_shape(db, *member) {
                all_sigs.extend(shape.construct_signatures.iter().cloned());
            }
        }
        if !all_sigs.is_empty() {
            return Some(all_sigs);
        }
    }
    None
}

/// Get the union of all construct signature return types from a callable shape.
///
/// Returns `Some(TypeId)` for the union of all construct signature return types,
/// or `None` if the shape has no construct signatures. This encapsulates the common
/// pattern of iterating construct signatures to collect instance types.
pub fn get_construct_return_type_union(
    db: &dyn TypeDatabase,
    shape_id: crate::types::CallableShapeId,
) -> Option<TypeId> {
    let shape = db.callable_shape(shape_id);
    if shape.construct_signatures.is_empty() {
        return None;
    }
    let returns: Vec<TypeId> = shape
        .construct_signatures
        .iter()
        .map(|sig| sig.return_type)
        .collect();
    Some(crate::utils::union_or_single(db, returns))
}

/// Get the function shape for a function type.
///
/// Returns None if the type is not a Function.
pub fn get_function_shape(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<crate::types::FunctionShape>> {
    match db.lookup(type_id) {
        Some(TypeData::Function(shape_id)) => Some(db.function_shape(shape_id)),
        _ => None,
    }
}

/// Return a function type with all `ERROR` parameter and return positions rewritten to `ANY`.
///
/// Returns the original `type_id` when:
/// - it is not a function type
/// - the function shape does not contain `ERROR` in parameter or return positions
pub fn rewrite_function_error_slots_to_any(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    let Some(shape) = get_function_shape(db, type_id) else {
        return type_id;
    };

    let has_error = shape.params.iter().any(|p| p.type_id == TypeId::ERROR)
        || shape.return_type == TypeId::ERROR;
    if !has_error {
        return type_id;
    }

    let params = shape
        .params
        .iter()
        .map(|p| crate::types::ParamInfo {
            type_id: if p.type_id == TypeId::ERROR {
                TypeId::ANY
            } else {
                p.type_id
            },
            ..p.clone()
        })
        .collect();
    let return_type = if shape.return_type == TypeId::ERROR {
        TypeId::ANY
    } else {
        shape.return_type
    };

    db.function(crate::types::FunctionShape {
        type_params: shape.type_params.clone(),
        params,
        this_type: shape.this_type,
        return_type,
        type_predicate: shape.type_predicate.clone(),
        is_constructor: shape.is_constructor,
        is_method: shape.is_method,
    })
}

/// Return a function type with the same signature but a replaced return type.
///
/// Returns the original `type_id` when:
/// - it is not a function type
/// - the existing return type already equals `new_return`
pub fn replace_function_return_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    new_return: TypeId,
) -> TypeId {
    let Some(shape) = get_function_shape(db, type_id) else {
        return type_id;
    };
    if shape.return_type == new_return {
        return type_id;
    }

    db.function(crate::types::FunctionShape {
        type_params: shape.type_params.clone(),
        params: shape.params.clone(),
        this_type: shape.this_type,
        return_type: new_return,
        type_predicate: shape.type_predicate.clone(),
        is_constructor: shape.is_constructor,
        is_method: shape.is_method,
    })
}

/// Erase a generic function's type parameters by replacing them with `any`.
///
/// This mirrors TSC's `getErasedSignature` used in `isImplementationCompatibleWithOverload`.
/// Returns the original type when it is not a function or has no type parameters.
pub fn erase_function_type_params_to_any(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    let Some(shape) = get_function_shape(db, type_id) else {
        return type_id;
    };
    if shape.type_params.is_empty() {
        return type_id;
    }

    use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};

    let mut subst = TypeSubstitution::new();
    for tp in &shape.type_params {
        subst.insert(tp.name, TypeId::ANY);
    }

    let params = shape
        .params
        .iter()
        .map(|p| crate::types::ParamInfo {
            type_id: instantiate_type(db, p.type_id, &subst),
            ..p.clone()
        })
        .collect();
    let return_type = instantiate_type(db, shape.return_type, &subst);
    let this_type = shape.this_type.map(|t| instantiate_type(db, t, &subst));

    db.function(crate::types::FunctionShape {
        type_params: Vec::new(), // erased
        params,
        this_type,
        return_type,
        type_predicate: shape.type_predicate.clone(),
        is_constructor: shape.is_constructor,
        is_method: shape.is_method,
    })
}

/// Get the conditional type info for a conditional type.
///
/// Returns None if the type is not a Conditional.
pub fn get_conditional_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<crate::types::ConditionalType>> {
    match db.lookup(type_id) {
        Some(TypeData::Conditional(cond_id)) => Some(db.conditional_type(cond_id)),
        _ => None,
    }
}

/// Get the mapped type info for a mapped type.
///
/// Returns None if the type is not a Mapped type.
pub fn get_mapped_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<crate::types::MappedType>> {
    match db.lookup(type_id) {
        Some(TypeData::Mapped(mapped_id)) => Some(db.mapped_type(mapped_id)),
        _ => None,
    }
}

/// Get the mapped type id together with the mapped type info.
///
/// Returns None if the type is not a Mapped type.
pub fn get_mapped_type_with_id(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<(
    crate::types::MappedTypeId,
    std::sync::Arc<crate::types::MappedType>,
)> {
    match db.lookup(type_id) {
        Some(TypeData::Mapped(mapped_id)) => Some((mapped_id, db.mapped_type(mapped_id))),
        _ => None,
    }
}

/// Get the default type for a type-parameter-like type.
///
/// Returns None if the type is not a `TypeParameter` or `Infer`, or if it has no default.
pub fn get_type_parameter_default(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    match db.lookup(type_id) {
        Some(TypeData::TypeParameter(info) | TypeData::Infer(info)) => info.default,
        _ => None,
    }
}

/// Get the type application info for a generic application type.
///
/// Returns None if the type is not an Application.
pub fn get_type_application(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<crate::types::TypeApplication>> {
    match db.lookup(type_id) {
        Some(TypeData::Application(app_id)) => Some(db.type_application(app_id)),
        _ => None,
    }
}

/// Get the index access components (object type and index type).
///
/// Returns None if the type is not an `IndexAccess`.
pub fn get_index_access_types(db: &dyn TypeDatabase, type_id: TypeId) -> Option<(TypeId, TypeId)> {
    match db.lookup(type_id) {
        Some(TypeData::IndexAccess(obj, idx)) => Some((obj, idx)),
        _ => None,
    }
}

/// Instantiate a mapped type template for a specific property key, handling
/// name collisions between the mapped key parameter and outer type parameters.
///
/// When a mapped type template is `IndexAccess(T, K)` and the object type `T`
/// is a `TypeParameter` with the **same name atom** as the mapped key parameter,
/// name-based `TypeSubstitution` would incorrectly replace both `T` and `K`
/// with the key literal.  This happens with e.g. `Readonly<P>` where the lib
/// defines `type Readonly<T> = { readonly [P in keyof T]: T[P] }` and the user
/// has a type parameter also named `P`.
///
/// Returns `IndexAccess(T, key_literal)` when a collision is detected (bypassing
/// substitution), or the normally-substituted template otherwise.
pub fn instantiate_mapped_template_for_property(
    db: &dyn TypeDatabase,
    template: TypeId,
    key_param_name: Atom,
    key_literal: TypeId,
) -> TypeId {
    use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};

    // Check if template is IndexAccess(obj, key) where obj is a TypeParameter
    // sharing the same name as the mapped key parameter.
    if let Some((idx_obj, idx_key)) = get_index_access_types(db, template)
        && idx_obj != idx_key
        && let Some(info) = get_type_parameter_info(db, idx_obj)
        && info.name == key_param_name
    {
        // Name collision detected — construct IndexAccess directly
        return db.index_access(idx_obj, key_literal);
    }

    // Normal path: substitute the key parameter name with the key literal
    let mut subst = TypeSubstitution::new();
    subst.insert(key_param_name, key_literal);
    instantiate_type(db, template, &subst)
}

fn collect_exact_literal_property_keys_inner(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    keys: &mut FxHashSet<Atom>,
    visited: &mut FxHashSet<TypeId>,
) -> Option<()> {
    if !visited.insert(type_id) {
        return Some(());
    }

    let evaluated = crate::evaluation::evaluate::evaluate_type(db, type_id);
    if evaluated != type_id {
        return collect_exact_literal_property_keys_inner(db, evaluated, keys, visited);
    }

    match db.lookup(type_id) {
        Some(TypeData::Literal(LiteralValue::String(atom))) => {
            keys.insert(atom);
            Some(())
        }
        Some(TypeData::Literal(LiteralValue::Number(n))) => {
            let atom = db.intern_string(
                &crate::relations::subtype::rules::literals::format_number_for_template(n.0),
            );
            keys.insert(atom);
            Some(())
        }
        Some(TypeData::UniqueSymbol(sym)) => {
            let atom = db.intern_string(&format!("__unique_{}", sym.0));
            keys.insert(atom);
            Some(())
        }
        Some(TypeData::Union(members)) => {
            for &member in db.type_list(members).iter() {
                collect_exact_literal_property_keys_inner(db, member, keys, visited)?;
            }
            Some(())
        }
        Some(TypeData::Intersection(members)) => {
            let mut saw_precise_member = false;
            for &member in db.type_list(members).iter() {
                if collect_exact_literal_property_keys_inner(db, member, keys, visited).is_some() {
                    saw_precise_member = true;
                    continue;
                }
                if intersection_member_preserves_literal_keys(db, member) {
                    continue;
                }
                return None;
            }
            saw_precise_member.then_some(())
        }
        Some(TypeData::Enum(_, members)) => {
            collect_exact_literal_property_keys_inner(db, members, keys, visited)
        }
        Some(TypeData::Conditional(cond_id)) => {
            let cond = db.conditional_type(cond_id);
            let branch = resolve_concrete_conditional_branch(db, &cond)?;
            collect_exact_literal_property_keys_inner(db, branch, keys, visited)
        }
        Some(TypeData::TypeParameter(info) | TypeData::Infer(info)) => {
            info.constraint.and_then(|constraint| {
                collect_exact_literal_property_keys_inner(db, constraint, keys, visited)
            })
        }
        Some(TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner)) => {
            collect_exact_literal_property_keys_inner(db, inner, keys, visited)
        }
        Some(TypeData::Intrinsic(crate::types::IntrinsicKind::Never)) => Some(()),
        _ => None,
    }
}

fn collect_exact_literal_property_keys(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<FxHashSet<Atom>> {
    let mut keys = FxHashSet::default();
    let mut visited = FxHashSet::default();
    collect_exact_literal_property_keys_inner(db, type_id, &mut keys, &mut visited)?;
    Some(keys)
}

fn intersection_member_preserves_literal_keys(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(
        db.lookup(type_id),
        Some(
            TypeData::Intrinsic(crate::types::IntrinsicKind::String)
                | TypeData::Intrinsic(crate::types::IntrinsicKind::Number)
        )
    )
}

fn resolve_concrete_conditional_branch(
    db: &dyn TypeDatabase,
    cond: &crate::types::ConditionalType,
) -> Option<TypeId> {
    resolve_concrete_conditional_result(db, cond, cond.check_type)
}

fn resolve_concrete_conditional_result(
    db: &dyn TypeDatabase,
    cond: &crate::types::ConditionalType,
    check_input: TypeId,
) -> Option<TypeId> {
    let check_type = crate::evaluation::evaluate::evaluate_type(db, check_input);
    let extends_type = crate::evaluation::evaluate::evaluate_type(db, cond.extends_type);

    if let Some(TypeData::Union(members)) = db.lookup(check_type) {
        let members = db.type_list(members);
        let mut results = Vec::new();
        for &member in members.iter() {
            results.push(resolve_concrete_conditional_result(db, cond, member)?);
        }
        return Some(crate::utils::union_or_single(db, results));
    }

    if contains_type_parameters_db(db, check_type)
        || matches!(check_type, TypeId::ANY | TypeId::UNKNOWN | TypeId::ERROR)
        || matches!(extends_type, TypeId::ANY | TypeId::UNKNOWN | TypeId::ERROR)
    {
        return None;
    }

    if let Some(TypeData::StringIntrinsic { kind, type_arg }) = db.lookup(extends_type)
        && type_arg == TypeId::STRING
    {
        let transformed =
            crate::evaluation::evaluate::evaluate_type(db, db.string_intrinsic(kind, check_type));
        return Some(if transformed == check_type {
            cond.true_type
        } else {
            cond.false_type
        });
    }

    if contains_type_parameters_db(db, extends_type)
        && !contains_type_parameters_db(db, cond.check_type)
    {
        let evaluator = TypeEvaluator::new(db);
        if evaluator.type_contains_infer(cond.extends_type) {
            let mut bindings = rustc_hash::FxHashMap::default();
            let mut visited = FxHashSet::default();
            let mut checker = SubtypeChecker::new(db);
            if evaluator.match_infer_pattern(
                check_type,
                cond.extends_type,
                &mut bindings,
                &mut visited,
                &mut checker,
            ) {
                let substituted = evaluator.substitute_infer(cond.true_type, &bindings);
                let evaluated = crate::evaluation::evaluate::evaluate_type(db, substituted);
                return Some(evaluated);
            }
            return Some(cond.false_type);
        }
        return None;
    }

    Some(if crate::is_subtype_of(db, check_type, extends_type) {
        cond.true_type
    } else {
        cond.false_type
    })
}

fn remap_mapped_property_key(
    db: &dyn TypeDatabase,
    mapped: &crate::types::MappedType,
    source_key: TypeId,
) -> TypeId {
    use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};

    let Some(name_type) = mapped.name_type else {
        return source_key;
    };

    let mut subst = TypeSubstitution::new();
    subst.insert(mapped.type_param.name, source_key);
    crate::evaluation::evaluate::evaluate_type(db, instantiate_type(db, name_type, &subst))
}

fn add_mapped_property_optional_undefined(
    db: &dyn TypeDatabase,
    mapped: &crate::types::MappedType,
    value_type: TypeId,
) -> TypeId {
    if mapped.optional_modifier == Some(MappedModifier::Add) {
        db.union(vec![value_type, TypeId::UNDEFINED])
    } else {
        value_type
    }
}

fn specialize_mapped_property_value_type_for_key(
    db: &dyn TypeDatabase,
    value_type: TypeId,
    key_literal: TypeId,
) -> TypeId {
    let value_type = crate::evaluation::evaluate::evaluate_type(db, value_type);
    match db.lookup(value_type) {
        Some(TypeData::Application(app_id)) => {
            let app = db.type_application(app_id);
            let args: Vec<_> = app
                .args
                .iter()
                .map(|&arg| specialize_mapped_property_value_type_for_key(db, arg, key_literal))
                .collect();
            if args == app.args {
                value_type
            } else {
                db.application(app.base, args)
            }
        }
        Some(TypeData::Function(shape_id)) => {
            let shape = db.function_shape(shape_id);
            let params: Vec<_> = shape
                .params
                .iter()
                .map(|param| crate::ParamInfo {
                    type_id: specialize_mapped_property_value_type_for_key(
                        db,
                        param.type_id,
                        key_literal,
                    ),
                    ..param.clone()
                })
                .collect();
            let return_type =
                specialize_mapped_property_value_type_for_key(db, shape.return_type, key_literal);
            if params.iter().zip(shape.params.iter()).all(|(a, b)| a == b)
                && return_type == shape.return_type
            {
                value_type
            } else {
                db.function(crate::FunctionShape {
                    type_params: shape.type_params.clone(),
                    params,
                    this_type: shape.this_type,
                    return_type,
                    type_predicate: shape.type_predicate.clone(),
                    is_constructor: shape.is_constructor,
                    is_method: shape.is_method,
                })
            }
        }
        Some(TypeData::Union(_)) => {
            if let Some(narrowed) =
                narrow_union_by_literal_discriminant_property(db, value_type, key_literal)
            {
                return narrowed;
            }
            value_type
        }
        _ => value_type,
    }
}

fn narrow_union_by_literal_discriminant_property(
    db: &dyn TypeDatabase,
    union_type: TypeId,
    key_literal: TypeId,
) -> Option<TypeId> {
    let TypeData::Union(list_id) = db.lookup(union_type)? else {
        return None;
    };
    let members = db.type_list(list_id);
    let mut candidate_props = FxHashSet::default();

    for &member in members.iter() {
        let Some(shape) = get_object_shape(db, member) else {
            continue;
        };
        for prop in &shape.properties {
            if prop.type_id == key_literal {
                candidate_props.insert(prop.name);
            }
        }
    }

    for prop_name in candidate_props {
        let retained: Vec<_> = members
            .iter()
            .copied()
            .filter(|member| {
                get_object_shape(db, *member).is_some_and(|shape| {
                    shape
                        .properties
                        .iter()
                        .find(|prop| prop.name == prop_name)
                        .is_some_and(|prop| prop.type_id == key_literal)
                })
            })
            .collect();
        if retained.is_empty() || retained.len() == members.len() {
            continue;
        }
        return Some(if retained.len() == 1 {
            retained[0]
        } else {
            db.union_preserve_members(retained)
        });
    }

    None
}

fn collect_mapped_property_names_from_source_keys(
    db: &dyn TypeDatabase,
    mapped: &crate::types::MappedType,
    source_keys: FxHashSet<Atom>,
) -> Option<FxHashSet<Atom>> {
    let mut property_names = FxHashSet::default();

    for source_key in source_keys {
        let key_literal = property_key_atom_to_type(db, source_key);
        let mapped_key = remap_mapped_property_key(db, mapped, key_literal);
        let mapped_names = collect_exact_literal_property_keys(db, mapped_key)?;
        property_names.extend(mapped_names);
    }

    Some(property_names)
}

/// Collect exact property names for a mapped type when its key constraint can be reduced
/// to a finite set of literal property keys.
pub fn collect_finite_mapped_property_names(
    db: &dyn TypeDatabase,
    mapped_id: crate::types::MappedTypeId,
) -> Option<FxHashSet<Atom>> {
    let mapped = db.mapped_type(mapped_id);
    let source_keys = collect_exact_literal_property_keys(db, mapped.constraint)?;
    collect_mapped_property_names_from_source_keys(db, &mapped, source_keys)
}

/// Resolve the exact property type for a property on a mapped type when its key
/// constraint is a finite literal set.
pub fn get_finite_mapped_property_type(
    db: &dyn TypeDatabase,
    mapped_id: crate::types::MappedTypeId,
    property_name: &str,
) -> Option<TypeId> {
    let mapped = db.mapped_type(mapped_id);
    let source_keys = collect_exact_literal_property_keys(db, mapped.constraint)?;
    let target_atom = db.intern_string(property_name);
    let mut matches = Vec::new();

    for source_key in source_keys {
        let key_literal = property_key_atom_to_type(db, source_key);
        let remapped = remap_mapped_property_key(db, &mapped, key_literal);
        let remapped_keys = collect_exact_literal_property_keys(db, remapped)?;
        if !remapped_keys.contains(&target_atom) {
            continue;
        }

        let instantiated = instantiate_mapped_template_for_property(
            db,
            mapped.template,
            mapped.type_param.name,
            key_literal,
        );
        let value_type = specialize_mapped_property_value_type_for_key(
            db,
            crate::evaluation::evaluate::evaluate_type(db, instantiated),
            key_literal,
        );
        matches.push(add_mapped_property_optional_undefined(
            db, &mapped, value_type,
        ));
    }

    match matches.len() {
        0 => None,
        1 => Some(matches[0]),
        _ => Some(db.union_preserve_members(matches)),
    }
}

fn property_key_atom_to_type(db: &dyn TypeDatabase, key: Atom) -> TypeId {
    let key_str = db.resolve_atom(key);
    if let Some(symbol_ref) = key_str.strip_prefix("__unique_")
        && let Ok(id) = symbol_ref.parse::<u32>()
    {
        return db.unique_symbol(crate::types::SymbolRef(id));
    }
    db.literal_string(key_str.as_ref())
}

/// Backward-compatible alias for callers that only used this on deferred/remapped mapped types.
pub fn collect_deferred_mapped_property_names(
    db: &dyn TypeDatabase,
    mapped_id: crate::types::MappedTypeId,
) -> Option<FxHashSet<Atom>> {
    collect_finite_mapped_property_names(db, mapped_id)
}

/// Backward-compatible alias for callers that only used this on deferred/remapped mapped types.
pub fn get_deferred_mapped_property_type(
    db: &dyn TypeDatabase,
    mapped_id: crate::types::MappedTypeId,
    property_name: &str,
) -> Option<TypeId> {
    get_finite_mapped_property_type(db, mapped_id, property_name)
}

/// Find the private brand name for a type.
///
/// Private members in TypeScript classes use a "brand" property for nominal typing.
/// The brand is a property named like `__private_brand_#className`.
///
/// Returns the full brand property name (e.g., `"__private_brand_#Foo"`) if found,
/// or None if the type has no private brand.
pub fn get_private_brand_name(db: &dyn TypeDatabase, type_id: TypeId) -> Option<String> {
    match db.lookup(type_id)? {
        TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
            let shape = db.object_shape(shape_id);
            for prop in &shape.properties {
                let name = db.resolve_atom(prop.name);
                if name.starts_with("__private_brand_") {
                    return Some(name);
                }
            }
            None
        }
        TypeData::Callable(shape_id) => {
            let shape = db.callable_shape(shape_id);
            for prop in &shape.properties {
                let name = db.resolve_atom(prop.name);
                if name.starts_with("__private_brand_") {
                    return Some(name);
                }
            }
            None
        }
        _ => None,
    }
}

/// Find the private field name from a type's properties.
///
/// Given a type with private members, returns the name of the first private field
/// (a property starting with `#` that is not a brand marker).
///
/// Returns `Some(field_name)` (e.g., `"#foo"`) if found, None otherwise.
pub fn get_private_field_name(db: &dyn TypeDatabase, type_id: TypeId) -> Option<String> {
    match db.lookup(type_id)? {
        TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
            let shape = db.object_shape(shape_id);
            for prop in &shape.properties {
                let name = db.resolve_atom(prop.name);
                if name.starts_with('#') && !name.starts_with("__private_brand_") {
                    return Some(name);
                }
            }
            None
        }
        TypeData::Callable(shape_id) => {
            let shape = db.callable_shape(shape_id);
            for prop in &shape.properties {
                let name = db.resolve_atom(prop.name);
                if name.starts_with('#') && !name.starts_with("__private_brand_") {
                    return Some(name);
                }
            }
            None
        }
        _ => None,
    }
}

/// Get the symbol associated with a type's shape.
///
/// Checks object, object-with-index, and callable shapes for their `symbol` field.
/// Returns the first `SymbolId` found, or None if the type has no shape with a symbol.
pub fn get_type_shape_symbol(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<tsz_binder::SymbolId> {
    match db.lookup(type_id)? {
        TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
            db.object_shape(shape_id).symbol
        }
        TypeData::Callable(shape_id) => db.callable_shape(shape_id).symbol,
        _ => None,
    }
}

/// Get the `DefId` from an Enum type.
///
/// Returns None if the type is not an Enum type.
pub fn get_enum_def_id(db: &dyn TypeDatabase, type_id: TypeId) -> Option<crate::def::DefId> {
    match db.lookup(type_id) {
        Some(TypeData::Enum(def_id, _)) => Some(def_id),
        _ => None,
    }
}

/// Get the structural member type from an Enum type.
///
/// Returns None if the type is not an Enum type.
pub fn get_enum_member_type(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    match db.lookup(type_id) {
        Some(TypeData::Enum(_, member_type)) => Some(member_type),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TypeInterner;
    use crate::types::{CallSignature, CallableShape, ParamInfo, TypeParamInfo};

    fn make_callable_with_construct_sig(
        interner: &TypeInterner,
        return_type: TypeId,
        type_params: Vec<TypeParamInfo>,
    ) -> TypeId {
        let shape = CallableShape {
            call_signatures: vec![],
            construct_signatures: vec![CallSignature {
                type_params,
                params: vec![ParamInfo {
                    name: None,
                    type_id: TypeId::STRING,
                    optional: false,
                    rest: false,
                }],
                this_type: None,
                return_type,
                type_predicate: None,
                is_method: false,
            }],
            properties: vec![],
            string_index: None,
            number_index: None,
            symbol: None,
            is_abstract: false,
        };
        interner.callable(shape)
    }

    fn make_callable_with_call_sig(interner: &TypeInterner, return_type: TypeId) -> TypeId {
        let shape = CallableShape {
            call_signatures: vec![CallSignature {
                type_params: vec![],
                params: vec![ParamInfo {
                    name: None,
                    type_id: TypeId::NUMBER,
                    optional: false,
                    rest: false,
                }],
                this_type: None,
                return_type,
                type_predicate: None,
                is_method: false,
            }],
            construct_signatures: vec![],
            properties: vec![],
            string_index: None,
            number_index: None,
            symbol: None,
            is_abstract: false,
        };
        interner.callable(shape)
    }

    #[test]
    fn get_construct_signatures_direct_callable() {
        let interner = TypeInterner::new();
        let callable = make_callable_with_construct_sig(&interner, TypeId::STRING, vec![]);
        let sigs = get_construct_signatures(&interner, callable);
        assert!(sigs.is_some());
        assert_eq!(sigs.unwrap().len(), 1);
    }

    #[test]
    fn get_construct_signatures_intersection_collects_from_members() {
        let interner = TypeInterner::new();
        // Create two callables with construct signatures
        let ctor1 = make_callable_with_construct_sig(&interner, TypeId::STRING, vec![]);
        let ctor2 = make_callable_with_construct_sig(&interner, TypeId::NUMBER, vec![]);
        // Create intersection: ctor1 & ctor2
        let intersection = interner.intersection(vec![ctor1, ctor2]);
        let sigs = get_construct_signatures(&interner, intersection);
        assert!(sigs.is_some());
        let sigs = sigs.unwrap();
        assert_eq!(
            sigs.len(),
            2,
            "Should collect construct sigs from both members"
        );
    }

    #[test]
    fn get_construct_signatures_intersection_with_non_callable_member() {
        let interner = TypeInterner::new();
        // Create intersection: Constructor & { prop: string }
        let ctor = make_callable_with_construct_sig(&interner, TypeId::STRING, vec![]);
        let obj = interner.object(vec![]); // plain object, no construct sigs
        let intersection = interner.intersection(vec![ctor, obj]);
        let sigs = get_construct_signatures(&interner, intersection);
        assert!(sigs.is_some());
        assert_eq!(
            sigs.unwrap().len(),
            1,
            "Should find construct sig from callable member"
        );
    }

    #[test]
    fn get_construct_signatures_intersection_no_construct_sigs() {
        let interner = TypeInterner::new();
        // Intersection of non-callable types
        let intersection = interner.intersection(vec![TypeId::STRING, TypeId::NUMBER]);
        let sigs = get_construct_signatures(&interner, intersection);
        assert!(sigs.is_none());
    }

    #[test]
    fn get_call_signatures_intersection_collects_from_members() {
        let interner = TypeInterner::new();
        let fn1 = make_callable_with_call_sig(&interner, TypeId::STRING);
        let fn2 = make_callable_with_call_sig(&interner, TypeId::NUMBER);
        let intersection = interner.intersection(vec![fn1, fn2]);
        let sigs = get_call_signatures(&interner, intersection);
        assert!(sigs.is_some());
        let sigs = sigs.unwrap();
        assert_eq!(sigs.len(), 2, "Should collect call sigs from both members");
    }

    #[test]
    fn get_call_signatures_intersection_no_call_sigs() {
        let interner = TypeInterner::new();
        let intersection = interner.intersection(vec![TypeId::STRING, TypeId::NUMBER]);
        let sigs = get_call_signatures(&interner, intersection);
        assert!(sigs.is_none());
    }

    #[test]
    fn construct_sig_with_application_return_type_is_extractable() {
        // Simulates the JSX class component scenario where:
        // interface ComponentClass<P> { new(props: P): Component<P, any>; }
        // interface TestClass extends ComponentClass<{reqd: any}> {}
        //
        // The construct signature return type is Application(Component, [props, any])
        // which needs evaluation. The checker should evaluate it before bailing out.
        let interner = TypeInterner::new();

        // Create an Application type (simulating Component<{reqd: any}, any>)
        let inner_obj = interner.object(vec![]);
        let app_type = interner.application(inner_obj, vec![TypeId::STRING, TypeId::ANY]);

        // Create a callable with construct sig returning the Application type
        let callable = make_callable_with_construct_sig(&interner, app_type, vec![]);

        // Verify we CAN extract construct signatures
        let sigs = get_construct_signatures(&interner, callable);
        assert!(sigs.is_some(), "Should extract construct signatures");
        let sigs = sigs.unwrap();
        assert_eq!(sigs.len(), 1);

        // The return type IS an Application (needs evaluation)
        let return_type = sigs[0].return_type;
        assert!(
            crate::type_queries::needs_evaluation_for_merge(&interner, return_type),
            "Application return type needs evaluation"
        );

        // But the type itself does NOT contain type parameters
        // (all args are concrete: STRING, ANY)
        assert!(
            !crate::contains_type_parameters(&interner, return_type),
            "Concrete application should not contain type parameters"
        );
    }
}
