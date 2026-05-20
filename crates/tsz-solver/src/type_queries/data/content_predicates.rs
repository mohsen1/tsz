//! Type content predicates and compound type extraction helpers.
//!
//! Contains `contains_*`, `is_*` predicates, union/intersection member access,
//! array/tuple extraction, and compound member mapping.

use crate::TypeDatabase;
use crate::def::DefinitionStore;
use crate::types::{IntrinsicKind, TypeData, TypeId};
use crate::visitors::visitor_predicates::contains_type_matching;
use rustc_hash::{FxHashMap, FxHashSet};
use tsz_common::interner::Atom;

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

/// Check if a type contains named type parameters or canonical bound
/// parameters, excluding in-flight `infer` placeholders and polymorphic `this`.
pub fn contains_named_or_bound_type_parameters_db(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    contains_type_matching(db, type_id, |key| {
        matches!(
            key,
            TypeData::TypeParameter(_) | TypeData::BoundParameter(_)
        )
    })
}

/// Like `contains_type_parameters_db`, but ignores references to a known
/// locally-bound mapped key parameter. See
/// [`contains_free_type_parameters_except_name`] for the leaf-treatment
/// rationale.
///
/// [`contains_free_type_parameters_except_name`]:
///     crate::visitors::visitor_predicates::contains_free_type_parameters_except_name
pub fn contains_type_parameters_except_name_db(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    excluded_name: Atom,
) -> bool {
    crate::visitors::visitor_predicates::contains_free_type_parameters_except_name(
        db,
        type_id,
        excluded_name,
    )
}

