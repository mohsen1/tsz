//! Type Content Queries and Data Extraction Helpers
//!
//! This module provides functions for extracting type data and checking type content.
//! These functions abstract away the internal `TypeData` representation and provide
//! a stable API for querying type properties without matching on `TypeData` directly.

use crate::TypeDatabase;
use crate::types::{TypeData, TypeId};
use tsz_common::Atom;

// =============================================================================
// Type Content Queries
// =============================================================================

/// Check if a type contains any type parameters (`TypeDatabase` version).
///
/// This is a TypeDatabase-based alternative to `visitor::contains_type_parameters`.
pub fn contains_type_parameters_db(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    contains_type_matching_impl(db, type_id, |key| {
        matches!(key, TypeData::TypeParameter(_) | TypeData::Infer(_))
    })
}

/// Check if a type contains any `infer` types (`TypeDatabase` version).
///
/// This is a TypeDatabase-based alternative to `visitor::contains_infer_types`.
pub fn contains_infer_types_db(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    contains_type_matching_impl(db, type_id, |key| matches!(key, TypeData::Infer(_)))
}

/// Check if a type contains the error type (`TypeDatabase` version).
///
/// This is a TypeDatabase-based alternative to `visitor::contains_error_type`.
pub fn contains_error_type_db(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id == TypeId::ERROR {
        return true;
    }
    contains_type_matching_impl(db, type_id, |key| matches!(key, TypeData::Error))
}

/// Check if a type contains any type matching a predicate.
fn contains_type_matching_impl<F>(db: &dyn TypeDatabase, type_id: TypeId, predicate: F) -> bool
where
    F: Fn(&TypeData) -> bool + Copy,
{
    let mut checker = ContainsTypeChecker {
        db,
        predicate,
        guard: crate::recursion::RecursionGuard::with_profile(
            crate::recursion::RecursionProfile::ShallowTraversal,
        ),
    };
    checker.check(type_id)
}

struct ContainsTypeChecker<'a, F>
where
    F: Fn(&TypeData) -> bool,
{
    db: &'a dyn TypeDatabase,
    predicate: F,
    guard: crate::recursion::RecursionGuard<TypeId>,
}

impl<'a, F> ContainsTypeChecker<'a, F>
where
    F: Fn(&TypeData) -> bool,
{
    fn check(&mut self, type_id: TypeId) -> bool {
        let Some(key) = self.db.lookup(type_id) else {
            return false;
        };

        if (self.predicate)(&key) {
            return true;
        }

        match self.guard.enter(type_id) {
            crate::recursion::RecursionResult::Entered => {}
            _ => return false,
        }

        let result = self.check_key(&key);

        self.guard.leave(type_id);

        result
    }

    fn check_key(&mut self, key: &TypeData) -> bool {
        match key {
            TypeData::Intrinsic(_)
            | TypeData::Literal(_)
            | TypeData::Error
            | TypeData::Lazy(_)
            | TypeData::Recursive(_)
            | TypeData::TypeQuery(_)
            | TypeData::UniqueSymbol(_)
            | TypeData::ModuleNamespace(_) => false,
            // ThisType is polymorphic (`this`) and cannot be resolved at the
            // definition site, so it should be treated as containing type params.
            // BoundParameter is a type parameter bound to a generic signature index.
            TypeData::ThisType | TypeData::BoundParameter(_) => true,
            TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
                let shape = self.db.object_shape(*shape_id);
                shape.properties.iter().any(|p| self.check(p.type_id))
                    || shape
                        .string_index
                        .as_ref()
                        .is_some_and(|i| self.check(i.value_type))
                    || shape
                        .number_index
                        .as_ref()
                        .is_some_and(|i| self.check(i.value_type))
            }
            TypeData::Union(list_id) | TypeData::Intersection(list_id) => {
                let members = self.db.type_list(*list_id);
                members.iter().any(|&m| self.check(m))
            }
            TypeData::Array(elem) => self.check(*elem),
            TypeData::Tuple(list_id) => {
                let elements = self.db.tuple_list(*list_id);
                elements.iter().any(|e| self.check(e.type_id))
            }
            TypeData::Function(shape_id) => {
                let shape = self.db.function_shape(*shape_id);
                shape.params.iter().any(|p| self.check(p.type_id))
                    || self.check(shape.return_type)
                    || shape.this_type.is_some_and(|t| self.check(t))
            }
            TypeData::Callable(shape_id) => {
                let shape = self.db.callable_shape(*shape_id);
                shape.call_signatures.iter().any(|s| {
                    s.params.iter().any(|p| self.check(p.type_id)) || self.check(s.return_type)
                }) || shape.construct_signatures.iter().any(|s| {
                    s.params.iter().any(|p| self.check(p.type_id)) || self.check(s.return_type)
                }) || shape.properties.iter().any(|p| self.check(p.type_id))
            }
            TypeData::TypeParameter(info) | TypeData::Infer(info) => {
                info.constraint.is_some_and(|c| self.check(c))
                    || info.default.is_some_and(|d| self.check(d))
            }
            TypeData::Application(app_id) => {
                let app = self.db.type_application(*app_id);
                self.check(app.base) || app.args.iter().any(|&a| self.check(a))
            }
            TypeData::Conditional(cond_id) => {
                let cond = self.db.conditional_type(*cond_id);
                self.check(cond.check_type)
                    || self.check(cond.extends_type)
                    || self.check(cond.true_type)
                    || self.check(cond.false_type)
            }
            TypeData::Mapped(mapped_id) => {
                let mapped = self.db.mapped_type(*mapped_id);
                self.check(mapped.constraint)
                    || self.check(mapped.template)
                    || mapped.name_type.is_some_and(|n| self.check(n))
            }
            TypeData::IndexAccess(obj, idx) => self.check(*obj) || self.check(*idx),
            TypeData::TemplateLiteral(list_id) => {
                let spans = self.db.template_list(*list_id);
                spans.iter().any(|span| {
                    if let crate::types::TemplateSpan::Type(type_id) = span {
                        self.check(*type_id)
                    } else {
                        false
                    }
                })
            }
            TypeData::KeyOf(inner) | TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner) => {
                self.check(*inner)
            }
            TypeData::StringIntrinsic { type_arg, .. } => self.check(*type_arg),
            TypeData::Enum(_def_id, member_type) => self.check(*member_type),
        }
    }
}

