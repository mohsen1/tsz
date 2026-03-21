//! Shared type query boundary functions used across multiple boundary modules.
//!
//! When a solver query is needed by multiple checker modules, define the
//! canonical thin-wrapper here and re-export it from the per-module boundary
//! files. This eliminates duplicate function bodies while preserving the
//! per-module namespace pattern that callers rely on.

use tsz_solver::{
    CallSignature, CallableShape, ObjectShape, TupleElement, TypeApplication, TypeDatabase, TypeId,
};

pub(crate) use tsz_solver::type_queries::TypeTraversalKind;

/// Re-export of the solver's property access result type.
///
/// Wraps `tsz_solver::operations::property::PropertyAccessResult`.
/// This is the result enum returned by property access evaluation in the solver.
pub(crate) use tsz_solver::operations::property::PropertyAccessResult;

/// Re-export of the solver's type substitution mapping.
///
/// Wraps `tsz_solver::TypeSubstitution`.
/// Used to build type parameter -> type argument mappings for instantiation.
pub(crate) use tsz_solver::TypeSubstitution;

/// Re-export of the solver's call resolution result type.
///
/// Wraps `tsz_solver::CallResult`.
/// This is the result enum returned by call/new expression resolution.
pub(crate) use tsz_solver::CallResult;

/// Thin wrapper around `tsz_solver::instantiate_type`.
///
/// Applies a `TypeSubstitution` to a type, producing a new type with type
/// parameters replaced by their corresponding type arguments.
pub(crate) fn instantiate_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    substitution: &tsz_solver::TypeSubstitution,
) -> TypeId {
    tsz_solver::instantiate_type(db, type_id, substitution)
}

pub(crate) fn callable_shape_for_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<CallableShape>> {
    tsz_solver::type_queries::get_callable_shape(db, type_id)
}

pub(crate) fn classify_for_traversal(db: &dyn TypeDatabase, type_id: TypeId) -> TypeTraversalKind {
    tsz_solver::type_queries::classify_for_traversal(db, type_id)
}

pub(crate) fn has_function_shape(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::get_function_shape(db, type_id).is_some()
}

pub(crate) fn union_members(db: &dyn TypeDatabase, type_id: TypeId) -> Option<Vec<TypeId>> {
    tsz_solver::type_queries::get_union_members(db, type_id)
}

pub(crate) fn is_type_parameter_like(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_type_parameter_like(db, type_id)
}

pub(crate) fn is_keyof_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_keyof_type(db, type_id)
}

pub(crate) fn is_index_access_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_index_access_type(db, type_id)
}

pub(crate) fn contains_type_parameters(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::contains_type_parameters_db(db, type_id)
}

pub(crate) fn contains_lazy_or_recursive(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::contains_lazy_or_recursive_db(db, type_id)
}

pub(crate) fn contains_error_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::contains_error_type_db(db, type_id)
}

pub(crate) fn contains_never_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::contains_never_type_db(db, type_id)
}

pub(crate) fn is_string_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_string_type(db, type_id)
}

pub(crate) fn lazy_def_id(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<tsz_solver::def::DefId> {
    tsz_solver::type_queries::get_lazy_def_id(db, type_id)
}

pub(crate) fn has_construct_signatures(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::has_construct_signatures(db, type_id)
}

pub(crate) fn type_parameter_default(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    tsz_solver::type_queries::get_type_parameter_default(db, type_id)
}

pub(crate) fn is_mapped_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_mapped_type(db, type_id)
}

/// Check if a type is a *generic* mapped type — one whose key constraint still
/// contains type parameters (e.g., `{ [K in keyof T]: ... }` where T is unresolved).
/// Mapped types with concrete key types (like `Partial<ConcreteType>`) return false
/// because they resolve to object types with statically known members.
/// This matches tsc's `isGenericMappedType`.
pub(crate) fn is_generic_mapped_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if let Some(mapped) = tsz_solver::type_queries::get_mapped_type(db, type_id) {
        // Match tsc's isGenericMappedType: only check constraint and name_type.
        // The template always contains the mapped type's own iteration variable
        // which is NOT an external type parameter.
        tsz_solver::type_queries::contains_type_parameters_db(db, mapped.constraint)
            || mapped
                .name_type
                .is_some_and(|nt| tsz_solver::type_queries::contains_type_parameters_db(db, nt))
    } else {
        false
    }
}

pub(crate) fn construct_signatures_for_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<Vec<CallSignature>> {
    tsz_solver::type_queries::get_construct_signatures(db, type_id)
}

pub(crate) fn is_generic_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_generic_type(db, type_id)
}

pub(crate) fn tuple_elements(db: &dyn TypeDatabase, type_id: TypeId) -> Option<Vec<TupleElement>> {
    tsz_solver::type_queries::get_tuple_elements(db, type_id)
}

pub(crate) fn call_signatures_for_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<Vec<CallSignature>> {
    tsz_solver::type_queries::get_call_signatures(db, type_id)
}

pub(crate) fn object_shape_for_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<ObjectShape>> {
    tsz_solver::type_queries::get_object_shape(db, type_id)
}

pub(crate) fn array_element_type(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    tsz_solver::type_queries::get_array_element_type(db, type_id)
}

pub(crate) fn intersection_members(db: &dyn TypeDatabase, type_id: TypeId) -> Option<Vec<TypeId>> {
    tsz_solver::type_queries::get_intersection_members(db, type_id)
}

pub(crate) fn is_unit_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_unit_type(db, type_id)
}

pub(crate) fn is_symbol_or_unique_symbol(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_symbol_or_unique_symbol(db, type_id)
}

pub(crate) fn unwrap_readonly(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    tsz_solver::type_queries::unwrap_readonly(db, type_id)
}

// ── Type application query ──

pub(crate) fn type_application(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<TypeApplication>> {
    tsz_solver::type_queries::get_type_application(db, type_id)
}

// ── Evaluation classification ──

pub(crate) use tsz_solver::type_queries::EvaluationNeeded;

pub(crate) fn classify_for_evaluation(db: &dyn TypeDatabase, type_id: TypeId) -> EvaluationNeeded {
    tsz_solver::type_queries::classify_for_evaluation(db, type_id)
}

// ── Predicate / narrowing classification ──

pub(crate) use tsz_solver::type_queries::PredicateSignatureKind;

pub(crate) fn classify_for_predicate_signature(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> PredicateSignatureKind {
    tsz_solver::type_queries::classify_for_predicate_signature(db, type_id)
}

pub(crate) fn is_narrowing_literal(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    tsz_solver::type_queries::is_narrowing_literal(db, type_id)
}

pub(crate) fn stringify_literal_type(db: &dyn TypeDatabase, type_id: TypeId) -> Option<String> {
    tsz_solver::type_queries::stringify_literal_type(db, type_id)
}

// ── Visitor wrappers ──

pub(crate) fn collect_referenced_types(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> rustc_hash::FxHashSet<TypeId> {
    tsz_solver::visitor::collect_referenced_types(db, type_id)
}

pub(crate) fn collect_enum_def_ids(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Vec<tsz_solver::def::DefId> {
    tsz_solver::visitor::collect_enum_def_ids(db, type_id)
}