/// Check if a type contains an indexed access whose object is a type parameter.
pub fn contains_index_access_with_type_parameter_object(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> bool {
    contains_type_matching(
        db,
        type_id,
        |key| matches!(key, TypeData::IndexAccess(object, _) if crate::type_queries::is_type_parameter_like(db, *object)),
    )
}

/// Check if a type contains a generic indexed access surface.
pub fn contains_generic_indexed_access_surface(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    let Some(TypeData::IndexAccess(object, index)) = db.lookup(type_id) else {
        return false;
    };
    crate::type_queries::is_type_parameter_like(db, object)
        || contains_type_parameters_db(db, index)
}

/// Check if a type contains an indexed access whose object is a variadic tuple
/// rest element containing a type parameter.
pub fn contains_index_access_with_variadic_tuple_object(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> bool {
    contains_type_matching(db, type_id, |key| {
        matches!(
            key,
            TypeData::IndexAccess(object, _)
                if variadic_tuple_object_contains_type_parameter(db, *object)
        )
    })
}

fn variadic_tuple_object_contains_type_parameter(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    get_tuple_elements(db, type_id).is_some_and(|elems| {
        elems
            .iter()
            .any(|elem| elem.rest && contains_type_parameters_db(db, elem.type_id))
    })
}

/// Check if a type contains *free* type parameters — type parameters that are
/// not bound by an enclosing function/callable signature's own type parameter list.
///
/// When an object type (interface) has method members like `bar<W>(): Inner<W>`,
/// the `W` type parameter inside the method body is bound by `bar`'s signature.
/// The standard `contains_type_parameters_db` traverses into these bodies and
/// finds `W`, incorrectly reporting that the object type "contains type parameters".
///
/// This variant skips function/callable bodies that have their own type parameters,
/// since any type parameter references inside those bodies are (or should be) bound
/// by the function's own generic declaration, not free from an outer scope.
///
/// Used by TS2344 constraint validation to decide whether a base constraint can
/// be checked eagerly or must be deferred to instantiation time.
pub fn contains_free_type_parameters_db(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
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
    crate::visitors::visitor_predicates::contains_free_type_parameters(db, type_id)
}

/// Check if a type contains generic type parameters, excluding `ThisType`.
///
/// Like `contains_type_parameters_db`, but does NOT treat `ThisType` as a type
/// parameter. This is appropriate for TS2352 (type assertion overlap) checking,
/// where `this` resolves to the enclosing class type and should still be checked
/// for overlap — tsc does not suppress type assertion checks for `this`.
pub fn contains_generic_type_parameters_db(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    match db.lookup(type_id) {
        Some(TypeData::TypeParameter(_) | TypeData::Infer(_) | TypeData::BoundParameter(_)) => {
            return true;
        }
        Some(
            TypeData::Literal(_)
            | TypeData::Intrinsic(_)
            | TypeData::Error
            | TypeData::UniqueSymbol(_)
            | TypeData::ModuleNamespace(_)
            | TypeData::Recursive(_)
            | TypeData::Enum(_, _)
            | TypeData::ThisType,
        ) => return false,
        _ => {}
    }
    contains_type_matching(db, type_id, |key| {
        matches!(
            key,
            TypeData::TypeParameter(_) | TypeData::Infer(_) | TypeData::BoundParameter(_)
        )
    })
}

/// Check if a type is directly an `Infer` type (not recursive).
///
/// This is a lightweight O(1) check that only inspects the top-level type.
/// Use this when you need to guard against caching leaked Infer results
/// without the cost of a full recursive walk.
pub fn is_infer_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
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
    if let Some(cached) = db.contains_infer_types_cached(type_id) {
        return cached;
    }
    // Fast path: leaf types (Literal, Object, Function, etc.) that don't
    // contain nested types can't contain Infer. Only composite types
    // (Union, Intersection, Application, etc.) need traversal.
    let result = match db.lookup(type_id) {
        Some(TypeData::Infer(_)) => true,
        Some(TypeData::TypeParameter(tp)) => {
            let name = db.resolve_atom_ref(tp.name);
            name.starts_with("__infer_") || name.starts_with("__infer_src_")
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
        ) => false,
        _ => contains_type_matching(db, type_id, |key| match key {
            TypeData::Infer(_) => true,
            TypeData::TypeParameter(tp) => {
                let name = db.resolve_atom_ref(tp.name);
                name.starts_with("__infer_") || name.starts_with("__infer_src_")
            }
            _ => false,
        }),
    };
    db.set_contains_infer_types_cache(type_id, result);
    result
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
    if let Some(cached) = db.contains_type_query_cached(type_id) {
        return cached;
    }
    let result = match db.lookup(type_id) {
        Some(TypeData::TypeQuery(_)) => true,
        Some(
            TypeData::Literal(_)
            | TypeData::Intrinsic(_)
            | TypeData::Error
            | TypeData::ThisType
            | TypeData::UniqueSymbol(_)
            | TypeData::ModuleNamespace(_)
            | TypeData::BoundParameter(_)
            | TypeData::Recursive(_),
        ) => false,
        _ => contains_type_matching(db, type_id, |key| matches!(key, TypeData::TypeQuery(_))),
    };
    db.set_contains_type_query_cache(type_id, result);
    result
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
    if type_id.is_intrinsic() {
        return false;
    }
    match db.lookup(type_id) {
        Some(TypeData::Infer(_)) => true,
        Some(TypeData::TypeParameter(tp)) => {
            let name = db.resolve_atom_ref(tp.name);
            name.starts_with("__infer_") || name.starts_with("__infer_src_")
        }
        _ => false,
    }
}

/// Check whether a type is itself a bare call-local inference placeholder.
///
/// Higher-order generic function inference also creates `__infer_src_*`
/// placeholders for the generic parameters of a source function argument. Those
/// are not stale call-local placeholders: when they survive into a returned
/// function type they represent type parameters that should be hoisted.
pub fn is_bare_current_infer_placeholder_db(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    match db.lookup(type_id) {
        Some(TypeData::Infer(_)) => true,
        Some(TypeData::TypeParameter(tp)) => {
            let name = db.resolve_atom_ref(tp.name);
            name.starts_with("__infer_") && !name.starts_with("__infer_src_")
        }
        _ => false,
    }
}

/// Check if a type is a spread marker tuple created by the checker.
pub fn is_spread_marker_tuple(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    if let Some(TypeData::Tuple(elems_id)) = db.lookup(type_id) {
        let elems = db.tuple_list(elems_id);
        if elems.len() != 1 || !elems[0].rest {
            return false;
        }
        elems[0]
            .name
            .is_some_and(|name| db.resolve_atom(name) == "__tsz_spread_argument__")
            || matches!(
                db.lookup(elems[0].type_id),
                Some(TypeData::TypeParameter(_))
            )
    } else {
        false
    }
}

pub fn rest_type_needs_aggregate_argument_check(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    match db.lookup(type_id) {
        Some(TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner)) => {
            rest_type_needs_aggregate_argument_check(db, inner)
        }
        Some(TypeData::Union(members)) => db.type_list(members).iter().any(|&member| {
            let member = match db.lookup(member) {
                Some(TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner)) => inner,
                _ => member,
            };
            matches!(db.lookup(member), Some(TypeData::Tuple(_)))
                || rest_type_needs_aggregate_argument_check(db, member)
        }),
        Some(
            TypeData::TypeParameter(_)
            | TypeData::Application(_)
            | TypeData::Conditional(_)
            | TypeData::Intersection(_)
            | TypeData::Lazy(_)
            | TypeData::Mapped(_)
            | TypeData::Object(_)
            | TypeData::ObjectWithIndex(_)
            | TypeData::IndexAccess(_, _),
        ) => true,
        _ => false,
    }
}