// =============================================================================
// Type Extraction Helpers (Phase 5 - Anti-Pattern 8.1 Removal)
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
/// ```ignore
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
        _ => None,
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
        Some(TypeData::Union(list_id)) => {
            let members = db.type_list(list_id);
            let applicable: Vec<TypeId> = members
                .iter()
                .filter(|&&m| matches!(db.lookup(m), Some(TypeData::Tuple(_) | TypeData::Array(_))))
                .copied()
                .collect();
            match applicable.len() {
                0 => None,
                1 => Some(applicable[0]),
                _ => Some(db.union(applicable)),
            }
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
    shape.properties.iter().find(|p| p.name == name).cloned()
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

/// Check if a type is an object type (Object or `ObjectWithIndex`) and return true.
///
/// This is a convenience alias for `is_object_type` for symmetry with extraction functions.
pub fn is_object_type_with_shape(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(
        db.lookup(type_id),
        Some(TypeData::Object(_) | TypeData::ObjectWithIndex(_))
    )
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

/// Get the callable shape ID for a callable type.
///
/// Returns None if the type is not a Callable.
pub fn get_callable_shape_id(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<crate::types::CallableShapeId> {
    match db.lookup(type_id) {
        Some(TypeData::Callable(shape_id)) => Some(shape_id),
        _ => None,
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

/// Check if a type has at least one call signature.
///
/// Returns false if the type is not a callable shape.
pub fn has_call_signatures(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    get_callable_shape(db, type_id).is_some_and(|shape| !shape.call_signatures.is_empty())
}

/// Get call signatures from a callable type.
///
/// Returns None if the type is not callable.
pub fn get_call_signatures(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<Vec<crate::CallSignature>> {
    get_callable_shape(db, type_id).map(|shape| shape.call_signatures.clone())
}

/// Get construct signatures from a callable type.
///
/// Returns None if the type is not callable.
pub fn get_construct_signatures(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<Vec<crate::CallSignature>> {
    get_callable_shape(db, type_id).map(|shape| shape.construct_signatures.clone())
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

/// Get the function shape ID for a function type.
///
/// Returns None if the type is not a Function.
pub fn get_function_shape_id(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<crate::types::FunctionShapeId> {
    match db.lookup(type_id) {
        Some(TypeData::Function(shape_id)) => Some(shape_id),
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

/// Get the keyof inner type.
///
/// Returns None if the type is not a `KeyOf`.
pub fn get_keyof_type(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    match db.lookup(type_id) {
        Some(TypeData::KeyOf(inner)) => Some(inner),
        _ => None,
    }
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

/// Get the `DefId` from a Lazy type.
///
/// Returns None if the type is not a Lazy type.
pub fn get_lazy_def_id(db: &dyn TypeDatabase, type_id: TypeId) -> Option<crate::def::DefId> {
    match db.lookup(type_id) {
        Some(TypeData::Lazy(def_id)) => Some(def_id),
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
