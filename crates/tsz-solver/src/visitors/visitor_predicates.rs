//! Type Predicate Functions
//!
//! This module provides convenience functions for checking type classifications
//! and querying whether types contain specific nested type kinds. These are
//! extracted from the main visitor module for maintainability.
//!
//! # Categories
//!
//! - **Simple predicates** (`is_*`): Check if a type matches a specific `TypeData` variant.
//! - **Deep predicates** (`contains_*`): Recursively check if a type contains specific nested types.
//! - **Constraint-unwrapping predicates** (`is_*_through_type_constraints`):
//!   Variants that unwrap through `ReadonlyType`, `NoInfer`, and `TypeParameter` constraints.
//! - **Object classification**: `ObjectTypeKind` enum and `classify_object_type`.

use crate::types::{IntrinsicKind, ObjectShapeId};
use crate::{TypeData, TypeDatabase, TypeId};
use rustc_hash::FxHashMap;
use tsz_common::Atom;

// =============================================================================
// Specialized Type Predicate Visitors
// =============================================================================

/// Check if a type is a literal type.
///
/// Matches: `TypeData::Literal`(_)
pub fn is_literal_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    // BOOLEAN_TRUE / BOOLEAN_FALSE are reserved intrinsic TypeIds whose
    // TypeData::lookup returns Literal(Boolean), so they ARE literal types.
    if type_id == TypeId::BOOLEAN_TRUE || type_id == TypeId::BOOLEAN_FALSE {
        return true;
    }
    if type_id.is_intrinsic() {
        return false;
    }
    matches!(types.lookup(type_id), Some(TypeData::Literal(_)))
}

/// Check if a type is a module namespace type (import * as ns).
///
/// Matches: `TypeData::ModuleNamespace`(_)
pub fn is_module_namespace_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    matches!(types.lookup(type_id), Some(TypeData::ModuleNamespace(_)))
}

/// Check if a type is an unresolved `Lazy(DefId)` reference.
///
/// Returns true if the type has not been evaluated/resolved yet. This is used
/// by the checker to determine whether the solver's `is_arithmetic_operand`
/// result is authoritative. When the type is resolved (e.g., to `Enum`, `Literal`,
/// etc.), `is_arithmetic_operand` can inspect the structural type and distinguish
/// numeric from string enums. When it's still `Lazy`, the checker may need to
/// use symbol-based fallback checks.
pub fn is_lazy_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    matches!(types.lookup(type_id), Some(TypeData::Lazy(_)))
}

/// Check if a type is a function type (Function or Callable).
///
/// This also handles intersections containing function types.
pub fn is_function_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    is_function_type_impl(types, type_id)
}

fn is_function_type_impl(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    // Fast path: intrinsic types are never `Function` / `Callable` /
    // `Intersection` — the existing match falls through to `_ => false`
    // for them. `is_intrinsic()` is a free `TypeId`-range check; skip the
    // `TypeData` lookup and match dispatch entirely. Same pattern as
    // #2001 / #2005 / #2008 / #2009 / #2014.
    if type_id.is_intrinsic() {
        return false;
    }
    match types.lookup(type_id) {
        Some(TypeData::Function(_) | TypeData::Callable(_)) => true,
        Some(TypeData::Intersection(members)) => {
            let members = types.type_list(members);
            members
                .iter()
                .any(|&member| is_function_type_impl(types, member))
        }
        _ => false,
    }
}

/// Check if a type is an object-like type (suitable for typeof "object").
///
/// Returns true for: Object, `ObjectWithIndex`, Array, Tuple, Mapped, `ReadonlyType` (of object)
pub fn is_object_like_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    is_object_like_type_impl(types, type_id)
}

fn is_object_like_type_impl(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    // Fast path: intrinsics are object-like ONLY for OBJECT and FUNCTION.
    // All other intrinsics fall through the match to `_ => false`.
    if type_id.is_intrinsic() {
        return type_id == TypeId::OBJECT || type_id == TypeId::FUNCTION;
    }
    match types.lookup(type_id) {
        Some(
            TypeData::Object(_)
            | TypeData::ObjectWithIndex(_)
            | TypeData::Array(_)
            | TypeData::Tuple(_)
            | TypeData::Mapped(_)
            | TypeData::Function(_)
            | TypeData::Callable(_)
            | TypeData::Intrinsic(IntrinsicKind::Object | IntrinsicKind::Function),
        ) => true,
        Some(TypeData::ReadonlyType(inner)) => is_object_like_type_impl(types, inner),
        Some(TypeData::Intersection(members)) => {
            let members = types.type_list(members);
            members
                .iter()
                .all(|&member| is_object_like_type_impl(types, member))
        }
        Some(TypeData::TypeParameter(info) | TypeData::Infer(info)) => info
            .constraint
            .is_some_and(|constraint| is_object_like_type_impl(types, constraint)),
        // Lazy types represent unresolved type references (interfaces, classes, type aliases).
        // These are object-like unless they resolve to the global `Function` interface.
        Some(TypeData::Lazy(def_id)) => {
            !types.is_boxed_def_id(def_id, crate::types::IntrinsicKind::Function)
        }
        _ => false,
    }
}

/// Check if a type has late-bound (computed) members.
///
/// Returns true when the type is an object with `HAS_LATE_BOUND_MEMBERS` flag,
/// indicating it has computed property members (e.g., `[symbol]()`) that are
/// not directly representable as named properties in the type system.
/// Also checks through Lazy/Application wrappers via evaluation.
pub fn has_late_bound_members(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    has_late_bound_members_impl(types, type_id)
}

fn has_late_bound_members_impl(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    // Fast path: intrinsic types (`number`, `string`, `any`, `never`, etc.)
    // are not Object/ObjectWithIndex/Intersection, so the existing match
    // falls through to the `_` arm. Calling `evaluate_type` on an intrinsic
    // returns the same TypeId, which then short-circuits to `false` — but
    // only after a `TypeData` lookup, an eight-arm match dispatch, and an
    // `evaluate_type` call. `TypeId::is_intrinsic` is a free range check;
    // skip the rest entirely. Same pattern as #2001 / #2005 / #2008 / #2009 /
    // #2014 / #2015 / #2017 / #2019.
    if type_id.is_intrinsic() {
        return false;
    }
    match types.lookup(type_id) {
        Some(TypeData::ObjectWithIndex(shape_id)) | Some(TypeData::Object(shape_id)) => {
            let shape = types.object_shape(shape_id);
            shape
                .flags
                .contains(crate::types::ObjectFlags::HAS_LATE_BOUND_MEMBERS)
        }
        Some(TypeData::Intersection(members_id)) => {
            let members = types.type_list(members_id);
            members
                .iter()
                .any(|&m| has_late_bound_members_impl(types, m))
        }
        _ => {
            // Try evaluating (resolve Lazy/Application) and check the result
            let evaluated = crate::evaluation::evaluate::evaluate_type(types, type_id);
            if evaluated != type_id {
                has_late_bound_members_impl(types, evaluated)
            } else {
                false
            }
        }
    }
}