/// This detects both bare placeholders and structural types that contain them
/// (e.g., unions like `__infer_0 | PromiseLike<__infer_0>`).
pub fn contains_infer_placeholder_db(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if is_bare_infer_placeholder_db(db, type_id) {
        return true;
    }
    contains_type_matching(db, type_id, |key| match key {
        TypeData::TypeParameter(tp) => {
            let name = db.resolve_atom_ref(tp.name);
            name.starts_with("__infer_") || name.starts_with("__infer_src_")
        }
        TypeData::Infer(_) => true,
        _ => false,
    })
}

/// Check if a type contains a call-local inference placeholder.
///
/// This intentionally excludes `__infer_src_*` placeholders because those carry
/// higher-order source generic parameters and are normalized or hoisted later.
pub fn contains_current_infer_placeholder_db(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if is_bare_current_infer_placeholder_db(db, type_id) {
        return true;
    }
    contains_type_matching(db, type_id, |key| match key {
        TypeData::TypeParameter(tp) => {
            let name = db.resolve_atom_ref(tp.name);
            name.starts_with("__infer_") && !name.starts_with("__infer_src_")
        }
        TypeData::Infer(_) => true,
        _ => false,
    })
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
    contains_type_matching(db, type_id, |key| {
        matches!(key, TypeData::Error | TypeData::UnresolvedTypeName(_))
    })
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

/// Check whether a type is "deeply any" — i.e. `any` itself, or a composite
/// (array, tuple, union, intersection) whose leaf elements are all `any`.
///
/// This is used during generic inference to detect when a round-1 inference
/// result is effectively `any` so the checker can fall back to better
/// contextual information.
pub fn is_type_deeply_any(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    fn walk(
        db: &dyn TypeDatabase,
        type_id: TypeId,
        visiting: &mut FxHashSet<TypeId>,
        memo: &mut FxHashMap<TypeId, bool>,
    ) -> bool {
        if let Some(&cached) = memo.get(&type_id) {
            return cached;
        }
        if !visiting.insert(type_id) {
            // Cycle while evaluating "all leaves are any" is conservatively false.
            return false;
        }
        let result = if type_id == TypeId::ANY {
            true
        } else if type_id.is_intrinsic() {
            // Non-ANY intrinsics resolve to TypeData::Intrinsic and are
            // never Array/Tuple/Union/Intersection — skip the dyn lookup.
            false
        } else {
            match db.lookup(type_id) {
                Some(TypeData::Array(elem)) => walk(db, elem, visiting, memo),
                Some(TypeData::Tuple(list_id)) => {
                    let elems = db.tuple_list(list_id);
                    elems.iter().all(|e| walk(db, e.type_id, visiting, memo))
                }
                Some(TypeData::Union(list_id)) => {
                    let members = db.type_list(list_id);
                    !members.is_empty() && members.iter().all(|&m| walk(db, m, visiting, memo))
                }
                Some(TypeData::Intersection(list_id)) => {
                    let members = db.type_list(list_id);
                    !members.is_empty() && members.iter().all(|&m| walk(db, m, visiting, memo))
                }
                _ => false,
            }
        };
        visiting.remove(&type_id);
        memo.insert(type_id, result);
        result
    }
    let mut visiting = FxHashSet::default();
    let mut memo = FxHashMap::default();
    walk(db, type_id, &mut visiting, &mut memo)
}

/// Check whether a type (or any union/intersection/readonly/noinfer wrapper)
/// contains an `Application` type.
///
/// Used to decide whether contextual instantiation results should be preserved
/// in their unevaluated form so that generic type argument structure is retained
/// for downstream inference.
pub fn contains_application_in_structure(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    match db.lookup(type_id) {
        Some(TypeData::Application(_)) => true,
        Some(TypeData::Union(list_id)) => {
            let members = db.type_list(list_id);
            members
                .iter()
                .any(|&m| contains_application_in_structure(db, m))
        }
        Some(TypeData::Intersection(list_id)) => {
            let members = db.type_list(list_id);
            members
                .iter()
                .any(|&m| contains_application_in_structure(db, m))
        }
        Some(TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner)) => {
            contains_application_in_structure(db, inner)
        }
        _ => false,
    }
}

