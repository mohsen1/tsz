//! Type Content Queries and Data Extraction Helpers
//!
//! This module provides functions for extracting type data and checking type content.
//! These functions abstract away the internal `TypeData` representation and provide
//! a stable API for querying type properties without matching on `TypeData` directly.

use super::traversal::collect_property_name_atoms_for_diagnostics;
use crate::TypeDatabase;
use crate::evaluation::evaluate::TypeEvaluator;
use crate::relations::subtype::SubtypeChecker;
use crate::types::{IntrinsicKind, LiteralValue, MappedModifier, PropertyInfo, TypeData, TypeId};
use crate::visitors::visitor_predicates::contains_type_matching;
use rustc_hash::{FxHashMap, FxHashSet};
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
    // Fast path: intrinsic types never contain type parameters
    if type_id.is_intrinsic() {
        return false;
    }
    // Fast path: check top-level type directly before creating ContainsTypeChecker
    match db.lookup(type_id) {
        Some(
            TypeData::TypeParameter(_)
            | TypeData::Infer(_)
            | TypeData::ThisType
            | TypeData::BoundParameter(_),
        ) => return true,
        Some(
            TypeData::Literal(_)
            | TypeData::Intrinsic(_)
            | TypeData::Error
            | TypeData::UniqueSymbol(_)
            | TypeData::ModuleNamespace(_)
            | TypeData::Recursive(_)
            | TypeData::Enum(_, _),
        ) => return false,
        _ => {}
    }
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

/// Check if a type is directly an `Infer` type (not recursive).
///
/// This is a lightweight O(1) check that only inspects the top-level type.
/// Use this when you need to guard against caching leaked Infer results
/// without the cost of a full recursive walk.
pub fn is_infer_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(db.lookup(type_id), Some(TypeData::Infer(_)))
}

/// Check if a type contains any `infer` types.
///
/// Delegates to `visitor_predicates::contains_type_matching` with an `Infer`-only
/// predicate.
pub fn contains_infer_types_db(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    // Fast path: intrinsic types never contain infer
    if type_id.is_intrinsic() {
        return false;
    }
    // Fast path: leaf types (Literal, Object, Function, etc.) that don't
    // contain nested types can't contain Infer. Only composite types
    // (Union, Intersection, Application, etc.) need traversal.
    match db.lookup(type_id) {
        Some(TypeData::Infer(_)) => return true,
        Some(TypeData::TypeParameter(tp)) => {
            let name = db.resolve_atom_ref(tp.name);
            return name.starts_with("__infer_") || name.starts_with("__infer_src_");
        }
        Some(
            TypeData::Literal(_)
            | TypeData::Intrinsic(_)
            | TypeData::Error
            | TypeData::ThisType
            | TypeData::UniqueSymbol(_)
            | TypeData::ModuleNamespace(_)
            | TypeData::BoundParameter(_)
            | TypeData::Recursive(_),
        ) => return false,
        _ => {}
    }
    contains_type_matching(db, type_id, |key| match key {
        TypeData::Infer(_) => true,
        TypeData::TypeParameter(tp) => {
            let name = db.resolve_atom_ref(tp.name);
            name.starts_with("__infer_") || name.starts_with("__infer_src_")
        }
        _ => false,
    })
}

/// Check if a type contains any unresolved `TypeQuery` references.
///
/// `TypeQuery` types represent `typeof X` that haven't been resolved to concrete types yet.
/// Evaluation results containing unresolved `TypeQuery` refs should not be cached, as the
/// `TypeQuery` may resolve to a different type once the referenced symbol's type is available
/// in the `TypeEnvironment`.
pub fn contains_type_query_db(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    match db.lookup(type_id) {
        Some(TypeData::TypeQuery(_)) => return true,
        Some(
            TypeData::Literal(_)
            | TypeData::Intrinsic(_)
            | TypeData::Error
            | TypeData::ThisType
            | TypeData::UniqueSymbol(_)
            | TypeData::ModuleNamespace(_)
            | TypeData::BoundParameter(_)
            | TypeData::Recursive(_),
        ) => return false,
        _ => {}
    }
    contains_type_matching(db, type_id, |key| matches!(key, TypeData::TypeQuery(_)))
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
            let name = db.resolve_atom_ref(tp.name);
            !(name.starts_with("__infer_") || name.starts_with("__infer_src_"))
        }
        TypeData::Infer(_) | TypeData::ThisType | TypeData::BoundParameter(_) => true,
        _ => false,
    })
}

/// Check if a type contains any lazy or recursive references.
///
/// This is used by checker query boundaries that need to reason about deferred
/// or cyclic types without matching on `TypeData` directly.
pub fn contains_lazy_or_recursive_db(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    contains_type_matching(db, type_id, |key| {
        matches!(key, TypeData::Lazy(_) | TypeData::Recursive(_))
    })
}