/// Check if a type is an empty object type (no properties, no index signatures).
pub fn is_empty_object_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    match types.lookup(type_id) {
        Some(TypeData::Object(shape_id)) => {
            let shape = types.object_shape(shape_id);
            shape.properties.is_empty()
        }
        Some(TypeData::ObjectWithIndex(shape_id)) => {
            let shape = types.object_shape(shape_id);
            shape.properties.is_empty()
                && shape.string_index.is_none()
                && shape.number_index.is_none()
        }
        _ => false,
    }
}

/// Check if a type is a primitive type (intrinsic or literal).
pub fn is_primitive_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    // Check well-known intrinsic primitive TypeIds first.
    // In tsc, Primitive = String | Number | BigInt | Boolean | Null | Undefined | ESSymbol | Void.
    // Exclude non-primitive intrinsics: object, never, unknown, any, error,
    // function, and internal sentinels. Note: void IS a primitive in tsc.
    if type_id.is_intrinsic() {
        return !matches!(
            type_id,
            TypeId::OBJECT
                | TypeId::NEVER
                | TypeId::UNKNOWN
                | TypeId::ANY
                | TypeId::ERROR
                | TypeId::FUNCTION
                | TypeId::PROMISE_BASE
                | TypeId::DELEGATE
                | TypeId::STRICT_ANY
        );
    }
    matches!(
        types.lookup(type_id),
        Some(TypeData::Intrinsic(_) | TypeData::Literal(_))
    )
}

/// Check if a type is a union type.
pub fn is_union_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    matches!(types.lookup(type_id), Some(TypeData::Union(_)))
}

/// Check if a type is an intersection type.
pub fn is_intersection_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    matches!(types.lookup(type_id), Some(TypeData::Intersection(_)))
}

/// Check if a type is an array type.
pub fn is_array_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    matches!(types.lookup(type_id), Some(TypeData::Array(_)))
}

/// Check if a type is a tuple type (including readonly tuples wrapped in `ReadonlyType`).
pub fn is_tuple_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    match types.lookup(type_id) {
        Some(TypeData::Tuple(_)) => true,
        Some(TypeData::ReadonlyType(inner)) => is_tuple_type(types, inner),
        _ => false,
    }
}

/// Check if a type provides structural wrapping that breaks type alias
/// circular reference chains.  In TypeScript, recursion through "deferred"
/// types is legal:
///   - Array, Tuple, `ReadonlyType` wrapping those
///   - Object / `ObjectWithIndex` (object literal types)
///   - Function / Callable (function/constructor types)
///   - Mapped types, Application (generic instantiation)
///
/// Conversely, Lazy, Union, and Intersection are transparent -- they do NOT
/// provide structural wrapping by themselves.
///
/// For union types the body is considered deferred only when **every** member
/// is itself deferred (e.g., `JsonValue[] | readonly JsonValue[]`).
pub fn is_structurally_deferred_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    match types.lookup(type_id) {
        Some(
            TypeData::Array(_)
            | TypeData::Tuple(_)
            | TypeData::Object(_)
            | TypeData::ObjectWithIndex(_)
            | TypeData::Function(_)
            | TypeData::Callable(_)
            | TypeData::Mapped(_)
            | TypeData::Application(_),
        ) => true,
        Some(TypeData::ReadonlyType(inner)) => is_structurally_deferred_type(types, inner),
        Some(TypeData::Union(list_id)) => {
            let members = types.type_list(list_id);
            !members.is_empty()
                && members
                    .iter()
                    .all(|&m| is_structurally_deferred_type(types, m))
        }
        _ => false,
    }
}

/// Check if a type is a type parameter.
pub fn is_type_parameter(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    matches!(
        types.lookup(type_id),
        Some(TypeData::TypeParameter(_) | TypeData::Infer(_))
    )
}

/// Check if a type is a conditional type.
pub fn is_conditional_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    matches!(types.lookup(type_id), Some(TypeData::Conditional(_)))
}

/// Check if a type contains a deferred conditional type, either directly
/// or as a member of an intersection. Used to determine whether an
/// excess property failure should be downgraded to a structural mismatch
/// (TS2322) since the deferred conditional makes the assignment incompatible
/// regardless of excess properties.
pub fn has_deferred_conditional_member(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    match types.lookup(type_id) {
        Some(TypeData::Conditional(_)) => true,
        Some(TypeData::Intersection(list_id)) => {
            let members = types.type_list(list_id);
            members
                .iter()
                .any(|m| matches!(types.lookup(*m), Some(TypeData::Conditional(_))))
        }
        _ => false,
    }
}

/// Check if a type is a mapped type.
pub fn is_mapped_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    matches!(types.lookup(type_id), Some(TypeData::Mapped(_)))
}

/// Check if a type is an index access type.
pub fn is_index_access_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    matches!(types.lookup(type_id), Some(TypeData::IndexAccess(_, _)))
}

/// Check if a type is a type query (typeof) type.
pub fn is_type_query_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    matches!(types.lookup(type_id), Some(TypeData::TypeQuery(_)))
}

/// Check if a type is a template literal type.
pub fn is_template_literal_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    matches!(types.lookup(type_id), Some(TypeData::TemplateLiteral(_)))
}

/// Check if a type is a type reference (Lazy/DefId).
pub fn is_type_reference(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    matches!(
        types.lookup(type_id),
        Some(TypeData::Lazy(_) | TypeData::Recursive(_))
    )
}

/// Check if a type is a generic type application.
pub fn is_generic_application(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    matches!(types.lookup(type_id), Some(TypeData::Application(_)))
}

/// Check if a type can be compared by `TypeId` identity alone (O(1) equality).
///
/// Identity-comparable types are types where subtyping reduces to identity: two different
/// identity-comparable types are always disjoint (neither is a subtype of the other).
///
/// This is used as an optimization to skip structural recursion in subtype checking.
/// For example, comparing `[E.A, E.B]` vs `[E.C, E.D]` can return `source == target`
/// in O(1) instead of walking into each tuple element.
///
/// Identity-comparable types include:
/// - Literal types (string, number, boolean, bigint literals)
/// - Enum members (`TypeData::Enum`)
/// - Unique symbols
/// - null, undefined, void, never
/// - Tuples where ALL elements are identity-comparable (and no rest elements)
///
/// NOTE: This is NOT the same as tsc's `isUnitType` (which excludes void, never, and tuples).
/// For tsc-compatible unit type semantics, use `type_queries::is_unit_type`.
///
/// NOTE: This does NOT handle `ReadonlyType` - readonly tuples must be checked separately
/// because `["a"]` is a subtype of `readonly ["a"]` even though they have different `TypeIds`.
pub fn is_identity_comparable_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    is_identity_comparable_type_impl(types, type_id, 0)
}

const MAX_IDENTITY_COMPARABLE_DEPTH: u32 = 10;

fn is_identity_comparable_type_impl(types: &dyn TypeDatabase, type_id: TypeId, depth: u32) -> bool {
    // Prevent stack overflow on pathological types
    if depth > MAX_IDENTITY_COMPARABLE_DEPTH {
        return false;
    }

    // Check well-known singleton types first
    if type_id == TypeId::NULL
        || type_id == TypeId::UNDEFINED
        || type_id == TypeId::VOID
        || type_id == TypeId::NEVER
    {
        return true;
    }

    match types.lookup(type_id) {
        // Identity-comparable scalar types.
        Some(TypeData::Literal(_))
        | Some(TypeData::Enum(_, _))
        | Some(TypeData::UniqueSymbol(_)) => true,

        // Tuples are NOT identity-comparable because labeled tuples like [a: 1]
        // and [b: 1] are compatible despite having different TypeIds.
        // Similarly, [1, 2?] and [a: 1, b?: 2] must go through structural comparison
        // (check_tuple_subtype) which correctly ignores labels.
        // This matches the same reasoning as ReadonlyType below.

        // Everything else is not identity-comparable.
        _ => false,
    }
}