/// Return true when `type_id` (or any union/intersection member reachable from it)
/// is a `ConditionalType` whose `extends_type` is still an unevaluated
/// `Application` type.
pub fn contains_conditional_with_application_extends(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> bool {
    fn walk(db: &dyn TypeDatabase, type_id: TypeId, depth: u32) -> bool {
        if depth > 32 {
            return false;
        }
        if let Some(TypeData::Conditional(cond_id)) = db.lookup(type_id) {
            let cond = db.get_conditional(cond_id);
            if matches!(db.lookup(cond.extends_type), Some(TypeData::Application(_))) {
                return true;
            }
        }
        if let Some(TypeData::Union(list_id) | TypeData::Intersection(list_id)) = db.lookup(type_id)
        {
            let members = db.type_list(list_id);
            if members.iter().any(|&member| walk(db, member, depth + 1)) {
                return true;
            }
        }
        false
    }

    walk(db, type_id, 0)
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
    if type_id.is_intrinsic() {
        return None;
    }
    match db.lookup(type_id) {
        Some(TypeData::Union(list_id)) => {
            let members = db.type_list(list_id);
            Some(members.to_vec())
        }
        _ => None,
    }
}

/// Returns `true` if `type_id` is a union or intersection whose members are
/// all primitive intrinsics or literal types (string/number/boolean literals).
/// tsc expands such type aliases in error messages instead of preserving the
/// alias name — e.g. `type T2 = "a" | "b"` displays as `"a" | "b"`, not `T2`.
pub fn is_primitive_or_literal_compound(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    let members_id = match db.lookup(type_id) {
        Some(TypeData::Union(m)) | Some(TypeData::Intersection(m)) => m,
        _ => return false,
    };
    let members = db.type_list(members_id);
    members.iter().all(|m| {
        m.is_intrinsic()
            || matches!(
                db.lookup(*m),
                Some(TypeData::Literal(_) | TypeData::Intrinsic(_))
            )
    })
}

/// Returns `true` if `type_id` is itself a literal/primitive, or a union or
/// intersection composed entirely of literal/primitive members.
///
/// Used for diagnostic display: when a generic type alias application reduces
/// to such a "terminal" form (e.g. `KeysExtendedBy<M, number>` reducing to
/// `"b"`), tsc drops the alias and shows the resolved literal in error
/// messages. Object/interface results keep the alias form.
pub fn is_literal_or_primitive_or_compound_of_those(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> bool {
    if type_id.is_intrinsic() {
        return true;
    }
    match db.lookup(type_id) {
        Some(TypeData::Literal(_) | TypeData::Intrinsic(_)) => true,
        Some(TypeData::Union(_) | TypeData::Intersection(_)) => {
            is_primitive_or_literal_compound(db, type_id)
        }
        _ => false,
    }
}

/// Returns true when `type_id` is a literal type or a union whose members are
/// all literal types.
pub fn is_literal_or_literal_union_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    match db.lookup(type_id) {
        Some(TypeData::Literal(_)) => true,
        Some(TypeData::Union(list_id)) => db
            .type_list(list_id)
            .iter()
            .all(|&member| is_literal_or_literal_union_type(db, member)),
        _ => false,
    }
}

/// Get the members of an intersection type.
///
/// Returns None if the type is not an intersection.
pub fn get_intersection_members(db: &dyn TypeDatabase, type_id: TypeId) -> Option<Vec<TypeId>> {
    if type_id.is_intrinsic() {
        return None;
    }
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
    if type_id.is_intrinsic() {
        return None;
    }
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
    if type_id.is_intrinsic() {
        return None;
    }
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
    if type_id.is_intrinsic() {
        return None;
    }
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

/// Return true when a constraint admits a mutable array or tuple candidate.
///
/// Const type parameters preserve literal types, but when their declared
/// constraint is mutable-array-like (`T extends unknown[]`, or a union with a
/// mutable array member), array literal candidates must not be converted to
/// readonly tuples.
pub fn constraint_allows_mutable_array_like(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }

    match db.lookup(type_id) {
        Some(TypeData::Array(_)) => true,
        Some(TypeData::Tuple(list_id)) => !db.tuple_list(list_id).is_empty(),
        Some(TypeData::TypeParameter(info) | TypeData::Infer(info)) => info
            .constraint
            .is_some_and(|constraint| constraint_allows_mutable_array_like(db, constraint)),
        Some(TypeData::Union(list_id)) => db
            .type_list(list_id)
            .iter()
            .any(|&member| constraint_allows_mutable_array_like(db, member)),
        Some(TypeData::Application(_) | TypeData::Lazy(_)) => {
            let evaluated = crate::evaluation::evaluate::evaluate_type(db, type_id);
            evaluated != type_id && constraint_allows_mutable_array_like(db, evaluated)
        }
        _ => false,
    }
}

/// Get the element type for mutable array forms that are identical for TS2403.
///
/// This intentionally recognizes `T[]` and canonical `Array<T>` applications
/// before application evaluation erases the as-written `Array<T>` identity.
pub fn mutable_array_element_for_redeclaration(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    array_base: Option<TypeId>,
    definition_store: Option<&DefinitionStore>,
) -> Option<TypeId> {
    if type_id.is_intrinsic() {
        return None;
    }

    match db.lookup(type_id) {
        Some(TypeData::Array(elem)) => Some(elem),
        Some(TypeData::Application(app_id)) => {
            let app = db.type_application(app_id);
            (is_array_application_base_for_redeclaration(
                db,
                app.base,
                array_base,
                definition_store,
            ) && app.args.len() == 1)
                .then_some(app.args[0])
        }
        _ => None,
    }
}

fn is_array_application_base_for_redeclaration(
    db: &dyn TypeDatabase,
    base: TypeId,
    array_base: Option<TypeId>,
    definition_store: Option<&DefinitionStore>,
) -> bool {
    let array_base = array_base.or_else(|| db.get_array_base_type());
    let array_display_base = db.get_array_display_base_type();
    if array_base == Some(base)
        || array_display_base.is_some_and(|display_base| display_base == base)
    {
        return true;
    }

    db.get_display_alias(base).is_some_and(|alias| {
        array_base == Some(alias)
            || array_display_base.is_some_and(|display_base| display_base == alias)
    }) || lazy_base_names_array(db, definition_store, base)
}

fn lazy_base_names_array(
    db: &dyn TypeDatabase,
    definition_store: Option<&DefinitionStore>,
    base: TypeId,
) -> bool {
    let (Some(definition_store), Some(TypeData::Lazy(def_id))) =
        (definition_store, db.lookup(base))
    else {
        return false;
    };

    definition_store
        .get(def_id)
        .is_some_and(|def| db.resolve_atom_ref(def.name).as_ref() == "Array")
}

/// Get the elements of a tuple type.
///
/// Returns None if the type is not a tuple.
/// Returns a vector of (`TypeId`, optional, rest, name) tuples.
pub fn get_tuple_elements(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<Vec<crate::types::TupleElement>> {
    if type_id.is_intrinsic() {
        return None;
    }
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
    if type_id.is_intrinsic() {
        return false;
    }
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
    if type_id.is_intrinsic() {
        return false;
    }
    match db.lookup(type_id) {
        Some(TypeData::Union(list_id)) => {
            let members = db.type_list(list_id);
            members.iter().any(|&m| {
                !m.is_intrinsic()
                    && matches!(
                        db.lookup(m),
                        Some(TypeData::TypeParameter(_) | TypeData::Infer(_))
                    )
            })
        }
        _ => false,
    }
}