/// Check whether a type is itself a bare unresolved infer placeholder, not a
/// larger structural type that merely contains placeholders.
pub fn is_bare_infer_placeholder_db(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    match db.lookup(type_id) {
        Some(TypeData::Infer(_)) => true,
        Some(TypeData::TypeParameter(tp)) => {
            let name = db.resolve_atom_ref(tp.name);
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
    // Fast path: intrinsic and leaf types can't contain Error
    if type_id.is_intrinsic() {
        return false;
    }
    if matches!(
        db.lookup(type_id),
        Some(
            TypeData::Literal(_)
                | TypeData::TypeParameter(_)
                | TypeData::Infer(_)
                | TypeData::ThisType
                | TypeData::UniqueSymbol(_)
                | TypeData::ModuleNamespace(_)
                | TypeData::BoundParameter(_)
                | TypeData::Recursive(_)
        )
    ) {
        return false;
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
        Some(TypeData::TypeParameter(info) | TypeData::Infer(info)) => info
            .constraint
            .and_then(|constraint| get_array_element_type(db, constraint)),
        Some(TypeData::Application(_) | TypeData::Lazy(_)) => {
            let evaluated = crate::evaluation::evaluate::evaluate_type(db, type_id);
            (evaluated != type_id)
                .then(|| get_array_element_type(db, evaluated))
                .flatten()
        }
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
        Some(TypeData::TypeParameter(info) | TypeData::Infer(info)) => info
            .constraint
            .and_then(|constraint| get_tuple_elements(db, constraint)),
        Some(TypeData::Application(_) | TypeData::Lazy(_)) => {
            let evaluated = crate::evaluation::evaluate::evaluate_type(db, type_id);
            (evaluated != type_id)
                .then(|| get_tuple_elements(db, evaluated))
                .flatten()
        }
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

/// Check if a union type has a direct `TypeParameter` or Infer member (not nested).
///
/// Returns true for `string | T` or `number | infer U`, false for
/// `string | MyInterface` even if `MyInterface` contains type parameters internally.
/// Used to suppress diagnostics when generic type parameters are directly present.
pub fn union_has_direct_type_parameter(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    match db.lookup(type_id) {
        Some(TypeData::Union(list_id)) => {
            let members = db.type_list(list_id);
            members.iter().any(|&m| {
                matches!(
                    db.lookup(m),
                    Some(TypeData::TypeParameter(_) | TypeData::Infer(_))
                )
            })
        }
        _ => false,
    }
}

/// Collect `TypeIds` of callable properties from an object type.
///
/// Iterates the object's named properties and returns those whose type is a
/// Function or Callable. Also includes the string index signature value type
/// if it's callable. Used for contextual typing of callback-bearing objects.
pub fn collect_callable_property_types(db: &dyn TypeDatabase, type_id: TypeId) -> Vec<TypeId> {
    let shape_id = match db.lookup(type_id) {
        Some(TypeData::Object(id) | TypeData::ObjectWithIndex(id)) => id,
        _ => return Vec::new(),
    };
    let shape = db.object_shape(shape_id);
    let mut result = Vec::new();
    for prop in &shape.properties {
        if is_callable_type(db, prop.type_id) {
            result.push(prop.type_id);
        }
    }
    if let Some(index) = &shape.string_index
        && is_callable_type(db, index.value_type)
    {
        result.push(index.value_type);
    }
    if let Some(index) = &shape.number_index
        && is_callable_type(db, index.value_type)
    {
        result.push(index.value_type);
    }
    result
}

/// Check if a type is a callable type (Function or Callable with call signatures).
fn is_callable_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    match db.lookup(type_id) {
        Some(TypeData::Function(_)) => true,
        Some(TypeData::Callable(id)) => !db.callable_shape(id).call_signatures.is_empty(),
        _ => false,
    }
}

/// Check if a type (or any union member) is constructor-like.
///
/// Returns true when the type has construct signatures (Callable with
/// `construct_signatures`) or is a constructor Function (`is_constructor`).
/// For union types, returns true if ANY member is constructor-like.
pub fn is_constructor_like_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if let Some(shape_id) = crate::visitor::callable_shape_id(db, type_id)
        && !db.callable_shape(shape_id).construct_signatures.is_empty()
    {
        return true;
    }
    if let Some(shape_id) = crate::visitor::function_shape_id(db, type_id)
        && db.function_shape(shape_id).is_constructor
    {
        return true;
    }
    if let Some(members) = get_union_members(db, type_id) {
        return members.iter().any(|&m| is_constructor_like_type(db, m));
    }
    false
}

/// Extract type parameters from a callable/function type for type argument checking.
///
/// For Function types: returns the function's type parameters directly.
/// For Callable types: finds the call signature whose type parameter arity
/// matches `type_arg_count`, or falls back to the first signature.
/// Returns empty if the type has no type parameters or if multiple overloads
/// match the arity (overload resolution handles those cases).
pub fn extract_type_params_for_call(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    type_arg_count: usize,
) -> Option<Vec<crate::types::TypeParamInfo>> {
    match db.lookup(type_id) {
        Some(TypeData::Function(shape_id)) => {
            let shape = db.function_shape(shape_id);
            Some(shape.type_params.clone())
        }
        Some(TypeData::Callable(shape_id)) => {
            let shape = db.callable_shape(shape_id);
            let matching: Vec<_> = shape
                .call_signatures
                .iter()
                .filter(|sig| {
                    let max = sig.type_params.len();
                    let min = sig
                        .type_params
                        .iter()
                        .filter(|tp| tp.default.is_none())
                        .count();
                    type_arg_count >= min && type_arg_count <= max
                })
                .collect();
            // Multiple overloads match → skip (overload resolution handles it)
            if matching.len() > 1 {
                return None;
            }
            if let Some(sig) = matching.first() {
                Some(sig.type_params.clone())
            } else {
                // Fall back to first signature for diagnostics
                Some(
                    shape
                        .call_signatures
                        .first()
                        .map(|sig| sig.type_params.clone())
                        .unwrap_or_default(),
                )
            }
        }
        _ => None,
    }
}

/// Get a `CallableShape` for any callable type (Function or Callable).
///
/// For Callable types: returns the shape directly.
/// For Function types: wraps the function as a single-signature callable.
/// Returns None for non-callable types.
///
/// This unifies the Function/Callable distinction so callers don't need
/// to handle both variants separately.
pub fn get_callable_shape_for_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<crate::types::CallableShape>> {
    if let Some(shape_id) = crate::visitor::callable_shape_id(db, type_id) {
        return Some(db.callable_shape(shape_id));
    }
    if let Some(shape_id) = crate::visitor::function_shape_id(db, type_id) {
        let func = db.function_shape(shape_id);
        return Some(std::sync::Arc::new(crate::types::CallableShape {
            call_signatures: vec![crate::types::CallSignature {
                type_params: func.type_params.clone(),
                params: func.params.clone(),
                this_type: func.this_type,
                return_type: func.return_type,
                type_predicate: func.type_predicate.clone(),
                is_method: func.is_method,
            }],
            construct_signatures: Vec::new(),
            properties: Vec::new(),
            string_index: None,
            number_index: None,
            symbol: None,
            is_abstract: false,
        }));
    }
    None
}

/// Get overload call signatures if a type has multiple call overloads.
///
/// Returns `Some(signatures)` when the type has more than one call signature
/// (overloaded function). Returns `None` for single-signature or non-callable types.
pub fn get_overload_call_signatures(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<Vec<crate::types::CallSignature>> {
    if let Some(shape_id) = crate::visitor::callable_shape_id(db, type_id) {
        let shape = db.callable_shape(shape_id);
        if shape.call_signatures.len() > 1 {
            return Some(shape.call_signatures.clone());
        }
    }
    None
}

/// Get the symbol associated with an object type's shape.
///
/// Returns the `SymbolId` from the `ObjectShape` for Object or `ObjectWithIndex`
/// types. Returns None for non-object types or objects without a symbol.
pub fn get_object_symbol(db: &dyn TypeDatabase, type_id: TypeId) -> Option<tsz_binder::SymbolId> {
    match db.lookup(type_id) {
        Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
            db.object_shape(shape_id).symbol
        }
        _ => None,
    }
}

/// Get the raw property type by name from an object shape.
///
/// Looks up a named property in an Object or `ObjectWithIndex` type and returns
/// its type. Does NOT use full property access resolution — returns the raw
/// declared type from the shape. Returns None if the type isn't an object or
/// the property doesn't exist.
pub fn get_raw_property_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    prop_name: tsz_common::Atom,
) -> Option<TypeId> {
    let shape_id = match db.lookup(type_id) {
        Some(TypeData::Object(id) | TypeData::ObjectWithIndex(id)) => id,
        _ => return None,
    };
    let shape = db.object_shape(shape_id);
    shape
        .properties
        .iter()
        .find(|p| p.name == prop_name)
        .map(|p| p.type_id)
}

/// Intersect all constructor return types with a base instance type.
///
/// For Callable types: intersects each construct signature's return type
/// with `base_type`. For Function constructors: intersects the return type.
/// Returns the original type unchanged if it has no construct signatures.
///
/// Used during class inheritance to merge derived constructor return types
/// with the base class instance type.
pub fn intersect_constructor_returns(
    db: &dyn crate::caches::db::QueryDatabase,
    ctor_type: TypeId,
    base_type: TypeId,
) -> TypeId {
    let factory = db.factory();
    if let Some(shape_id) = crate::visitor::callable_shape_id(db, ctor_type) {
        let shape = db.callable_shape(shape_id);
        if shape.construct_signatures.is_empty() {
            return ctor_type;
        }
        let mut new_shape = (*shape).clone();
        new_shape.construct_signatures = shape
            .construct_signatures
            .iter()
            .map(|sig| {
                let mut updated = sig.clone();
                updated.return_type = factory.intersection2(updated.return_type, base_type);
                updated
            })
            .collect();
        return factory.callable(new_shape);
    }
    if let Some(shape_id) = crate::visitor::function_shape_id(db, ctor_type) {
        let shape = db.function_shape(shape_id);
        if !shape.is_constructor {
            return ctor_type;
        }
        let mut new_shape = (*shape).clone();
        new_shape.return_type = factory.intersection2(new_shape.return_type, base_type);
        return factory.function(new_shape);
    }
    ctor_type
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
    let mut key_types: Vec<TypeId> = Vec::new();
    let mut has_symbol_key = false;
    for p in &shape.properties {
        if p.visibility != crate::Visibility::Public {
            continue;
        }
        let name = db.resolve_atom_ref(p.name);
        if name.starts_with("__private_brand_") {
            continue;
        }
        // Computed symbol properties (e.g., [Symbol.iterator]) contribute
        // `symbol` to keyof, not a string literal key.
        if name.starts_with('[') {
            has_symbol_key = true;
            continue;
        }
        key_types.push(db.literal_string_atom(p.name));
    }
    // Include `symbol` in keyof when the object has computed symbol properties.
    if has_symbol_key {
        key_types.push(TypeId::SYMBOL);
    }
    if key_types.is_empty() {
        return Some(TypeId::NEVER);
    }
    Some(crate::utils::union_or_single(db, key_types))
}

/// Detect intersections that should preserve a discriminated object-union shape
/// instead of being eagerly collapsed by downstream evaluators.
///
/// This matches the interner-side preservation rule used for intersections like
/// `{ v: T } & ({ v: A, a: string } | { v: B, b: string })`.
pub fn is_discriminated_object_intersection(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    let Some(members) = get_intersection_members(db, type_id) else {
        return false;
    };

    let mut candidate_names = FxHashSet::default();
    for &member in &members {
        if get_union_members(db, member).is_some() {
            continue;
        }
        let Some(shape) = get_object_shape(db, member) else {
            continue;
        };
        for prop in &shape.properties {
            candidate_names.insert(prop.name);
        }
    }

    if candidate_names.is_empty() {
        return false;
    }

    members.iter().copied().any(|member| {
        let Some(union_members) = get_union_members(db, member) else {
            return false;
        };
        if union_members.len() < 2 {
            return false;
        }

        candidate_names.iter().copied().any(|prop_name| {
            let mut seen = FxHashSet::default();
            for branch in &union_members {
                let Some(shape) = get_object_shape(db, *branch) else {
                    return false;
                };
                let Some(prop) = shape.properties.iter().find(|prop| prop.name == prop_name) else {
                    return false;
                };
                if !crate::type_queries::is_unit_type(db, prop.type_id) {
                    return false;
                }
                seen.insert(prop.type_id);
            }
            seen.len() > 1
        })
    })
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
        let mut unpacked = Vec::new();
        for elem in tuple_elements {
            if !elem.rest {
                unpacked.push(crate::types::ParamInfo {
                    name: elem.name,
                    type_id: elem.type_id,
                    optional: elem.optional,
                    rest: false,
                });
                continue;
            }

            let expansion = crate::utils::expand_tuple_rest(db, elem.type_id);
            for fixed in expansion.fixed {
                unpacked.push(crate::types::ParamInfo {
                    name: fixed.name,
                    type_id: fixed.type_id,
                    optional: fixed.optional,
                    rest: false,
                });
            }
            if let Some(variadic) = expansion.variadic {
                unpacked.push(crate::types::ParamInfo {
                    name: elem.name,
                    type_id: db.array(variadic),
                    optional: false,
                    rest: true,
                });
            }
            for tail in expansion.tail {
                unpacked.push(crate::types::ParamInfo {
                    name: tail.name,
                    type_id: tail.type_id,
                    optional: tail.optional,
                    rest: tail.rest,
                });
            }
        }
        unpacked
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
        return (value as u64).to_string();
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
        // For type parameters, check the constraint.
        // E.g., `T extends { abc: number }` — T.abc should resolve through the constraint.
        Some(TypeData::TypeParameter(info)) => {
            if let Some(constraint) = info.constraint {
                type_has_property_by_str(db, constraint, name)
            } else {
                false
            }
        }
        // Callable shapes (interfaces with call/construct signatures) also have properties
        Some(TypeData::Callable(shape_id)) => {
            let shape = db.callable_shape(shape_id);
            shape
                .properties
                .iter()
                .any(|p| db.resolve_atom_ref(p.name).as_ref() == name)
        }
        _ => false,
    }
}