// =============================================================================
// Type Contains Visitor - Check if a type contains specific types
// =============================================================================

/// Check if a type contains any type parameters.
pub fn contains_type_parameters(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    contains_type_matching(types, type_id, |key| {
        matches!(key, TypeData::TypeParameter(_) | TypeData::Infer(_))
    })
}

/// Check if a type contains free type parameters, excluding those bound by
/// enclosing function/callable signatures. See `contains_free_type_parameters_db`
/// in `content_predicates` for the full doc.
pub fn contains_free_type_parameters(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    let mut checker = FreeTypeParamChecker {
        types,
        memo: FxHashMap::default(),
        guard: crate::recursion::RecursionGuard::with_profile(
            crate::recursion::RecursionProfile::ShallowTraversal,
        ),
    };
    checker.check(type_id)
}

/// Check if a type contains any `infer` types.
pub fn contains_infer_types(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    contains_type_matching(types, type_id, |key| matches!(key, TypeData::Infer(_)))
}

/// Check if a type contains any "free" `infer` types — inference placeholders
/// that are NOT buried inside a `TypeParameter`'s constraint or default.
///
/// `TypeParameter` constraints/defaults are definitional (e.g., `T extends Foo`
/// where `Foo = X extends Bar<infer V> ? V : never`). The `infer V` there is
/// structural and already resolved at the definition site. Walking into it
/// produces false positives when used to decide whether to suppress diagnostics.
///
/// This variant is used by `should_suppress_assignability_diagnostic` to avoid
/// suppressing real errors like TS2322 when the only `infer` types are in
/// type parameter constraint chains.
pub fn contains_free_infer_types(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    let mut checker = FreeInferChecker {
        types,
        memo: FxHashMap::default(),
        guard: crate::recursion::RecursionGuard::with_profile(
            crate::recursion::RecursionProfile::ShallowTraversal,
        ),
    };
    checker.check(type_id)
}

/// Check if a type contains the `any` intrinsic anywhere.
pub fn contains_any_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id == TypeId::ANY {
        return true;
    }
    contains_type_matching(types, type_id, |key| {
        matches!(key, TypeData::Intrinsic(IntrinsicKind::Any))
    })
}

/// Check if a type contains the error type.
///
/// This handles `TypeId::ERROR` directly and also detects error types nested
/// inside Application types (e.g., `Application(Error, args)` which displays
/// as `error<args>`). The generic `contains_type_matching` visitor can't catch
/// these because (a) its intrinsic fast-path skips `TypeId::ERROR` and (b) it
/// doesn't check Application bases.
pub fn contains_error_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id == TypeId::ERROR {
        return true;
    }
    contains_error_type_recursive(types, type_id, &mut FxHashMap::default())
}

fn contains_error_type_recursive(
    types: &dyn TypeDatabase,
    type_id: TypeId,
    memo: &mut FxHashMap<TypeId, bool>,
) -> bool {
    if type_id == TypeId::ERROR {
        return true;
    }
    if type_id.is_intrinsic() {
        return false;
    }
    if let Some(&cached) = memo.get(&type_id) {
        return cached;
    }
    // Mark as false to break cycles
    memo.insert(type_id, false);

    let Some(key) = types.lookup(type_id) else {
        return false;
    };
    if matches!(key, TypeData::Error | TypeData::UnresolvedTypeName(_)) {
        memo.insert(type_id, true);
        return true;
    }

    // Terminal-kind fast path. These variants have no children to recurse
    // into and fall through the match below to `_ => false`. Short-circuiting
    // here skips the eight-arm dispatch and the trailing memo write (we
    // already inserted `false` at line 462 for cycle prevention, and the
    // match's `_ => false` would just rewrite the same value).
    if matches!(
        key,
        TypeData::Literal(_)
            | TypeData::ThisType
            | TypeData::BoundParameter(_)
            | TypeData::Lazy(_)
            | TypeData::Recursive(_)
            | TypeData::TypeQuery(_)
            | TypeData::UniqueSymbol(_)
            | TypeData::ModuleNamespace(_)
            | TypeData::Intrinsic(_)
    ) {
        return false;
    }

    let result = match key {
        TypeData::Application(app_id) => {
            let app = types.type_application(app_id);
            // Check both base AND args for error types. Unlike the generic
            // contains_type_matching which skips bases to avoid false positives
            // with type parameters, error types in the base are always wrong.
            contains_error_type_recursive(types, app.base, memo)
                || app
                    .args
                    .iter()
                    .any(|&a| contains_error_type_recursive(types, a, memo))
        }
        TypeData::Union(list_id) | TypeData::Intersection(list_id) => {
            let members = types.type_list(list_id);
            members
                .iter()
                .any(|&m| contains_error_type_recursive(types, m, memo))
        }
        TypeData::Tuple(tuple_list_id) => {
            let elements = types.tuple_list(tuple_list_id);
            elements
                .iter()
                .any(|elem| contains_error_type_recursive(types, elem.type_id, memo))
        }
        TypeData::Array(element_type) => contains_error_type_recursive(types, element_type, memo),
        TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
            let shape = types.object_shape(shape_id);
            shape.properties.iter().any(|prop| {
                contains_error_type_recursive(types, prop.type_id, memo)
                    || contains_error_type_recursive(types, prop.write_type, memo)
            }) || shape.string_index.as_ref().is_some_and(|index| {
                contains_error_type_recursive(types, index.key_type, memo)
                    || contains_error_type_recursive(types, index.value_type, memo)
            }) || shape.number_index.as_ref().is_some_and(|index| {
                contains_error_type_recursive(types, index.key_type, memo)
                    || contains_error_type_recursive(types, index.value_type, memo)
            })
        }
        TypeData::Function(shape_id) => {
            let shape = types.function_shape(shape_id);
            contains_error_type_recursive(types, shape.return_type, memo)
                || shape
                    .params
                    .iter()
                    .any(|p| contains_error_type_recursive(types, p.type_id, memo))
        }
        TypeData::Callable(shape_id) => {
            let shape = types.callable_shape(shape_id);
            shape.call_signatures.iter().any(|sig| {
                sig.params
                    .iter()
                    .any(|param| contains_error_type_recursive(types, param.type_id, memo))
                    || contains_error_type_recursive(types, sig.return_type, memo)
                    || sig.this_type.is_some_and(|this_type| {
                        contains_error_type_recursive(types, this_type, memo)
                    })
            }) || shape.construct_signatures.iter().any(|sig| {
                sig.params
                    .iter()
                    .any(|param| contains_error_type_recursive(types, param.type_id, memo))
                    || contains_error_type_recursive(types, sig.return_type, memo)
                    || sig.this_type.is_some_and(|this_type| {
                        contains_error_type_recursive(types, this_type, memo)
                    })
            }) || shape.properties.iter().any(|prop| {
                contains_error_type_recursive(types, prop.type_id, memo)
                    || contains_error_type_recursive(types, prop.write_type, memo)
            }) || shape.string_index.as_ref().is_some_and(|index| {
                contains_error_type_recursive(types, index.key_type, memo)
                    || contains_error_type_recursive(types, index.value_type, memo)
            }) || shape.number_index.as_ref().is_some_and(|index| {
                contains_error_type_recursive(types, index.key_type, memo)
                    || contains_error_type_recursive(types, index.value_type, memo)
            })
        }
        _ => false,
    };
    memo.insert(type_id, result);
    result
}

