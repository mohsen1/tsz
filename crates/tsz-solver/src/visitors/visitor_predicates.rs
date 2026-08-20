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

mod constraint_unwrap;
mod identity_comparable;
mod predicate_pool;

use crate::construction::TypeDatabase;
use crate::types::IntrinsicKind;
use crate::{TypeData, TypeId};
pub use constraint_unwrap::{
    ObjectTypeKind, classify_object_type, is_empty_object_type_through_type_constraints,
    is_function_type_through_type_constraints, is_literal_type_through_type_constraints,
    is_object_like_type_through_type_constraints,
};
pub use identity_comparable::is_identity_comparable_type;

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

/// Check if a type is a union whose every member is a fresh literal.
///
/// Returns `true` for `"a" | "b" | "c"`, `1 | 2 | 3`, `true | false`, etc.
/// Returns `false` for scalar `Literal` types, primitives, and any union that
/// contains at least one non-literal member.
pub fn is_union_of_fresh_literals(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    match types.lookup(type_id) {
        Some(TypeData::Union(list_id)) => {
            let members = types.type_list(list_id);
            !members.is_empty() && members.iter().all(|&m| is_literal_type(types, m))
        }
        _ => false,
    }
}

/// Decide whether an array-element inference union should have its fresh literal
/// members widened to their primitive base.
///
/// Returns `true` when the union has at least one literal member and every
/// member is either a literal or one of the primitives that literal widening
/// produces (`number` / `string` / `boolean` / `bigint`). This covers:
/// - pure literal unions (`"a" | "b"`, `1 | 2`) — equivalent to
///   `is_union_of_fresh_literals`; and
/// - unions that mix fresh literals with an already-widened primitive, which is
///   exactly the shape produced by spreading a widened array alongside a literal
///   element (`number | "x"` from `[...numberArray, "x"]`). The widened
///   primitive proves the array literal already carries a widened element, so
///   the fresh literal siblings must widen too, matching tsc's
///   `getWidenedLiteralType`.
///
/// A union whose non-literal members include `null` / `undefined` / objects is
/// left alone, preserving the literal members for downstream narrowing (the
/// conservative baseline for mixed literal+nullable element unions).
pub fn array_element_union_widens_literals(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    match types.lookup(type_id) {
        Some(TypeData::Union(list_id)) => {
            let members = types.type_list(list_id);
            let mut has_literal = false;
            for &member in members.iter() {
                if is_literal_type(types, member) {
                    has_literal = true;
                } else if !matches!(
                    member,
                    TypeId::NUMBER | TypeId::STRING | TypeId::BOOLEAN | TypeId::BIGINT
                ) {
                    return false;
                }
            }
            has_literal
        }
        _ => false,
    }
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

/// Check if an invokable type still carries unbound signature type parameters.
///
/// Returns `true` for `TypeData::Function` whose shape declares type
/// parameters and for `TypeData::Callable` with at least one generic call
/// signature. Declaration emit uses this to reject un-instantiated generic
/// callee return types whose free type variables cannot be resolved without
/// checker inference.
pub fn has_generic_call_signature(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    match types.lookup(type_id) {
        Some(TypeData::Function(shape_id)) => {
            !types.function_shape(shape_id).type_params.is_empty()
        }
        Some(TypeData::Callable(shape_id)) => types
            .callable_shape(shape_id)
            .call_signatures
            .iter()
            .any(|sig| !sig.type_params.is_empty()),
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

/// Check if a type is a "widening" primitive intrinsic — i.e., the wide
/// `string` / `number` / `boolean` / `bigint` / `symbol` types whose
/// literal subtypes get absorbed during union normalization.
///
/// Used to recognize the branded-primitive idiom (`string & {}`,
/// `number & {}`, …): subtype-based intersection simplification must
/// preserve the empty-object brand here so unions like
/// `(string & {}) | "literal"` retain their literal members. Literal
/// types like `"hello"` are NOT widening primitives — `"hello" & {}`
/// still collapses to `"hello"`.
pub fn is_widening_primitive_intrinsic(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    match type_id {
        TypeId::STRING | TypeId::NUMBER | TypeId::BOOLEAN | TypeId::BIGINT | TypeId::SYMBOL => true,
        _ => matches!(
            types.lookup(type_id),
            Some(TypeData::Intrinsic(
                IntrinsicKind::String
                    | IntrinsicKind::Number
                    | IntrinsicKind::Boolean
                    | IntrinsicKind::Bigint
                    | IntrinsicKind::Symbol
            ))
        ),
    }
}

/// Check if a type is a bare intrinsic keyword type (`any`, `unknown`, `never`,
/// `void`, `object`, `null`, `undefined`, `boolean`, `number`, `string`,
/// `bigint`, `symbol`) or a literal type.
///
/// These are exactly the types tsc does not attach an `aliasSymbol` to: they
/// resolve to shared singleton types rather than freshly-constructed structural
/// types. A type alias whose body resolves to one of them is therefore rendered
/// structurally (`string`, `42`, `true`, …) in diagnostics rather than by the
/// alias name, mirroring tsc's display policy.
pub fn is_intrinsic_or_literal_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(
        types.lookup(type_id),
        Some(TypeData::Intrinsic(_) | TypeData::Literal(_))
    )
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
                | TypeId::STRICT_ANY
        );
    }
    matches!(
        types.lookup(type_id),
        Some(
            TypeData::Intrinsic(
                IntrinsicKind::Void
                    | IntrinsicKind::Null
                    | IntrinsicKind::Undefined
                    | IntrinsicKind::Boolean
                    | IntrinsicKind::Number
                    | IntrinsicKind::String
                    | IntrinsicKind::Bigint
                    | IntrinsicKind::Symbol,
            ) | TypeData::Literal(_)
        )
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

/// Whether `type_id` is an object that was synthesized by merging the object
/// members of an object-only intersection (`{ a } & { b }`). Carries the
/// `ObjectFlags::INTERSECTION_MERGED` marker, which is part of the shape's
/// identity, so it never aliases a plain object literal of the same shape.
/// Diagnostics use this to recover that a target really is an intersection even
/// after the merge collapsed the structural `Intersection`.
pub fn is_merged_intersection_object(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    let Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) =
        types.lookup(type_id)
    else {
        return false;
    };
    types
        .object_shape(shape_id)
        .flags
        .contains(crate::types::ObjectFlags::INTERSECTION_MERGED)
}

/// Check if a type is an array type.
pub fn is_array_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    match types.lookup(type_id) {
        Some(TypeData::Array(_)) => true,
        Some(TypeData::Substitution { constraint, .. }) => is_array_type(types, constraint),
        _ => false,
    }
}

