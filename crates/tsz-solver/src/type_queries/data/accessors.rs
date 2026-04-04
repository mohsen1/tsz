//! Callable, object, and property accessor queries.
//!
//! Contains functions for extracting callable shapes, overload signatures,
//! object properties, mapped type helpers, and readonly unwrap utilities.

use super::content_predicates::{
    get_array_element_type, get_intersection_members, get_tuple_elements, get_union_members,
};
use crate::TypeDatabase;
use crate::types::{LiteralValue, PropertyInfo, TypeData, TypeId};
use rustc_hash::FxHashSet;
use tsz_common::Atom;

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

/// For a callable type with overloads, returns the distinct type-parameter counts
/// that the overloads accept. Returns `None` for non-callable types or types with
/// only one signature. Used by the checker to emit TS2743 when no overload matches
/// the provided type argument count.
pub fn overload_type_param_counts(db: &dyn TypeDatabase, type_id: TypeId) -> Option<Vec<usize>> {
    match db.lookup(type_id) {
        Some(TypeData::Callable(shape_id)) => {
            let shape = db.callable_shape(shape_id);
            // Collect all signatures (call + construct)
            let all_sigs = shape
                .call_signatures
                .iter()
                .chain(shape.construct_signatures.iter());
            let mut counts: Vec<usize> = all_sigs.map(|sig| sig.type_params.len()).collect();
            counts.sort_unstable();
            counts.dedup();
            if counts.len() >= 2 {
                Some(counts)
            } else {
                None
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
                type_predicate: func.type_predicate,
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
            return Some(reorder_overload_candidates(db, &shape.call_signatures));
        }
    }
    None
}

/// Reorder overload candidates so that specialized signatures (those with literal
/// type parameters) come before non-specialized signatures.
///
/// This matches tsc's `reorderCandidates` behavior (TypeScript GH#1133). Without
/// this reordering, catch-all `string`/`number` parameter overloads inherited from
/// base types can shadow more specific literal overloads from derived types in
/// diamond inheritance scenarios.
fn reorder_overload_candidates(
    db: &dyn TypeDatabase,
    signatures: &[crate::types::CallSignature],
) -> Vec<crate::types::CallSignature> {
    let mut has_specialized = false;
    let mut has_non_specialized = false;
    for sig in signatures {
        if signature_has_literal_types(db, sig) {
            has_specialized = true;
        } else {
            has_non_specialized = true;
        }
        if has_specialized && has_non_specialized {
            break;
        }
    }
    // Only reorder if there's a mix of specialized and non-specialized
    if !has_specialized || !has_non_specialized {
        return signatures.to_vec();
    }
    let mut specialized = Vec::new();
    let mut non_specialized = Vec::new();
    for sig in signatures {
        if signature_has_literal_types(db, sig) {
            specialized.push(sig.clone());
        } else {
            non_specialized.push(sig.clone());
        }
    }
    specialized.extend(non_specialized);
    specialized
}

/// Check if a call signature has any parameters with literal types.
/// This matches tsc's `signatureHasLiteralTypes` flag.
fn signature_has_literal_types(db: &dyn TypeDatabase, sig: &crate::types::CallSignature) -> bool {
    sig.params.iter().any(|p| {
        matches!(
            db.lookup(p.type_id),
            Some(crate::types::TypeData::Literal(_))
        )
    })
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

/// For a mapped type, return the homomorphic source type if the template is `T[K]`
/// where `K` matches the mapped type's iteration parameter.
///
/// Returns `Some(source)` for homomorphic mapped types like `{ [K in keyof T]: T[K] }`,
/// `None` otherwise.
pub fn homomorphic_mapped_source(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    let Some(TypeData::Mapped(mapped_id)) = db.lookup(type_id) else {
        return None;
    };
    let mapped = db.mapped_type(mapped_id);
    let Some(TypeData::IndexAccess(source, idx)) = db.lookup(mapped.template) else {
        return None;
    };
    let Some(TypeData::TypeParameter(param)) = db.lookup(idx) else {
        return None;
    };
    if param.name == mapped.type_param.name {
        Some(source)
    } else {
        None
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
            ty = db.union2(ty, TypeId::UNDEFINED);
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
        return vec![*param];
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
        vec![*param]
    }
}

/// Get the object shape ID for an object type.
///
/// Returns None if the type is not an object type.
#[inline]
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
        Some(TypeData::TypeParameter(info)) => {
            // For type parameters with constraints, look through to the constraint.
            info.constraint.and_then(|c| get_object_shape(db, c))
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

/// Check if a type is "tuple-like", matching tsc's `isTupleLikeType`.
///
/// A type is tuple-like if it is a Tuple, Array, or an object type with a
/// property named `"0"`. This is used by array literal contextual typing
/// to decide whether to create a tuple type instead of an array type.
pub fn is_tuple_like_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    match db.lookup(type_id) {
        Some(TypeData::Tuple(_) | TypeData::Array(_)) => true,
        Some(TypeData::ReadonlyType(inner)) => is_tuple_like_type(db, inner),
        Some(TypeData::TypeParameter(info)) => {
            info.constraint.is_some_and(|c| is_tuple_like_type(db, c))
        }
        _ => find_property_in_object_by_str(db, type_id, "0").is_some(),
    }
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