/// Check if a type contains the `this` type anywhere.
///
/// The result is stable per `TypeId` within a single `TypeInterner`, so we
/// memoize in a project-wide `DashMap` on the interner to avoid the repeated
/// recursive walk that profiled at ~5% of total CPU on multi-file workloads.
#[inline]
pub fn contains_this_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    // Fast path: intrinsic types never contain ThisType
    if type_id.is_intrinsic() {
        return false;
    }
    if let Some(cached) = types.contains_this_type_cached(type_id) {
        return cached;
    }
    let result = contains_type_matching(types, type_id, |key| matches!(key, TypeData::ThisType));
    types.set_contains_this_type_cache(type_id, result);
    result
}

/// Check if a type contains any type matching a predicate.
pub fn contains_type_matching<F>(types: &dyn TypeDatabase, type_id: TypeId, predicate: F) -> bool
where
    F: Fn(&TypeData) -> bool,
{
    let mut checker = ContainsTypeChecker {
        types,
        predicate,
        memo: FxHashMap::default(),
        guard: crate::recursion::RecursionGuard::with_profile(
            crate::recursion::RecursionProfile::ShallowTraversal,
        ),
    };
    checker.check(type_id)
}

/// Check if a type contains a type parameter with the given name.
///
/// This is a convenience wrapper around `contains_type_matching` that avoids
/// requiring callers to match on `TypeData` internals directly.
pub fn contains_type_parameter_named(
    types: &dyn TypeDatabase,
    type_id: TypeId,
    name: Atom,
) -> bool {
    contains_type_matching(
        types,
        type_id,
        |td| matches!(td, TypeData::TypeParameter(info) if info.name == name),
    )
}

/// Check if a type contains a type parameter with the given name, WITHOUT
/// walking into other type parameters' constraints.
///
/// Unlike `contains_type_parameter_named`, this does not descend into
/// `TypeParameter.constraint` or `TypeParameter.default`. This is important
/// for mapped type circular-constraint detection: in `{ [K in keyof T]: T[K] }`,
/// `K`'s constraint is `keyof T`. The deep check would walk into `T`'s own
/// constraint (which may contain `K`), falsely reporting a cycle.
pub fn contains_type_parameter_named_shallow(
    types: &dyn TypeDatabase,
    type_id: TypeId,
    name: Atom,
) -> bool {
    use rustc_hash::FxHashSet;

    let mut visited = FxHashSet::default();
    let mut stack = vec![type_id];

    while let Some(current) = stack.pop() {
        if current.is_intrinsic() || !visited.insert(current) {
            continue;
        }

        let Some(data) = types.lookup(current) else {
            continue;
        };

        // Check predicate
        if matches!(&data, TypeData::TypeParameter(info) if info.name == name) {
            return true;
        }

        // Visit children but skip TypeParameter/Infer constraints/defaults.
        // For TypeParameter/Infer, we only care about identity (name match),
        // not what their constraints contain.
        if matches!(&data, TypeData::TypeParameter(_) | TypeData::Infer(_)) {
            continue;
        }
        // Terminal kinds have no children to enumerate. Skipping
        // `for_each_child_by_id` (which would iterate an empty child set)
        // saves the closure setup and visitor dispatch on the very common
        // input shape where the predicate is the entry-point lookup result.
        // The kinds listed here match the leaf arms of every other walker
        // that returns `false` for them — see `ContainsTypeChecker.check_key`,
        // `FreeTypeParamChecker.check_key`, and `FreeInferChecker.check_key`.
        if matches!(
            &data,
            TypeData::Literal(_)
                | TypeData::Error
                | TypeData::ThisType
                | TypeData::BoundParameter(_)
                | TypeData::Lazy(_)
                | TypeData::Recursive(_)
                | TypeData::TypeQuery(_)
                | TypeData::UniqueSymbol(_)
                | TypeData::ModuleNamespace(_)
                | TypeData::UnresolvedTypeName(_)
        ) {
            continue;
        }
        // For all other types, use the generic child visitor.
        super::visitor::for_each_child_by_id(types, current, |child| {
            if !visited.contains(&child) {
                stack.push(child);
            }
        });
    }
    false
}

/// Check if a type transitively references any type parameter whose name
/// is in the given set.
///
/// This is more efficient than `collect_referenced_types` followed by
/// per-element `type_param_info` checks, because it short-circuits on
/// the first match.
pub fn references_any_type_param_named(
    types: &dyn TypeDatabase,
    type_id: TypeId,
    names: &rustc_hash::FxHashSet<Atom>,
) -> bool {
    contains_type_matching(
        types,
        type_id,
        |td| matches!(td, TypeData::TypeParameter(info) if names.contains(&info.name)),
    )
}

/// Check if a constraint type references a type parameter along the base-constraint
/// resolution path. This mimics tsc's `getBaseConstraint` recursion, which only
/// follows certain structural paths:
///
/// Descended into (these require resolving sub-constraints):
/// - Union/intersection members
/// - Mapped type constraint (the key source)
/// - Conditional check/extends types
/// - Index access object/index
/// - `KeyOf` operand
///
/// NOT descended into (these are type references/wrappers — tsc treats them as opaque):
/// - Type application arguments (e.g. `Foo<T>`)
/// - Array/Tuple/ReadonlyType/NoInfer inner types (these are effectively type references)
/// - Object property types
/// - Function parameter/return types
///
/// This avoids false positives: `T extends Array<T>` is NOT circular,
/// but `T extends { [P in T]: number }` IS circular.
pub fn constraint_references_type_param_in_resolution_path(
    types: &dyn TypeDatabase,
    type_id: TypeId,
    param_name: Atom,
) -> bool {
    use rustc_hash::FxHashSet;

    let mut visited = FxHashSet::default();
    let mut stack = vec![type_id];

    while let Some(current) = stack.pop() {
        if current.is_intrinsic() || !visited.insert(current) {
            continue;
        }

        let Some(data) = types.lookup(current) else {
            continue;
        };

        // Found the type parameter we're looking for
        if matches!(&data, TypeData::TypeParameter(info) if info.name == param_name) {
            return true;
        }

        // Follow only resolution-path children (not type reference args)
        match &data {
            // Union/intersection: descend into all members
            TypeData::Union(list_id) | TypeData::Intersection(list_id) => {
                for &member in types.type_list(*list_id).iter() {
                    stack.push(member);
                }
            }
            // Mapped type: descend into the constraint (key source) only.
            // This catches `T extends { [P in T]: number }` (genuinely circular)
            // while NOT false-positiving on `T extends { [K in keyof T]: V }`
            // because we don't follow through KeyOf (see below).
            TypeData::Mapped(mapped_id) => {
                let mapped = types.get_mapped(*mapped_id);
                stack.push(mapped.constraint);
            }
            // Index access: descend into object and index.
            // Catches `T extends Foo | T["hello"]` (circular through index access).
            TypeData::IndexAccess(obj, idx) => {
                stack.push(*obj);
                stack.push(*idx);
            }
            // KeyOf, Conditional, and everything else (Application, Object,
            // Function, Array, Tuple, ReadonlyType, NoInfer, etc.) are opaque
            // at the constraint-resolution level. `T extends { [K in keyof T]: V }`
            // is NOT circular in tsc, and neither is `T extends null extends T ? any : never`.
            _ => {}
        }
    }
    false
}

