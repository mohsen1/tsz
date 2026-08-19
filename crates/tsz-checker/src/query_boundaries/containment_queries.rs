//! Containment / traversal queries exposed at the query boundary.
//!
//! Recursive "does this type transitively contain X" predicates and
//! reachability collectors (`contains_*` / `collect_*` / `walk_*`), each
//! delegating to the owning solver visitor or type query. Moved out of the
//! broad `common` quarantine as an #8225 paydown slice; `common` re-exports
//! every name so call sites are unchanged.

use tsz_solver::TypeId;
use tsz_solver::construction::TypeDatabase;

pub(crate) fn contains_keyof_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::contains_keyof_type(db, type_id)
}

pub(crate) fn contains_type_parameters(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::visitor::contains_type_parameters(db, type_id)
}

pub(crate) fn contains_generic_indexed_access_surface(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> bool {
    tsz_solver::type_queries::contains_generic_indexed_access_surface(db, type_id)
}

pub(crate) fn contains_free_type_parameters(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::contains_free_type_parameters_db(db, type_id)
}

pub(crate) fn contains_generic_type_parameters(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::contains_generic_type_parameters_db(db, type_id)
}

pub(crate) fn contains_file_relative_content(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::contains_file_relative_content_db(db, type_id)
}

pub(crate) fn contains_lazy_or_recursive(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::contains_lazy_or_recursive_db(db, type_id)
}

pub(crate) fn contains_application_in_structure(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::contains_application_in_structure(db, type_id)
}

pub(crate) fn contains_error_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::contains_error_type_db(db, type_id)
}

/// Like `contains_error_type`, but also detects `TypeId::ERROR` nested in
/// Application arguments. The visitor checks the error sentinel before the
/// intrinsic fast path, which is needed for manually-lowered overload types.
pub(crate) fn contains_error_type_in_args(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::visitor::contains_error_type(db, type_id)
}

pub(crate) fn contains_never_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::contains_never_type_db(db, type_id)
}

/// Recursively check if a type contains a conditional type along a projection
/// path (union/intersection members, generic application arguments, indexed
/// access components). See `shape_contains_conditional_type_db` for scope.
pub(crate) fn contains_conditional_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::shape_contains_conditional_type_db(db, type_id)
}

pub(crate) fn collect_referenced_types(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> rustc_hash::FxHashSet<TypeId> {
    tsz_solver::visitor::collect_referenced_types(db, type_id)
}

pub(crate) fn contains_infer_types(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::contains_infer_types_db(db, type_id)
}

pub(crate) fn contains_current_infer_placeholder(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::contains_current_infer_placeholder_db(db, type_id)
}

/// Check if a type transitively references any type parameter whose name is in the given set.
pub(crate) fn references_any_type_param_named(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    names: &rustc_hash::FxHashSet<tsz_common::interner::Atom>,
) -> bool {
    tsz_solver::references_any_type_param_named(db, type_id, names)
}

/// Check if a type transitively contains a type parameter with the given name.
pub(crate) fn contains_type_parameter_named(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    name: tsz_common::interner::Atom,
) -> bool {
    tsz_solver::contains_type_parameter_named(db, type_id, name)
}

/// Check if a type transitively contains a specific `TypeId`.
pub(crate) fn contains_type_by_id(db: &dyn TypeDatabase, type_id: TypeId, target: TypeId) -> bool {
    tsz_solver::contains_type_by_id(db, type_id, target)
}

/// Whether a generic call's resolved return type is still *unresolved* — it
/// mentions a type parameter, an `infer` placeholder, or `unknown`.
pub(crate) fn return_type_is_unresolved(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    contains_type_parameters(db, type_id)
        || contains_infer_types(db, type_id)
        || contains_type_by_id(db, type_id, TypeId::UNKNOWN)
}

/// Collect all types recursively reachable from a root type.
pub(crate) fn collect_all_types(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> rustc_hash::FxHashSet<TypeId> {
    tsz_solver::visitor::collect_all_types(db, type_id)
}

pub(crate) fn type_contains_string_literal(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::type_contains_string_literal(db, type_id)
}

pub(crate) fn collect_lazy_def_ids(
    db: &dyn TypeDatabase,
    root: TypeId,
) -> Vec<tsz_solver::def::DefId> {
    tsz_solver::visitor::collect_lazy_def_ids(db, root)
}

/// If `root` is a (possibly nested) union of intrinsics and *bare*
/// `Lazy(DefId)` references with no other structure, return the de-duplicated
/// referenced `DefId`s; otherwise `None`. See
/// [`tsz_solver::visitor::union_of_bare_lazy_def_ids`].
pub(crate) fn union_of_bare_lazy_def_ids(
    db: &dyn TypeDatabase,
    root: TypeId,
) -> Option<Vec<tsz_solver::def::DefId>> {
    tsz_solver::visitor::union_of_bare_lazy_def_ids(db, root)
}

pub(crate) fn collect_type_queries(
    db: &dyn TypeDatabase,
    root: TypeId,
) -> Vec<tsz_solver::SymbolRef> {
    tsz_solver::visitor::collect_type_queries(db, root)
}

pub(crate) fn walk_referenced_types<F>(db: &dyn TypeDatabase, type_id: TypeId, visitor: F)
where
    F: FnMut(TypeId),
{
    tsz_solver::visitor::walk_referenced_types(db, type_id, visitor)
}

pub(crate) fn contains_this_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::contains_this_type(db, type_id)
}

pub(crate) fn type_contains_undefined(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::narrowing::type_contains_undefined(db, type_id)
}

pub(crate) fn constraint_references_type_param_in_resolution_path(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    param_name: tsz_common::interner::Atom,
) -> bool {
    tsz_solver::constraint_references_type_param_in_resolution_path(db, type_id, param_name)
}

pub(crate) fn has_deferred_conditional_member(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::has_deferred_conditional_member(db, type_id)
}

pub(crate) fn contains_index_access_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::contains_index_access_type(db, type_id)
}