/// Get the inner type of a `ReadonlyType` wrapper.
///
/// Returns `Some(inner)` if the type is `ReadonlyType(inner)`, otherwise `None`.
pub fn get_readonly_inner(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    match db.lookup(type_id) {
        Some(TypeData::ReadonlyType(inner)) => Some(inner),
        _ => None,
    }
}

/// Get the inner type of a `NoInfer` wrapper.
///
/// Returns `Some(inner)` if the type is `NoInfer(inner)`, otherwise `None`.
pub fn get_noinfer_inner(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    match db.lookup(type_id) {
        Some(TypeData::NoInfer(inner)) => Some(inner),
        _ => None,
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

/// Check if a type is a type parameter (`TypeParameter` or Infer).
pub fn is_type_parameter(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(
        db.lookup(type_id),
        Some(TypeData::TypeParameter(_) | TypeData::Infer(_))
    )
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

/// Get the construct return type from any type (Callable or Function constructor).
///
/// For Callable types, returns the union of all construct signature return types.
/// For Function types marked as constructors, returns the return type.
/// Returns None for non-constructable types.
pub fn construct_return_type_for_type(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    use crate::type_queries::extended_constructors::InstanceTypeKind;
    match crate::type_queries::classify_for_instance_type(db, type_id) {
        InstanceTypeKind::Callable(shape_id) => get_construct_return_type_union(db, shape_id),
        InstanceTypeKind::Function(shape_id) => {
            let shape = db.function_shape(shape_id);
            if shape.is_constructor {
                Some(shape.return_type)
            } else {
                None
            }
        }
        _ => None,
    }
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
        Some(TypeData::KeyOf(operand)) => {
            collect_exact_literal_property_keys_from_keyof_operand(db, operand, keys, visited)
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

pub fn collect_exact_literal_property_keys(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<FxHashSet<Atom>> {
    let mut keys = FxHashSet::default();
    let mut visited = FxHashSet::default();
    collect_exact_literal_property_keys_inner(db, type_id, &mut keys, &mut visited)?;
    Some(keys)
}

fn collect_exact_literal_property_keys_from_keyof_operand(
    db: &dyn TypeDatabase,
    operand: TypeId,
    keys: &mut FxHashSet<Atom>,
    visited: &mut FxHashSet<TypeId>,
) -> Option<()> {
    let evaluated_operand = crate::evaluation::evaluate::evaluate_type(db, operand);
    let operand = if evaluated_operand != operand {
        evaluated_operand
    } else {
        operand
    };

    match db.lookup(operand) {
        Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
            let shape = db.object_shape(shape_id);
            if shape.string_index.is_some() || shape.number_index.is_some() {
                return None;
            }
            for prop in &shape.properties {
                keys.insert(prop.name);
            }
            Some(())
        }
        Some(TypeData::Callable(shape_id)) => {
            let shape = db.callable_shape(shape_id);
            if shape.string_index.is_some() || shape.number_index.is_some() {
                return None;
            }
            for prop in &shape.properties {
                keys.insert(prop.name);
            }
            Some(())
        }
        Some(TypeData::Union(_members)) => {
            let narrowed_operand = prune_impossible_object_union_members(db, operand);
            let members = match db.lookup(narrowed_operand) {
                Some(TypeData::Union(members)) => db.type_list(members).to_vec(),
                _ => {
                    return collect_exact_literal_property_keys_from_keyof_operand(
                        db,
                        narrowed_operand,
                        keys,
                        visited,
                    );
                }
            };
            for member in members {
                collect_exact_literal_property_keys_from_keyof_operand(db, member, keys, visited)?;
            }
            Some(())
        }
        Some(TypeData::Intersection(members)) => {
            let members = db.type_list(members);
            let mut saw_precise_member = false;
            for (member_idx, &member) in members.iter().enumerate() {
                let narrowed_member = narrow_keyof_intersection_member_by_literal_discriminants(
                    db, member, &members, member_idx,
                );
                if collect_exact_literal_property_keys_from_keyof_operand(
                    db,
                    narrowed_member,
                    keys,
                    visited,
                )
                .is_some()
                {
                    saw_precise_member = true;
                    continue;
                }
                if intersection_member_preserves_literal_keys(db, narrowed_member) {
                    continue;
                }
                return None;
            }
            saw_precise_member.then_some(())
        }
        Some(TypeData::TypeParameter(info) | TypeData::Infer(info)) => {
            info.constraint.and_then(|constraint| {
                collect_exact_literal_property_keys_inner(db, constraint, keys, visited)
            })
        }
        Some(TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner)) => {
            collect_exact_literal_property_keys_from_keyof_operand(db, inner, keys, visited)
        }
        _ => {
            let atoms = collect_property_name_atoms_for_diagnostics(db, operand, 8);
            if atoms.is_empty() {
                None
            } else {
                for atom in atoms {
                    keys.insert(atom);
                }
                Some(())
            }
        }
    }
}

pub(crate) fn narrow_keyof_intersection_member_by_literal_discriminants(
    db: &dyn TypeDatabase,
    member: TypeId,
    intersection_members: &[TypeId],
    member_idx: usize,
) -> TypeId {
    let evaluated_member = crate::evaluation::evaluate::evaluate_type(db, member);
    let member = if evaluated_member != member {
        evaluated_member
    } else {
        member
    };

    let Some(TypeData::Union(list_id)) = db.lookup(member) else {
        return member;
    };

    let mut discriminants = Vec::new();
    for (other_idx, &other_member) in intersection_members.iter().enumerate() {
        if other_idx == member_idx {
            continue;
        }
        let evaluated_other = crate::evaluation::evaluate::evaluate_type(db, other_member);
        let other_member = if evaluated_other != other_member {
            evaluated_other
        } else {
            other_member
        };
        let Some(shape) = get_object_shape(db, other_member) else {
            continue;
        };
        for prop in &shape.properties {
            if crate::type_queries::is_unit_type(db, prop.type_id) {
                discriminants.push((prop.name, prop.type_id));
            }
        }
    }

    if discriminants.is_empty() {
        return member;
    }

    let union_members = db.type_list(list_id);
    let retained: Vec<_> = union_members
        .iter()
        .copied()
        .filter(|&branch| {
            let Some(shape) = get_object_shape(db, branch) else {
                return true;
            };

            discriminants.iter().all(|&(disc_name, disc_type)| {
                let Some(prop) = shape.properties.iter().find(|prop| prop.name == disc_name) else {
                    return true;
                };
                !crate::type_queries::is_unit_type(db, prop.type_id)
                    || crate::is_subtype_of(db, disc_type, prop.type_id)
            })
        })
        .collect();

    if retained.is_empty() || retained.len() == union_members.len() {
        member
    } else if retained.len() == 1 {
        retained[0]
    } else {
        db.union_preserve_members(retained)
    }
}

fn intersection_has_impossible_literal_discriminants(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> bool {
    let Some(TypeData::Intersection(list_id)) = db.lookup(type_id) else {
        return false;
    };

    let mut discriminants: FxHashMap<Atom, Vec<TypeId>> = FxHashMap::default();
    for &member in db.type_list(list_id).iter() {
        let evaluated_member = crate::evaluation::evaluate::evaluate_type(db, member);
        let member = if evaluated_member != member {
            evaluated_member
        } else {
            member
        };
        let Some(shape) = get_object_shape(db, member) else {
            continue;
        };

        for prop in &shape.properties {
            if !crate::type_queries::is_unit_type(db, prop.type_id) {
                continue;
            }

            let seen = discriminants.entry(prop.name).or_default();
            if seen.iter().any(|&other| {
                !crate::is_subtype_of(db, prop.type_id, other)
                    && !crate::is_subtype_of(db, other, prop.type_id)
            }) {
                return true;
            }
            if !seen.contains(&prop.type_id) {
                seen.push(prop.type_id);
            }
        }
    }

    false
}

fn object_member_has_impossible_required_property(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    let evaluated_type = crate::evaluation::evaluate::evaluate_type(db, type_id);
    let type_id = if evaluated_type != type_id {
        evaluated_type
    } else {
        type_id
    };
    let Some(shape) = get_object_shape(db, type_id) else {
        return false;
    };

    shape.properties.iter().any(|prop| {
        !prop.optional
            && (crate::evaluation::evaluate::evaluate_type(db, prop.type_id) == TypeId::NEVER
                || unit_intersection_is_impossible(db, prop.type_id))
    })
}

fn unit_intersection_is_impossible(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    let evaluated = crate::evaluation::evaluate::evaluate_type(db, type_id);
    let type_id = if evaluated != type_id {
        evaluated
    } else {
        type_id
    };
    let Some(TypeData::Intersection(list_id)) = db.lookup(type_id) else {
        return false;
    };

    let mut units = Vec::new();
    for &member in db.type_list(list_id).iter() {
        let evaluated_member = crate::evaluation::evaluate::evaluate_type(db, member);
        let member = if evaluated_member != member {
            evaluated_member
        } else {
            member
        };
        if !crate::type_queries::is_unit_type(db, member) {
            continue;
        }
        if units.iter().any(|&other| {
            !crate::is_subtype_of(db, member, other) && !crate::is_subtype_of(db, other, member)
        }) {
            return true;
        }
        if !units.contains(&member) {
            units.push(member);
        }
    }

    false
}

pub fn prune_impossible_object_union_members(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    let Some(TypeData::Union(list_id)) = db.lookup(type_id) else {
        return type_id;
    };

    let members = db.type_list(list_id);
    let retained: Vec<_> = members
        .iter()
        .copied()
        .filter(|&member| {
            !intersection_has_impossible_literal_discriminants(db, member)
                && !object_member_has_impossible_required_property(db, member)
        })
        .collect();

    match retained.len() {
        0 => TypeId::NEVER,
        len if len == members.len() => type_id,
        1 => retained[0],
        _ => db.union_preserve_members(retained),
    }
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
    let Some(name_type) = mapped.name_type else {
        return source_key;
    };

    use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};

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

// =============================================================================
// Mapped-Type Source Classification and Expansion Helpers
// =============================================================================

/// Classification of a mapped type's source for structural preservation decisions.
///
/// When a homomorphic mapped type maps over `keyof T`, this classifies what `T`
/// resolves to, so callers can decide whether to preserve array/tuple identity
/// or expand to a plain object.
#[derive(Debug, Clone, PartialEq)]
pub enum MappedSourceKind {
    /// Source is an array type (`T[]`) — preserve as array after mapping.
    Array(TypeId),
    /// Source is a tuple type — preserve as tuple after mapping.
    Tuple(crate::types::TupleListId),
    /// Source is a readonly array (`ObjectWithIndex` with readonly number index).
    ReadonlyArray(TypeId),
    /// Source is a regular object or other non-array/tuple type.
    Object,
    /// Source is a type parameter with an array/tuple constraint.
    TypeParamWithArrayConstraint(TypeId),
}

/// Classify a resolved mapped-type source for array/tuple preservation.
///
/// Given the resolved source type from a homomorphic mapped type's `keyof T`
/// constraint, returns the structural kind. The checker/boundary can use this
/// to decide whether to delegate to the solver's tuple/array mapped evaluation
/// or use the standard object expansion path.
pub fn classify_mapped_source(db: &dyn TypeDatabase, source: TypeId) -> MappedSourceKind {
    let evaluated = crate::evaluation::evaluate::evaluate_type(db, source);
    classify_mapped_source_inner(db, evaluated)
}

fn classify_mapped_source_inner(db: &dyn TypeDatabase, source: TypeId) -> MappedSourceKind {
    match db.lookup(source) {
        Some(TypeData::Array(element_type)) => MappedSourceKind::Array(element_type),
        Some(TypeData::Tuple(tuple_id)) => MappedSourceKind::Tuple(tuple_id),
        Some(TypeData::ObjectWithIndex(shape_id)) => {
            let shape = db.object_shape(shape_id);
            if let Some(ref idx) = shape.number_index
                && idx.readonly
                && idx.key_type == TypeId::NUMBER
            {
                return MappedSourceKind::ReadonlyArray(idx.value_type);
            }
            MappedSourceKind::Object
        }
        Some(TypeData::TypeParameter(info)) => {
            if let Some(constraint) = info.constraint {
                let resolved = crate::evaluation::evaluate::evaluate_type(db, constraint);
                match classify_mapped_source_inner(db, resolved) {
                    MappedSourceKind::Object => MappedSourceKind::Object,
                    _ => MappedSourceKind::TypeParamWithArrayConstraint(constraint),
                }
            } else {
                MappedSourceKind::Object
            }
        }
        _ => MappedSourceKind::Object,
    }
}

/// Check if a mapped type's `as` clause is identity-preserving (no remapping).
///
/// Returns `true` when there's no `as` clause, or when the `as` clause maps
/// to the same type parameter (e.g., `{ [K in keyof T as K]: T[K] }`).
pub fn is_identity_name_mapping(db: &dyn TypeDatabase, mapped: &crate::types::MappedType) -> bool {
    match mapped.name_type {
        None => true,
        Some(nt) => matches!(
            db.lookup(nt),
            Some(TypeData::TypeParameter(param)) if param.name == mapped.type_param.name
        ),
    }
}

/// Info about an identity homomorphic mapped type `{ [K in keyof T]: T[K] }`.
///
/// Returned by [`classify_identity_mapped`] when a mapped type is confirmed to
/// be an identity mapping — constraint is `keyof T` where `T` is a type param,
/// and the template is `T[K]` where `K` is the mapped iteration variable.
#[derive(Clone, Debug)]
pub struct IdentityMappedInfo {
    /// Name of the source type parameter `T`.
    pub source_param_name: Atom,
    /// Constraint of the source type parameter (if any).
    pub source_constraint: Option<TypeId>,
}

/// Check if a mapped type is an identity homomorphic mapped type.
///
/// An identity mapped type has the form `{ [K in keyof T]: T[K] }` where
/// `T` is a type parameter and the template is an indexed access of `T` by `K`.
/// This is the pattern where `Partial<number>` evaluates to `number`.
///
/// Returns [`IdentityMappedInfo`] with the source type parameter's name and
/// constraint, or `None` if the mapped type is not identity-homomorphic.
pub fn classify_identity_mapped(
    db: &dyn TypeDatabase,
    mapped_id: crate::types::MappedTypeId,
) -> Option<IdentityMappedInfo> {
    let mapped = db.mapped_type(mapped_id);
    let keyof_source = crate::keyof_inner_type(db, mapped.constraint)?;
    let tp = crate::type_param_info(db, keyof_source)?;
    let (obj, key) = crate::index_access_parts(db, mapped.template)?;
    if obj != keyof_source {
        return None;
    }
    let kp = crate::type_param_info(db, key)?;
    if kp.name != mapped.type_param.name {
        return None;
    }
    Some(IdentityMappedInfo {
        source_param_name: tp.name,
        source_constraint: tp.constraint,
    })
}

/// Check if a mapped type's template is callable (has call/construct signatures).
///
/// This is used for TS2344 constraint checking: when an indexed access into a
/// mapped type (e.g., `{ [K in keyof T]: () => unknown }[keyof T]`) is checked
/// against a callable constraint, we need to know if the template type is callable.
pub fn is_mapped_template_callable(
    db: &dyn TypeDatabase,
    mapped_id: crate::types::MappedTypeId,
) -> bool {
    let mapped = db.mapped_type(mapped_id);
    super::is_callable_type(db, mapped.template)
        || get_callable_shape(db, mapped.template).is_some()
}

/// Get the inner type of a `keyof T` type, delegated from the visitor layer.
///
/// Returns `Some(T)` if the type is `KeyOf(T)`, `None` otherwise.
/// This is the boundary-safe version of `crate::keyof_inner_type`.
pub fn keyof_inner_type(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    crate::keyof_inner_type(db, type_id)
}

/// Check if a type is an array or tuple type.
///
/// Used for constraint classification in mapped type passthrough decisions:
/// when a type parameter is constrained to array/tuple, `any` arguments
/// should pass through identity mapped types rather than expanding.
pub fn is_array_or_tuple_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    crate::visitors::visitor_predicates::is_array_type(db, type_id)
        || crate::visitors::visitor_predicates::is_tuple_type(db, type_id)
}

/// Evaluate the passthrough result for an identity-homomorphic mapped type
/// applied to a primitive argument.
///
/// When a mapped type has the form `{ [K in keyof T]: T[K] }` (identity mapping)
/// and the type argument for `T` is a primitive type, tsc evaluates the result
/// directly without expanding the mapping:
///
/// - For concrete primitives (string, number, boolean, etc.): return the arg itself.
///   E.g., `Partial<number>` → `number`.
///
/// - For `any`/`unknown`/`never`/`error` with an array/tuple-constrained param:
///   return the arg. E.g., `Boxified<any>` where `T extends any[]` → `any`.
///
/// - For `any` with a non-array constraint: return an object with string+number
///   index signatures. E.g., `Objectish<any>` → `{ [x: string]: any; [x: number]: any }`.
///
/// Returns `Some(result)` if the passthrough applies, `None` otherwise.
/// The caller must supply the already-evaluated argument type.
pub fn evaluate_identity_mapped_passthrough(
    db: &dyn TypeDatabase,
    mapped_id: crate::types::MappedTypeId,
    arg: TypeId,
) -> Option<TypeId> {
    use crate::types::{IndexSignature, ObjectFlags, ObjectShape};

    // Check if this is an identity mapped type
    let identity_info = classify_identity_mapped(db, mapped_id)?;

    // Only applies to primitives (including any/unknown/never/error)
    if !crate::is_primitive_type(db, arg)
        && arg != TypeId::ANY
        && arg != TypeId::UNKNOWN
        && arg != TypeId::NEVER
        && arg != TypeId::ERROR
    {
        return None;
    }

    // For concrete primitives (not any/unknown/never/error), always passthrough
    if arg != TypeId::ANY && arg != TypeId::UNKNOWN && arg != TypeId::NEVER && arg != TypeId::ERROR
    {
        return Some(arg);
    }

    // For any/unknown/never/error: passthrough only when the type parameter is
    // constrained to array/tuple types.
    if let Some(constraint) = identity_info.source_constraint {
        let evaluated_constraint = crate::evaluation::evaluate::evaluate_type(db, constraint);
        if is_array_or_tuple_type(db, evaluated_constraint) {
            return Some(arg);
        }
    }

    // For `any` with no array/tuple constraint: produce { [x: string]: any; [x: number]: any }.
    // This matches tsc where `Objectish<any>` produces an object with index signatures,
    // NOT `any`. This ensures `Objectish<any>` is NOT assignable to `any[]`.
    if arg == TypeId::ANY {
        return Some(db.object_with_index(ObjectShape {
            flags: ObjectFlags::empty(),
            properties: vec![],
            string_index: Some(IndexSignature {
                key_type: TypeId::STRING,
                value_type: TypeId::ANY,
                readonly: false,
                param_name: None,
            }),
            number_index: Some(IndexSignature {
                key_type: TypeId::NUMBER,
                value_type: TypeId::ANY,
                readonly: false,
                param_name: None,
            }),
            symbol: None,
        }));
    }

    // For unknown/never/error without array constraint, no passthrough
    None
}

/// Reconstruct a mapped type with a new constraint, preserving all other fields.
///
/// Used when the checker evaluates a mapped type's constraint to concrete keys
/// and needs to create a new mapped type with the resolved constraint for
/// further evaluation (e.g., finite key collection).
///
/// Returns the `MappedTypeId` of the new (or interned-existing) mapped type.
pub fn reconstruct_mapped_with_constraint(
    db: &dyn TypeDatabase,
    mapped_id: crate::types::MappedTypeId,
    new_constraint: TypeId,
) -> crate::types::MappedTypeId {
    let mapped = db.mapped_type(mapped_id);
    if mapped.constraint == new_constraint {
        return mapped_id;
    }
    let new_mapped = crate::types::MappedType {
        type_param: mapped.type_param,
        constraint: new_constraint,
        name_type: mapped.name_type,
        template: mapped.template,
        readonly_modifier: mapped.readonly_modifier,
        optional_modifier: mapped.optional_modifier,
    };
    // Intern via the TypeDatabase factory and extract the MappedTypeId.
    let type_id = db.mapped(new_mapped);
    crate::mapped_type_id(db, type_id)
        .expect("freshly constructed mapped type should have MappedTypeId")
}

/// Compute modifier values for a mapped type property given the source property's
/// original modifiers and the mapped type's modifier directives.
///
/// This centralizes the `-?`, `+?`, `-readonly`, `+readonly` logic that was
/// previously duplicated between the solver's `evaluate_mapped` and the checker's
/// `evaluate_mapped_type_with_resolution_inner`.
pub const fn compute_mapped_modifiers(
    mapped: &crate::types::MappedType,
    is_homomorphic: bool,
    source_optional: bool,
    source_readonly: bool,
) -> (bool, bool) {
    let optional = match mapped.optional_modifier {
        Some(MappedModifier::Add) => true,
        Some(MappedModifier::Remove) => false,
        None => {
            if is_homomorphic {
                source_optional
            } else {
                false
            }
        }
    };
    let readonly = match mapped.readonly_modifier {
        Some(MappedModifier::Add) => true,
        Some(MappedModifier::Remove) => false,
        None => {
            if is_homomorphic {
                source_readonly
            } else {
                false
            }
        }
    };
    (optional, readonly)
}

/// Collect source property info from a homomorphic mapped type's source object.
///
/// For a mapped type `{ [K in keyof T]: ... }`, this resolves `T` and collects
/// its properties into a map of `(optional, readonly, declared_type)` tuples.
/// This is used by `expand_mapped_type_to_properties` to compute modifiers and
/// for `-?` to preserve the distinction between implicit and explicit undefined.
pub fn collect_homomorphic_source_properties(
    db: &dyn TypeDatabase,
    source: TypeId,
) -> FxHashMap<Atom, (bool, bool, TypeId)> {
    let evaluated = crate::evaluation::evaluate::evaluate_type(db, source);
    let mut props = FxHashMap::default();
    match db.lookup(evaluated) {
        Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
            let shape = db.object_shape(shape_id);
            props.reserve(shape.properties.len());
            for prop in &shape.properties {
                props.insert(prop.name, (prop.optional, prop.readonly, prop.type_id));
            }
        }
        Some(TypeData::Callable(shape_id)) => {
            let shape = db.callable_shape(shape_id);
            props.reserve(shape.properties.len());
            for prop in &shape.properties {
                props.insert(prop.name, (prop.optional, prop.readonly, prop.type_id));
            }
        }
        _ => {}
    }
    props
}

/// Expand a mapped type with resolved finite keys into a list of `PropertyInfo`.
///
/// This takes:
/// - `db`: type database
/// - `mapped`: the mapped type definition
/// - `string_keys`: pre-collected finite key atoms (already resolved from constraint)
/// - `source_props`: optional map of source property info for homomorphic types
///   (maps key atom → (optional, readonly, `declared_type`))
/// - `is_homomorphic`: whether this is a homomorphic mapped type (keyof T pattern)
///
/// Returns the expanded properties with correct modifiers and template instantiation.
/// Does NOT handle array/tuple preservation — callers should check `classify_mapped_source`
/// and use the solver's `evaluate_mapped_array`/`evaluate_mapped_tuple` for those cases.
pub fn expand_mapped_type_to_properties(
    db: &dyn TypeDatabase,
    mapped: &crate::types::MappedType,
    string_keys: &[Atom],
    source_props: &FxHashMap<Atom, (bool, bool, TypeId)>,
    is_homomorphic: bool,
) -> Vec<PropertyInfo> {
    use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};

    let is_remove_optional = mapped.optional_modifier == Some(MappedModifier::Remove);
    let mut properties = Vec::with_capacity(string_keys.len());
    let mut subst = TypeSubstitution::new();

    for &key_name in string_keys {
        let key_literal = db.literal_string_atom(key_name);

        // Handle name remapping
        let remapped = remap_mapped_property_key(db, mapped, key_literal);
        if remapped == TypeId::NEVER {
            continue;
        }

        // Extract property name(s) from remapped key
        let remapped_names: smallvec::SmallVec<[Atom; 1]> =
            if let Some(name) = crate::visitor::literal_string(db, remapped) {
                smallvec::smallvec![name]
            } else if let Some(TypeData::Union(list_id)) = db.lookup(remapped) {
                let members = db.type_list(list_id);
                let names: smallvec::SmallVec<[Atom; 1]> = members
                    .iter()
                    .filter_map(|&m| crate::visitor::literal_string(db, m))
                    .collect();
                if names.is_empty() {
                    continue;
                }
                names
            } else {
                // Can't resolve name — skip this key
                continue;
            };

        // Instantiate template with this key
        subst.clear();
        subst.insert(mapped.type_param.name, key_literal);
        let instantiated = instantiate_type(db, mapped.template, &subst);
        let mut property_type = crate::evaluation::evaluate::evaluate_type(db, instantiated);

        // Look up source property info for modifier computation
        let source_info = source_props.get(&key_name);
        let (source_optional, source_readonly) =
            source_info.map_or((false, false), |(opt, ro, _)| (*opt, *ro));

        let (optional, readonly) =
            compute_mapped_modifiers(mapped, is_homomorphic, source_optional, source_readonly);

        // For homomorphic mapped types with `-?` and optional source properties,
        // use the declared type (without implicit undefined from optionality).
        if is_homomorphic
            && is_remove_optional
            && source_optional
            && let Some((_, _, declared_type)) = source_info
        {
            property_type = *declared_type;
        } else if is_homomorphic
            && source_optional
            && let Some((_, _, declared_type)) = source_info
        {
            // For homomorphic types preserving optionality, use declared type
            // to avoid double-encoding undefined from indexed access.
            property_type = *declared_type;
        }

        for remapped_name in remapped_names {
            properties.push(PropertyInfo {
                name: remapped_name,
                type_id: property_type,
                write_type: property_type,
                optional,
                readonly,
                is_method: false,
                is_class_prototype: false,
                visibility: crate::types::Visibility::Public,
                parent_id: None,
                declaration_order: 0,
            });
        }
    }

    properties
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
    use crate::caches::db::QueryDatabase;
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
        let intersection = interner.intersection2(ctor1, ctor2);
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
        let intersection = interner.intersection2(ctor, obj);
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
        let intersection = interner.intersection2(TypeId::STRING, TypeId::NUMBER);
        let sigs = get_construct_signatures(&interner, intersection);
        assert!(sigs.is_none());
    }

    #[test]
    fn get_call_signatures_intersection_collects_from_members() {
        let interner = TypeInterner::new();
        let fn1 = make_callable_with_call_sig(&interner, TypeId::STRING);
        let fn2 = make_callable_with_call_sig(&interner, TypeId::NUMBER);
        let intersection = interner.intersection2(fn1, fn2);
        let sigs = get_call_signatures(&interner, intersection);
        assert!(sigs.is_some());
        let sigs = sigs.unwrap();
        assert_eq!(sigs.len(), 2, "Should collect call sigs from both members");
    }

    #[test]
    fn get_call_signatures_intersection_no_call_sigs() {
        let interner = TypeInterner::new();
        let intersection = interner.intersection2(TypeId::STRING, TypeId::NUMBER);
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

    #[test]
    fn test_union_has_direct_type_parameter() {
        let interner = crate::intern::TypeInterner::new();

        // Single type parameter
        let tp = interner.type_param(crate::types::TypeParamInfo {
            name: interner.intern_string("T"),
            constraint: None,
            default: None,
            is_const: false,
        });
        // Not a union — returns false
        assert!(!super::union_has_direct_type_parameter(&interner, tp));

        // Union containing a type parameter
        let union_with_tp = interner.union(vec![TypeId::STRING, tp]);
        assert!(super::union_has_direct_type_parameter(
            &interner,
            union_with_tp
        ));

        // Union without type parameters
        let plain_union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
        assert!(!super::union_has_direct_type_parameter(
            &interner,
            plain_union
        ));

        // Non-union type
        assert!(!super::union_has_direct_type_parameter(
            &interner,
            TypeId::STRING
        ));
    }

    #[test]
    fn test_collect_callable_property_types() {
        let interner = crate::intern::TypeInterner::new();
        use crate::types::{FunctionShape, PropertyInfo, Visibility};

        // Create a function type (callable property)
        let fn_type = interner.function(FunctionShape {
            type_params: vec![],
            params: vec![],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });

        // Create an object with one callable and one non-callable property
        let obj = interner.object(vec![
            PropertyInfo {
                name: interner.intern_string("callback"),
                type_id: fn_type,
                write_type: fn_type,
                optional: false,
                readonly: false,
                is_method: false,
                is_class_prototype: false,
                visibility: Visibility::Public,
                parent_id: None,
                declaration_order: 0,
            },
            PropertyInfo {
                name: interner.intern_string("value"),
                type_id: TypeId::STRING,
                write_type: TypeId::STRING,
                optional: false,
                readonly: false,
                is_method: false,
                is_class_prototype: false,
                visibility: Visibility::Public,
                parent_id: None,
                declaration_order: 1,
            },
        ]);

        let result = super::collect_callable_property_types(&interner, obj);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], fn_type);

        // Non-object type returns empty
        assert!(super::collect_callable_property_types(&interner, TypeId::STRING).is_empty());
    }

    #[test]
    fn test_construct_return_type_for_type() {
        let interner = crate::intern::TypeInterner::new();
        use crate::types::{CallSignature, CallableShape, FunctionShape};

        // Function constructor
        let fn_ctor = interner.function(FunctionShape {
            type_params: vec![],
            params: vec![],
            this_type: None,
            return_type: TypeId::STRING,
            type_predicate: None,
            is_constructor: true,
            is_method: false,
        });
        assert_eq!(
            super::construct_return_type_for_type(&interner, fn_ctor),
            Some(TypeId::STRING)
        );

        // Non-constructor function → None
        let fn_regular = interner.function(FunctionShape {
            type_params: vec![],
            params: vec![],
            this_type: None,
            return_type: TypeId::NUMBER,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });
        assert_eq!(
            super::construct_return_type_for_type(&interner, fn_regular),
            None
        );

        // Callable with construct signature
        let callable = interner.callable(CallableShape {
            call_signatures: vec![],
            construct_signatures: vec![CallSignature {
                type_params: vec![],
                params: vec![],
                this_type: None,
                return_type: TypeId::BOOLEAN,
                type_predicate: None,
                is_method: false,
            }],
            properties: vec![],
            string_index: None,
            number_index: None,
            symbol: None,
            is_abstract: false,
        });
        assert_eq!(
            super::construct_return_type_for_type(&interner, callable),
            Some(TypeId::BOOLEAN)
        );

        // Non-constructable type → None
        assert_eq!(
            super::construct_return_type_for_type(&interner, TypeId::STRING),
            None
        );
    }

    #[test]
    fn test_is_constructor_like_type() {
        let interner = crate::intern::TypeInterner::new();
        use crate::types::{CallSignature, CallableShape, FunctionShape};

        // Constructor function
        let fn_ctor = interner.function(FunctionShape {
            type_params: vec![],
            params: vec![],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            is_constructor: true,
            is_method: false,
        });
        assert!(super::is_constructor_like_type(&interner, fn_ctor));

        // Regular function — not constructor-like
        let fn_regular = interner.function(FunctionShape {
            type_params: vec![],
            params: vec![],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });
        assert!(!super::is_constructor_like_type(&interner, fn_regular));

        // Callable with construct signature
        let callable_ctor = interner.callable(CallableShape {
            call_signatures: vec![],
            construct_signatures: vec![CallSignature {
                type_params: vec![],
                params: vec![],
                this_type: None,
                return_type: TypeId::OBJECT,
                type_predicate: None,
                is_method: false,
            }],
            properties: vec![],
            string_index: None,
            number_index: None,
            symbol: None,
            is_abstract: false,
        });
        assert!(super::is_constructor_like_type(&interner, callable_ctor));

        // Union containing a constructor — should be constructor-like
        let union_with_ctor = interner.union(vec![TypeId::STRING, fn_ctor]);
        assert!(super::is_constructor_like_type(&interner, union_with_ctor));

        // Plain type — not constructor-like
        assert!(!super::is_constructor_like_type(&interner, TypeId::STRING));
    }

    #[test]
    fn test_extract_type_params_for_call() {
        let interner = crate::intern::TypeInterner::new();
        use crate::types::{FunctionShape, TypeParamInfo};

        let tp_t = TypeParamInfo {
            name: interner.intern_string("T"),
            constraint: None,
            default: None,
            is_const: false,
        };

        // Function with 1 type param
        let fn_generic = interner.function(FunctionShape {
            type_params: vec![tp_t],
            params: vec![],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });
        let result = super::extract_type_params_for_call(&interner, fn_generic, 1);
        assert!(result.is_some());
        assert_eq!(result.unwrap().len(), 1);

        // Non-callable type → None
        assert!(super::extract_type_params_for_call(&interner, TypeId::STRING, 0).is_none());
    }

    #[test]
    fn test_get_callable_shape_for_type() {
        let interner = crate::intern::TypeInterner::new();
        use crate::types::FunctionShape;

        // Function → wrapped as single-sig callable
        let fn_type = interner.function(FunctionShape {
            type_params: vec![],
            params: vec![],
            this_type: None,
            return_type: TypeId::STRING,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });
        let shape = super::get_callable_shape_for_type(&interner, fn_type);
        assert!(shape.is_some());
        let shape = shape.unwrap();
        assert_eq!(shape.call_signatures.len(), 1);
        assert_eq!(shape.call_signatures[0].return_type, TypeId::STRING);

        // Non-callable → None
        assert!(super::get_callable_shape_for_type(&interner, TypeId::NUMBER).is_none());
    }

    #[test]
    fn test_get_overload_call_signatures() {
        let interner = crate::intern::TypeInterner::new();
        use crate::types::{CallSignature, CallableShape};

        // Callable with 2 overloads → Some
        let overloaded = interner.callable(CallableShape {
            call_signatures: vec![
                CallSignature {
                    type_params: vec![],
                    params: vec![],
                    this_type: None,
                    return_type: TypeId::STRING,
                    type_predicate: None,
                    is_method: false,
                },
                CallSignature {
                    type_params: vec![],
                    params: vec![],
                    this_type: None,
                    return_type: TypeId::NUMBER,
                    type_predicate: None,
                    is_method: false,
                },
            ],
            construct_signatures: vec![],
            properties: vec![],
            string_index: None,
            number_index: None,
            symbol: None,
            is_abstract: false,
        });
        let sigs = super::get_overload_call_signatures(&interner, overloaded);
        assert!(sigs.is_some());
        assert_eq!(sigs.unwrap().len(), 2);

        // Callable with 1 signature → None (not overloaded)
        let single = interner.callable(CallableShape {
            call_signatures: vec![CallSignature {
                type_params: vec![],
                params: vec![],
                this_type: None,
                return_type: TypeId::VOID,
                type_predicate: None,
                is_method: false,
            }],
            construct_signatures: vec![],
            properties: vec![],
            string_index: None,
            number_index: None,
            symbol: None,
            is_abstract: false,
        });
        assert!(super::get_overload_call_signatures(&interner, single).is_none());

        // Non-callable → None
        assert!(super::get_overload_call_signatures(&interner, TypeId::STRING).is_none());
    }

    #[test]
    fn test_get_object_symbol() {
        let interner = crate::intern::TypeInterner::new();
        use crate::types::{ObjectFlags, ObjectShape, PropertyInfo, Visibility};

        let sym = tsz_binder::SymbolId(42);

        // Object with symbol — use object_with_index to comply with intern quarantine
        let obj_with_sym = interner.object_with_index(ObjectShape {
            flags: ObjectFlags::empty(),
            properties: vec![PropertyInfo {
                name: interner.intern_string("x"),
                type_id: TypeId::STRING,
                write_type: TypeId::STRING,
                optional: false,
                readonly: false,
                is_method: false,
                is_class_prototype: false,
                visibility: Visibility::Public,
                parent_id: None,
                declaration_order: 0,
            }],
            string_index: None,
            number_index: None,
            symbol: Some(sym),
        });
        assert_eq!(super::get_object_symbol(&interner, obj_with_sym), Some(sym));

        // Non-object → None
        assert_eq!(super::get_object_symbol(&interner, TypeId::STRING), None);
    }

    #[test]
    fn test_get_raw_property_type() {
        let interner = crate::intern::TypeInterner::new();
        use crate::types::{PropertyInfo, Visibility};

        let name_x = interner.intern_string("x");
        let name_y = interner.intern_string("y");

        let obj = interner.object(vec![
            PropertyInfo {
                name: name_x,
                type_id: TypeId::STRING,
                write_type: TypeId::STRING,
                optional: false,
                readonly: false,
                is_method: false,
                is_class_prototype: false,
                visibility: Visibility::Public,
                parent_id: None,
                declaration_order: 0,
            },
            PropertyInfo {
                name: name_y,
                type_id: TypeId::NUMBER,
                write_type: TypeId::NUMBER,
                optional: false,
                readonly: false,
                is_method: false,
                is_class_prototype: false,
                visibility: Visibility::Public,
                parent_id: None,
                declaration_order: 1,
            },
        ]);

        assert_eq!(
            super::get_raw_property_type(&interner, obj, name_x),
            Some(TypeId::STRING)
        );
        assert_eq!(
            super::get_raw_property_type(&interner, obj, name_y),
            Some(TypeId::NUMBER)
        );

        // Non-existent property
        let name_z = interner.intern_string("z");
        assert_eq!(super::get_raw_property_type(&interner, obj, name_z), None);

        // Non-object type
        assert_eq!(
            super::get_raw_property_type(&interner, TypeId::STRING, name_x),
            None
        );
    }

    #[test]
    fn test_intersect_constructor_returns() {
        let interner = crate::intern::TypeInterner::new();
        use crate::types::FunctionShape;

        // Function constructor — return type gets intersected
        let fn_ctor = interner.function(FunctionShape {
            type_params: vec![],
            params: vec![],
            this_type: None,
            return_type: TypeId::OBJECT,
            type_predicate: None,
            is_constructor: true,
            is_method: false,
        });
        let result = super::intersect_constructor_returns(&interner, fn_ctor, TypeId::STRING);
        assert_ne!(result, fn_ctor); // Should produce a new type
        // The result should be a Function with intersected return type
        if let Some(shape_id) = crate::visitor::function_shape_id(&interner, result) {
            let shape = interner.function_shape(shape_id);
            assert!(shape.is_constructor);
            // return type should be object & string (intersection)
            assert_ne!(shape.return_type, TypeId::OBJECT);
        } else {
            panic!("Expected Function type");
        }

        // Non-constructor function — unchanged
        let fn_regular = interner.function(FunctionShape {
            type_params: vec![],
            params: vec![],
            this_type: None,
            return_type: TypeId::NUMBER,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });
        assert_eq!(
            super::intersect_constructor_returns(&interner, fn_regular, TypeId::STRING),
            fn_regular
        );

        // Non-callable — unchanged
        assert_eq!(
            super::intersect_constructor_returns(&interner, TypeId::STRING, TypeId::NUMBER),
            TypeId::STRING
        );
    }

    #[test]
    fn test_identity_mapped_passthrough_concrete_primitive() {
        use crate::types::{MappedType, TypeParamInfo};

        let interner = TypeInterner::new();

        // Build: { [K in keyof T]: T[K] } where T is a type parameter
        let t_name = interner.intern_string("T");
        let k_name = interner.intern_string("K");
        let t_param = interner.type_param(TypeParamInfo {
            name: t_name,
            constraint: None,
            default: None,
            is_const: false,
        });
        let k_param = interner.type_param(TypeParamInfo {
            name: k_name,
            constraint: None,
            default: None,
            is_const: false,
        });
        let constraint = interner.keyof(t_param);
        let template = interner.index_access(t_param, k_param);
        let mapped = MappedType {
            type_param: TypeParamInfo {
                name: k_name,
                constraint: None,
                default: None,
                is_const: false,
            },
            constraint,
            name_type: None,
            template,
            readonly_modifier: None,
            optional_modifier: None,
        };
        let mapped_type = interner.mapped(mapped);
        let mapped_id =
            crate::mapped_type_id(&interner, mapped_type).expect("should be a mapped type");

        // Concrete primitives pass through
        assert_eq!(
            super::evaluate_identity_mapped_passthrough(&interner, mapped_id, TypeId::STRING),
            Some(TypeId::STRING)
        );
        assert_eq!(
            super::evaluate_identity_mapped_passthrough(&interner, mapped_id, TypeId::NUMBER),
            Some(TypeId::NUMBER)
        );
        assert_eq!(
            super::evaluate_identity_mapped_passthrough(&interner, mapped_id, TypeId::BOOLEAN),
            Some(TypeId::BOOLEAN)
        );
    }

    #[test]
    fn test_identity_mapped_passthrough_any_no_constraint() {
        use crate::types::{MappedType, TypeParamInfo};

        let interner = TypeInterner::new();

        // Build identity mapped type with unconstrained T
        let t_name = interner.intern_string("T");
        let k_name = interner.intern_string("K");
        let t_param = interner.type_param(TypeParamInfo {
            name: t_name,
            constraint: None,
            default: None,
            is_const: false,
        });
        let k_param = interner.type_param(TypeParamInfo {
            name: k_name,
            constraint: None,
            default: None,
            is_const: false,
        });
        let mapped = MappedType {
            type_param: TypeParamInfo {
                name: k_name,
                constraint: None,
                default: None,
                is_const: false,
            },
            constraint: interner.keyof(t_param),
            name_type: None,
            template: interner.index_access(t_param, k_param),
            readonly_modifier: None,
            optional_modifier: None,
        };
        let mapped_type = interner.mapped(mapped);
        let mapped_id = crate::mapped_type_id(&interner, mapped_type).unwrap();

        // `any` with no array constraint → produces object with index signatures (not `any`)
        let result = super::evaluate_identity_mapped_passthrough(&interner, mapped_id, TypeId::ANY);
        assert!(result.is_some());
        let result = result.unwrap();
        assert_ne!(
            result,
            TypeId::ANY,
            "Objectish<any> should not passthrough to any"
        );

        // unknown with no array constraint → no passthrough
        assert_eq!(
            super::evaluate_identity_mapped_passthrough(&interner, mapped_id, TypeId::UNKNOWN),
            None
        );
    }

    #[test]
    fn test_identity_mapped_passthrough_any_with_array_constraint() {
        use crate::types::{MappedType, TypeParamInfo};

        let interner = TypeInterner::new();

        // Build identity mapped type with T extends any[]
        let t_name = interner.intern_string("T");
        let k_name = interner.intern_string("K");
        let array_constraint = interner.factory().array(TypeId::ANY);
        let t_param = interner.type_param(TypeParamInfo {
            name: t_name,
            constraint: Some(array_constraint),
            default: None,
            is_const: false,
        });
        let k_param = interner.type_param(TypeParamInfo {
            name: k_name,
            constraint: None,
            default: None,
            is_const: false,
        });
        let mapped = MappedType {
            type_param: TypeParamInfo {
                name: k_name,
                constraint: None,
                default: None,
                is_const: false,
            },
            constraint: interner.keyof(t_param),
            name_type: None,
            template: interner.index_access(t_param, k_param),
            readonly_modifier: None,
            optional_modifier: None,
        };
        let mapped_type = interner.mapped(mapped);
        let mapped_id = crate::mapped_type_id(&interner, mapped_type).unwrap();

        // `any` with array constraint → passthrough
        assert_eq!(
            super::evaluate_identity_mapped_passthrough(&interner, mapped_id, TypeId::ANY),
            Some(TypeId::ANY)
        );
    }

    #[test]
    fn test_identity_mapped_passthrough_non_identity() {
        use crate::types::{MappedType, TypeParamInfo};

        let interner = TypeInterner::new();

        // Build non-identity mapped type: { [K in keyof T]: string }
        let t_name = interner.intern_string("T");
        let k_name = interner.intern_string("K");
        let t_param = interner.type_param(TypeParamInfo {
            name: t_name,
            constraint: None,
            default: None,
            is_const: false,
        });
        let mapped = MappedType {
            type_param: TypeParamInfo {
                name: k_name,
                constraint: None,
                default: None,
                is_const: false,
            },
            constraint: interner.keyof(t_param),
            name_type: None,
            template: TypeId::STRING, // Non-identity: template is string, not T[K]
            readonly_modifier: None,
            optional_modifier: None,
        };
        let mapped_type = interner.mapped(mapped);
        let mapped_id = crate::mapped_type_id(&interner, mapped_type).unwrap();

        // Non-identity mapped type → no passthrough
        assert_eq!(
            super::evaluate_identity_mapped_passthrough(&interner, mapped_id, TypeId::NUMBER),
            None
        );
    }
}