/// Check if a type transitively contains a specific `TypeId`.
///
/// This is more efficient than `collect_referenced_types(…).contains(&target)`
/// because it short-circuits as soon as the target is found.
pub fn contains_type_by_id(types: &dyn TypeDatabase, root: TypeId, target: TypeId) -> bool {
    if root == target {
        return true;
    }
    let mut visited = FxHashMap::default();
    let mut stack = vec![root];
    while let Some(current) = stack.pop() {
        if current == target {
            return true;
        }
        if visited.contains_key(&current) {
            continue;
        }
        visited.insert(current, true);
        super::visitor::for_each_child_by_id(types, current, |child| {
            if !visited.contains_key(&child) {
                stack.push(child);
            }
        });
    }
    false
}

struct ContainsTypeChecker<'a, F>
where
    F: Fn(&TypeData) -> bool,
{
    types: &'a dyn TypeDatabase,
    predicate: F,
    memo: FxHashMap<TypeId, bool>,
    guard: crate::recursion::RecursionGuard<TypeId>,
}

impl<'a, F> ContainsTypeChecker<'a, F>
where
    F: Fn(&TypeData) -> bool,
{
    fn check(&mut self, type_id: TypeId) -> bool {
        // Fast path: intrinsic types (primitives, any, never, etc.) have no subtypes
        // and can never contain nested type structures.
        if type_id.is_intrinsic() {
            return false;
        }

        if let Some(&cached) = self.memo.get(&type_id) {
            return cached;
        }

        let Some(key) = self.types.lookup(type_id) else {
            return false;
        };

        if (self.predicate)(&key) {
            self.memo.insert(type_id, true);
            return true;
        }

        // Terminal-kind fast path: types with no children to walk and no
        // cycle risk. The recursive `check_key` below would dispatch to its
        // leaf arm and immediately return `false` for these kinds, so
        // skipping the `guard.enter`/`guard.leave` HashSet round-trip is a
        // pure win. Memo is still updated so repeat visits of the same
        // type within one `contains_type_matching` call stay O(1).
        //
        // `Intrinsic` is already handled by the entry-level `is_intrinsic`
        // check above. The remaining terminal kinds match the recursive
        // walker's leaf arm in `check_key`.
        if matches!(
            key,
            TypeData::Literal(_)
                | TypeData::Error
                | TypeData::ThisType
                | TypeData::BoundParameter(_)
                | TypeData::Lazy(_)
                | TypeData::Recursive(_)
                | TypeData::TypeQuery(_)
                | TypeData::UniqueSymbol(_)
                | TypeData::ModuleNamespace(_)
                | TypeData::UnresolvedTypeName(_)
        ) {
            self.memo.insert(type_id, false);
            return false;
        }

        match self.guard.enter(type_id) {
            crate::recursion::RecursionResult::Entered => {}
            _ => return false,
        }

        let result = self.check_key(&key);

        self.guard.leave(type_id);
        self.memo.insert(type_id, result);

        result
    }

    fn check_key(&mut self, key: &TypeData) -> bool {
        match key {
            TypeData::Intrinsic(_)
            | TypeData::Literal(_)
            | TypeData::Error
            | TypeData::ThisType
            | TypeData::BoundParameter(_)
            | TypeData::Lazy(_)
            | TypeData::Recursive(_)
            | TypeData::TypeQuery(_)
            | TypeData::UniqueSymbol(_)
            | TypeData::ModuleNamespace(_)
            | TypeData::UnresolvedTypeName(_) => false,
            TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
                let shape = self.types.object_shape(*shape_id);
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
                let members = self.types.type_list(*list_id);
                members.iter().any(|&m| self.check(m))
            }
            TypeData::Array(elem) => self.check(*elem),
            TypeData::Tuple(list_id) => {
                let elements = self.types.tuple_list(*list_id);
                elements.iter().any(|e| self.check(e.type_id))
            }
            TypeData::Function(shape_id) => {
                let shape = self.types.function_shape(*shape_id);
                shape.params.iter().any(|p| self.check(p.type_id))
                    || self.check(shape.return_type)
                    || shape.this_type.is_some_and(|t| self.check(t))
            }
            TypeData::Callable(shape_id) => {
                let shape = self.types.callable_shape(*shape_id);
                shape.call_signatures.iter().any(|s| {
                    s.params.iter().any(|p| self.check(p.type_id))
                        || self.check(s.return_type)
                        || s.this_type.is_some_and(|t| self.check(t))
                }) || shape.construct_signatures.iter().any(|s| {
                    s.params.iter().any(|p| self.check(p.type_id))
                        || self.check(s.return_type)
                        || s.this_type.is_some_and(|t| self.check(t))
                }) || shape.properties.iter().any(|p| self.check(p.type_id))
            }
            TypeData::TypeParameter(info) | TypeData::Infer(info) => {
                info.constraint.is_some_and(|c| self.check(c))
                    || info.default.is_some_and(|d| self.check(d))
            }
            TypeData::Application(app_id) => {
                // Only check args, not base. The base type's own type parameters
                // are bound by the application arguments and should not count as
                // "containing type parameters". E.g., `A<number>` is concrete even
                // though `A`'s definition contains `TypeParameter T`.
                let app = self.types.type_application(*app_id);
                app.args.iter().any(|&a| self.check(a))
            }
            TypeData::Conditional(cond_id) => {
                let cond = self.types.get_conditional(*cond_id);
                self.check(cond.check_type)
                    || self.check(cond.extends_type)
                    || self.check(cond.true_type)
                    || self.check(cond.false_type)
            }
            TypeData::Mapped(mapped_id) => {
                let mapped = self.types.get_mapped(*mapped_id);
                mapped.type_param.constraint.is_some_and(|c| self.check(c))
                    || mapped.type_param.default.is_some_and(|d| self.check(d))
                    || self.check(mapped.constraint)
                    || self.check(mapped.template)
                    || mapped.name_type.is_some_and(|n| self.check(n))
            }
            TypeData::IndexAccess(obj, idx) => self.check(*obj) || self.check(*idx),
            TypeData::TemplateLiteral(list_id) => {
                let spans = self.types.template_list(*list_id);
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
// FreeTypeParamChecker — like ContainsTypeChecker but skips bound type params
// in function/callable signatures
// =============================================================================

struct FreeTypeParamChecker<'a> {
    types: &'a dyn TypeDatabase,
    memo: FxHashMap<TypeId, bool>,
    guard: crate::recursion::RecursionGuard<TypeId>,
}

impl<'a> FreeTypeParamChecker<'a> {
    fn check(&mut self, type_id: TypeId) -> bool {
        if type_id.is_intrinsic() {
            return false;
        }
        if let Some(&cached) = self.memo.get(&type_id) {
            return cached;
        }
        let Some(key) = self.types.lookup(type_id) else {
            return false;
        };
        if matches!(
            key,
            TypeData::TypeParameter(_)
                | TypeData::Infer(_)
                | TypeData::ThisType
                | TypeData::BoundParameter(_)
        ) {
            self.memo.insert(type_id, true);
            return true;
        }
        // Terminal-kind fast path: same set that `check_key` returns `false`
        // for unconditionally. Short-circuit before the recursion-guard
        // enter/leave so common terminals (`Lazy(DefId)`, `TypeQuery`, etc.)
        // skip the per-call `FxHashSet` insert + remove. Mirrors #1978/#1990.
        if matches!(
            key,
            TypeData::Intrinsic(_)
                | TypeData::Literal(_)
                | TypeData::Error
                | TypeData::Lazy(_)
                | TypeData::Recursive(_)
                | TypeData::TypeQuery(_)
                | TypeData::UniqueSymbol(_)
                | TypeData::ModuleNamespace(_)
                | TypeData::UnresolvedTypeName(_)
        ) {
            self.memo.insert(type_id, false);
            return false;
        }
        match self.guard.enter(type_id) {
            crate::recursion::RecursionResult::Entered => {}
            _ => return false,
        }
        let result = self.check_key(&key);
        self.guard.leave(type_id);
        self.memo.insert(type_id, result);
        result
    }

    fn check_key(&mut self, key: &TypeData) -> bool {
        match key {
            TypeData::Intrinsic(_)
            | TypeData::Literal(_)
            | TypeData::Error
            | TypeData::ThisType
            | TypeData::BoundParameter(_)
            | TypeData::Lazy(_)
            | TypeData::Recursive(_)
            | TypeData::TypeQuery(_)
            | TypeData::UniqueSymbol(_)
            | TypeData::ModuleNamespace(_)
            | TypeData::UnresolvedTypeName(_) => false,
            TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
                let shape = self.types.object_shape(*shape_id);
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
                let members = self.types.type_list(*list_id);
                members.iter().any(|&m| self.check(m))
            }
            TypeData::Array(elem) => self.check(*elem),
            TypeData::Tuple(list_id) => {
                let elements = self.types.tuple_list(*list_id);
                elements.iter().any(|e| self.check(e.type_id))
            }
            TypeData::Function(shape_id) => {
                let shape = self.types.function_shape(*shape_id);
                if !shape.type_params.is_empty() {
                    // Generic function: type params in body are bound, not free.
                    // Skip body traversal to avoid counting bound params.
                    return false;
                }
                shape.params.iter().any(|p| self.check(p.type_id))
                    || self.check(shape.return_type)
                    || shape.this_type.is_some_and(|t| self.check(t))
            }
            TypeData::Callable(shape_id) => {
                let shape = self.types.callable_shape(*shape_id);
                shape.call_signatures.iter().any(|s| {
                    if !s.type_params.is_empty() {
                        return false;
                    }
                    s.params.iter().any(|p| self.check(p.type_id))
                        || self.check(s.return_type)
                        || s.this_type.is_some_and(|t| self.check(t))
                }) || shape.construct_signatures.iter().any(|s| {
                    if !s.type_params.is_empty() {
                        return false;
                    }
                    s.params.iter().any(|p| self.check(p.type_id))
                        || self.check(s.return_type)
                        || s.this_type.is_some_and(|t| self.check(t))
                }) || shape.properties.iter().any(|p| self.check(p.type_id))
            }
            TypeData::TypeParameter(info) | TypeData::Infer(info) => {
                info.constraint.is_some_and(|c| self.check(c))
                    || info.default.is_some_and(|d| self.check(d))
            }
            TypeData::Application(app_id) => {
                let app = self.types.type_application(*app_id);
                app.args.iter().any(|&a| self.check(a))
            }
            TypeData::Conditional(cond_id) => {
                let cond = self.types.get_conditional(*cond_id);
                self.check(cond.check_type)
                    || self.check(cond.extends_type)
                    || self.check(cond.true_type)
                    || self.check(cond.false_type)
            }
            TypeData::Mapped(mapped_id) => {
                let mapped = self.types.get_mapped(*mapped_id);
                mapped.type_param.constraint.is_some_and(|c| self.check(c))
                    || mapped.type_param.default.is_some_and(|d| self.check(d))
                    || self.check(mapped.constraint)
                    || self.check(mapped.template)
                    || mapped.name_type.is_some_and(|n| self.check(n))
            }
            TypeData::IndexAccess(obj, idx) => self.check(*obj) || self.check(*idx),
            TypeData::TemplateLiteral(list_id) => {
                let spans = self.types.template_list(*list_id);
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
// FreeInferChecker — like ContainsTypeChecker but skips TypeParameter constraints
// =============================================================================

struct FreeInferChecker<'a> {
    types: &'a dyn TypeDatabase,
    memo: FxHashMap<TypeId, bool>,
    guard: crate::recursion::RecursionGuard<TypeId>,
}

impl<'a> FreeInferChecker<'a> {
    fn check(&mut self, type_id: TypeId) -> bool {
        if type_id.is_intrinsic() {
            return false;
        }
        if let Some(&cached) = self.memo.get(&type_id) {
            return cached;
        }
        let Some(key) = self.types.lookup(type_id) else {
            return false;
        };
        if matches!(key, TypeData::Infer(_)) {
            self.memo.insert(type_id, true);
            return true;
        }
        // Terminal-kind fast path: same set that `check_key` returns `false`
        // for unconditionally (TypeParameter is included here because this
        // walker, by design, does not descend into TypeParameter
        // constraints/defaults). Short-circuit before the recursion-guard
        // enter/leave dance. Mirrors #1978/#1990.
        if matches!(
            key,
            TypeData::Intrinsic(_)
                | TypeData::Literal(_)
                | TypeData::Error
                | TypeData::ThisType
                | TypeData::BoundParameter(_)
                | TypeData::Lazy(_)
                | TypeData::Recursive(_)
                | TypeData::TypeQuery(_)
                | TypeData::UniqueSymbol(_)
                | TypeData::ModuleNamespace(_)
                | TypeData::TypeParameter(_)
                | TypeData::UnresolvedTypeName(_)
        ) {
            self.memo.insert(type_id, false);
            return false;
        }
        match self.guard.enter(type_id) {
            crate::recursion::RecursionResult::Entered => {}
            _ => return false,
        }
        let result = self.check_key(&key);
        self.guard.leave(type_id);
        self.memo.insert(type_id, result);
        result
    }

    fn check_key(&mut self, key: &TypeData) -> bool {
        match key {
            TypeData::Intrinsic(_)
            | TypeData::Literal(_)
            | TypeData::Error
            | TypeData::ThisType
            | TypeData::BoundParameter(_)
            | TypeData::Lazy(_)
            | TypeData::Recursive(_)
            | TypeData::TypeQuery(_)
            | TypeData::UniqueSymbol(_)
            | TypeData::ModuleNamespace(_)
            // TypeParameter/Infer: do NOT walk into constraints/defaults.
            // Structural `infer` patterns in constraints (e.g., from type alias
            // definitions like `type Foo = X extends Bar<infer V> ? V : never`)
            // are definitional, not live inference variables.
            | TypeData::TypeParameter(_)
            | TypeData::Infer(_)
            | TypeData::UnresolvedTypeName(_) => false,
            TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
                let shape = self.types.object_shape(*shape_id);
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
                let members = self.types.type_list(*list_id);
                members.iter().any(|&m| self.check(m))
            }
            TypeData::Array(elem) => self.check(*elem),
            TypeData::Tuple(list_id) => {
                let elements = self.types.tuple_list(*list_id);
                elements.iter().any(|e| self.check(e.type_id))
            }
            TypeData::Function(shape_id) => {
                let shape = self.types.function_shape(*shape_id);
                shape.params.iter().any(|p| self.check(p.type_id))
                    || self.check(shape.return_type)
                    || shape.this_type.is_some_and(|t| self.check(t))
            }
            TypeData::Callable(shape_id) => {
                let shape = self.types.callable_shape(*shape_id);
                shape.call_signatures.iter().any(|s| {
                    s.params.iter().any(|p| self.check(p.type_id))
                        || self.check(s.return_type)
                        || s.this_type.is_some_and(|t| self.check(t))
                }) || shape.construct_signatures.iter().any(|s| {
                    s.params.iter().any(|p| self.check(p.type_id))
                        || self.check(s.return_type)
                        || s.this_type.is_some_and(|t| self.check(t))
                }) || shape.properties.iter().any(|p| self.check(p.type_id))
            }
            TypeData::Application(app_id) => {
                let app = self.types.type_application(*app_id);
                app.args.iter().any(|&a| self.check(a))
            }
            TypeData::Conditional(cond_id) => {
                let cond = self.types.get_conditional(*cond_id);
                self.check(cond.check_type)
                    || self.check(cond.extends_type)
                    || self.check(cond.true_type)
                    || self.check(cond.false_type)
            }
            TypeData::Mapped(mapped_id) => {
                let mapped = self.types.get_mapped(*mapped_id);
                mapped.type_param.constraint.is_some_and(|c| self.check(c))
                    || mapped.type_param.default.is_some_and(|d| self.check(d))
                    || self.check(mapped.constraint)
                    || self.check(mapped.template)
                    || mapped.name_type.is_some_and(|n| self.check(n))
            }
            TypeData::IndexAccess(obj, idx) => self.check(*obj) || self.check(*idx),
            TypeData::TemplateLiteral(list_id) => {
                let spans = self.types.template_list(*list_id);
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
// ShallowContainsTypeChecker — checks type parameter name without traversing
// into type parameter constraints/defaults (prevents false circularity detection)
// =============================================================================

#[allow(dead_code)]
struct ShallowContainsTypeChecker<'a> {
    types: &'a dyn TypeDatabase,
    name: Atom,
    memo: FxHashMap<TypeId, bool>,
    guard: crate::recursion::RecursionGuard<TypeId>,
}

#[allow(dead_code)]
impl<'a> ShallowContainsTypeChecker<'a> {
    fn check(&mut self, type_id: TypeId) -> bool {
        if type_id.is_intrinsic() {
            return false;
        }
        if let Some(&cached) = self.memo.get(&type_id) {
            return cached;
        }
        let Some(key) = self.types.lookup(type_id) else {
            return false;
        };
        // Direct match: is this type parameter the one we're looking for?
        if matches!(&key, TypeData::TypeParameter(info) if info.name == self.name) {
            self.memo.insert(type_id, true);
            return true;
        }
        // Terminal-kind fast path: same set that `check_key` returns `false`
        // for unconditionally. Note: `TypeParameter(_)` is also a terminal
        // here — by design "shallow" does not descend into constraints —
        // but we exclude it from this short-circuit because the positive
        // match above already drained the matching name. Any remaining
        // `TypeParameter` is a non-match terminal. Mirrors #1978/#1990.
        if matches!(
            key,
            TypeData::Intrinsic(_)
                | TypeData::Literal(_)
                | TypeData::Error
                | TypeData::ThisType
                | TypeData::BoundParameter(_)
                | TypeData::Lazy(_)
                | TypeData::Recursive(_)
                | TypeData::TypeQuery(_)
                | TypeData::UniqueSymbol(_)
                | TypeData::ModuleNamespace(_)
                | TypeData::TypeParameter(_)
                | TypeData::Infer(_)
                | TypeData::UnresolvedTypeName(_)
        ) {
            self.memo.insert(type_id, false);
            return false;
        }
        match self.guard.enter(type_id) {
            crate::recursion::RecursionResult::Entered => {}
            _ => return false,
        }
        let result = self.check_key(&key);
        self.guard.leave(type_id);
        self.memo.insert(type_id, result);
        result
    }

    fn check_key(&mut self, key: &TypeData) -> bool {
        match key {
            TypeData::Intrinsic(_)
            | TypeData::Literal(_)
            | TypeData::Error
            | TypeData::ThisType
            | TypeData::BoundParameter(_)
            | TypeData::Lazy(_)
            | TypeData::Recursive(_)
            | TypeData::TypeQuery(_)
            | TypeData::UniqueSymbol(_)
            | TypeData::ModuleNamespace(_)
            // Do NOT traverse into TypeParameter constraints/defaults — that's
            // the whole point of the "shallow" variant. We only check if the
            // type parameter itself matches, not what its constraint contains.
            | TypeData::TypeParameter(_)
            | TypeData::Infer(_)
            | TypeData::UnresolvedTypeName(_) => false,
            TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id) => {
                let shape = self.types.object_shape(*shape_id);
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
                let members = self.types.type_list(*list_id);
                members.iter().any(|&m| self.check(m))
            }
            TypeData::Array(elem) => self.check(*elem),
            TypeData::Tuple(list_id) => {
                let elements = self.types.tuple_list(*list_id);
                elements.iter().any(|e| self.check(e.type_id))
            }
            TypeData::Function(shape_id) => {
                let shape = self.types.function_shape(*shape_id);
                shape.params.iter().any(|p| self.check(p.type_id))
                    || self.check(shape.return_type)
                    || shape.this_type.is_some_and(|t| self.check(t))
            }
            TypeData::Callable(shape_id) => {
                let shape = self.types.callable_shape(*shape_id);
                shape.call_signatures.iter().any(|s| {
                    s.params.iter().any(|p| self.check(p.type_id))
                        || self.check(s.return_type)
                        || s.this_type.is_some_and(|t| self.check(t))
                }) || shape.construct_signatures.iter().any(|s| {
                    s.params.iter().any(|p| self.check(p.type_id))
                        || self.check(s.return_type)
                        || s.this_type.is_some_and(|t| self.check(t))
                }) || shape.properties.iter().any(|p| self.check(p.type_id))
            }
            TypeData::Application(app_id) => {
                let app = self.types.type_application(*app_id);
                app.args.iter().any(|&a| self.check(a))
            }
            TypeData::Conditional(cond_id) => {
                let cond = self.types.get_conditional(*cond_id);
                self.check(cond.check_type)
                    || self.check(cond.extends_type)
                    || self.check(cond.true_type)
                    || self.check(cond.false_type)
            }
            TypeData::Mapped(mapped_id) => {
                let mapped = self.types.get_mapped(*mapped_id);
                mapped.type_param.constraint.is_some_and(|c| self.check(c))
                    || mapped.type_param.default.is_some_and(|d| self.check(d))
                    || self.check(mapped.constraint)
                    || self.check(mapped.template)
                    || mapped.name_type.is_some_and(|n| self.check(n))
            }
            TypeData::IndexAccess(obj, idx) => self.check(*obj) || self.check(*idx),
            TypeData::TemplateLiteral(list_id) => {
                let spans = self.types.template_list(*list_id);
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
// TypeDatabase-based convenience functions with constraint unwrapping
// =============================================================================

/// Check if a type is a literal type (`TypeDatabase` version).
pub fn is_literal_type_through_type_constraints(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    LiteralTypeChecker::check(types, type_id)
}

/// Check if a type is a function type (`TypeDatabase` version).
pub fn is_function_type_through_type_constraints(
    types: &dyn TypeDatabase,
    type_id: TypeId,
) -> bool {
    FunctionTypeChecker::check(types, type_id)
}

/// Check if a type is object-like (`TypeDatabase` version).
pub fn is_object_like_type_through_type_constraints(
    types: &dyn TypeDatabase,
    type_id: TypeId,
) -> bool {
    ObjectTypeChecker::check(types, type_id)
}

/// Check if a type is an empty object type (`TypeDatabase` version).
pub fn is_empty_object_type_through_type_constraints(
    types: &dyn TypeDatabase,
    type_id: TypeId,
) -> bool {
    let checker = EmptyObjectChecker::new(types);
    checker.check(type_id)
}

// =============================================================================
// Object Type Classification
// =============================================================================

/// Classification of object types for freshness tracking.
pub enum ObjectTypeKind {
    /// A regular object type (no index signatures).
    Object(ObjectShapeId),
    /// An object type with index signatures.
    ObjectWithIndex(ObjectShapeId),
    /// Not an object type.
    NotObject,
}

/// Classify a type as an object type kind.
///
/// This is used by the freshness tracking system to determine if a type
/// is a fresh object literal that needs special handling.
pub fn classify_object_type(types: &dyn TypeDatabase, type_id: TypeId) -> ObjectTypeKind {
    match types.lookup(type_id) {
        Some(TypeData::Object(shape_id)) => ObjectTypeKind::Object(shape_id),
        Some(TypeData::ObjectWithIndex(shape_id)) => ObjectTypeKind::ObjectWithIndex(shape_id),
        _ => ObjectTypeKind::NotObject,
    }
}

// =============================================================================
// Visitor Pattern Implementations for Helper Functions
// =============================================================================

/// Visitor to check if a type is a literal type.
struct LiteralTypeChecker;

impl LiteralTypeChecker {
    fn check(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
        // Fast path: intrinsic types are never literal types EXCEPT for
        // `BOOLEAN_TRUE` (14) and `BOOLEAN_FALSE` (15) which are reserved
        // intrinsic IDs for the `true` / `false` literal types. All other
        // intrinsic IDs match no arm and fall through to `_ => false`.
        // `is_intrinsic()` is a free `TypeId`-range check; the explicit
        // exception preserves slow-path behaviour without `TypeData`
        // lookup. Same family as #2001 / #2005 / #2008 / #2009 / #2014
        // / #2019 / #2025.
        if type_id.is_intrinsic() {
            return type_id == TypeId::BOOLEAN_TRUE || type_id == TypeId::BOOLEAN_FALSE;
        }
        match types.lookup(type_id) {
            Some(TypeData::Literal(_)) => true,
            Some(TypeData::Enum(_, structural_type)) => Self::check(types, structural_type),
            Some(TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner)) => {
                Self::check(types, inner)
            }
            Some(TypeData::TypeParameter(info) | TypeData::Infer(info)) => {
                info.constraint.is_some_and(|c| Self::check(types, c))
            }
            _ => false,
        }
    }
}

/// Visitor to check if a type is a function type.
struct FunctionTypeChecker;

impl FunctionTypeChecker {
    fn check(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
        // Fast path: intrinsic types match no arm. Skip lookup + dispatch.
        // Same family as #2001 / #2005 / #2008 / #2009 / #2014 / #2019 / #2025 / #2032.
        if type_id.is_intrinsic() {
            return false;
        }
        match types.lookup(type_id) {
            Some(TypeData::Function(_) | TypeData::Callable(_)) => true,
            Some(TypeData::Intersection(members)) => {
                let members = types.type_list(members);
                members.iter().any(|&member| Self::check(types, member))
            }
            Some(TypeData::TypeParameter(info) | TypeData::Infer(info)) => {
                info.constraint.is_some_and(|c| Self::check(types, c))
            }
            // The global `Function` interface is typeof "function" at runtime.
            // Check if this Lazy type is the known boxed Function type.
            Some(TypeData::Lazy(def_id)) => {
                types.is_boxed_def_id(def_id, crate::types::IntrinsicKind::Function)
            }
            _ => false,
        }
    }
}

/// Visitor to check if a type is object-like.
struct ObjectTypeChecker;

impl ObjectTypeChecker {
    fn check(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
        // Fast path: intrinsic types match no arm. Skip lookup + dispatch.
        if type_id.is_intrinsic() {
            return false;
        }
        match types.lookup(type_id) {
            Some(
                TypeData::Object(_)
                | TypeData::ObjectWithIndex(_)
                | TypeData::Array(_)
                | TypeData::Tuple(_)
                | TypeData::Mapped(_)
                | TypeData::Application(_),
            ) => true,
            Some(TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner)) => {
                Self::check(types, inner)
            }
            Some(TypeData::Intersection(members)) => {
                let members = types.type_list(members);
                members.iter().all(|&member| Self::check(types, member))
            }
            Some(TypeData::TypeParameter(info) | TypeData::Infer(info)) => info
                .constraint
                .is_some_and(|constraint| Self::check(types, constraint)),
            // Lazy types represent unresolved type references (interfaces, classes).
            // Most are object-like at runtime (interfaces/classes), but the global
            // `Function` interface is typeof "function". Check if this Lazy type
            // is the known boxed Function — if so, it's NOT object-like.
            Some(TypeData::Lazy(def_id)) => {
                !types.is_boxed_def_id(def_id, crate::types::IntrinsicKind::Function)
            }
            _ => false,
        }
    }
}

/// Visitor to check if a type is an empty object type.
struct EmptyObjectChecker<'a> {
    db: &'a dyn TypeDatabase,
}

impl<'a> EmptyObjectChecker<'a> {
    fn new(db: &'a dyn TypeDatabase) -> Self {
        Self { db }
    }

    fn check(&self, type_id: TypeId) -> bool {
        match self.db.lookup(type_id) {
            Some(TypeData::Object(shape_id)) => {
                let shape = self.db.object_shape(shape_id);
                shape.properties.is_empty()
            }
            Some(TypeData::ObjectWithIndex(shape_id)) => {
                let shape = self.db.object_shape(shape_id);
                shape.properties.is_empty()
                    && shape.string_index.is_none()
                    && shape.number_index.is_none()
            }
            Some(TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner)) => self.check(inner),
            Some(TypeData::TypeParameter(info) | TypeData::Infer(info)) => {
                info.constraint.is_some_and(|c| self.check(c))
            }
            _ => false,
        }
    }
}