/// Check if a type is a tuple type (including readonly tuples wrapped in `ReadonlyType`).
pub fn is_tuple_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    match types.lookup(type_id) {
        Some(TypeData::Tuple(_)) => true,
        Some(TypeData::ReadonlyType(inner)) => is_tuple_type(types, inner),
        Some(TypeData::Substitution { constraint, .. }) => is_tuple_type(types, constraint),
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
            members.iter().any(|m| {
                !m.is_intrinsic() && matches!(types.lookup(*m), Some(TypeData::Conditional(_)))
            })
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

/// Returns `true` when `type_id`'s outer shape performs fresh tuple synthesis
/// on evaluation — `Application`, `Conditional`, `Mapped`, `IndexAccess`, or
/// `KeyOf`. Used by the checker to attribute the `tuple_too_large` flag to the
/// alias whose body owns the synthesis, not to a transitive referrer whose body
/// is a plain `Lazy` or already-materialized `Tuple`.
pub fn is_fresh_tuple_synthesis_site(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    matches!(
        types.lookup(type_id),
        Some(
            TypeData::Application(_)
                | TypeData::Conditional(_)
                | TypeData::Mapped(_)
                | TypeData::IndexAccess(_, _)
                | TypeData::KeyOf(_),
        )
    )
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

mod content;

pub use content::{
    constraint_references_type_param_identity_in_resolution_path,
    constraint_references_type_param_in_resolution_path, contains_any_type, contains_error_type,
    contains_free_infer_types, contains_free_type_parameters,
    contains_free_type_parameters_except_name, contains_infer_types, contains_this_type,
    contains_type_by_id, contains_type_matching, contains_type_parameter_binder,
    contains_type_parameter_identity_shallow, contains_type_parameter_named,
    contains_type_parameter_named_shallow, contains_type_parameters,
    contains_unknown_at_instantiation_positions, free_decl_scoped_type_parameter_origins_in,
    free_type_parameter_ids_in, mapped_context_references_type_param_binder,
    mapped_context_references_type_param_named, references_any_type_param_named,
    references_type_param_outside_id_set,
};
