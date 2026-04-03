use tsz_solver::{FunctionShape, QueryDatabase, TypeDatabase, TypeId};

pub(crate) use super::common::PropertyAccessResult;
pub(crate) use super::common::{
    array_element_type, callable_shape_for_type as callable_shape, is_string_type, unwrap_readonly,
};

/// Resolve a named property on a type through the solver's property evaluator.
///
/// This is the canonical boundary for property access resolution. Checker code
/// must use this instead of directly instantiating `PropertyAccessEvaluator`.
pub(crate) fn resolve_property_access(
    db: &dyn QueryDatabase,
    obj_type: TypeId,
    prop_name: &str,
) -> PropertyAccessResult {
    let evaluator = tsz_solver::operations::property::PropertyAccessEvaluator::new(db);
    evaluator.resolve_property_access(obj_type, prop_name)
}

/// Like [`resolve_property_access`] but preserves raw `ThisType` in the result.
///
/// When `skip_this_binding` is set, the solver does not eagerly bind `this` to
/// the structural object shape. The caller can then substitute `this` with the
/// correct nominal receiver type (e.g., the class type instead of the flattened
/// intersection shape).
pub(crate) fn resolve_property_access_raw_this(
    db: &dyn QueryDatabase,
    obj_type: TypeId,
    prop_name: &str,
) -> PropertyAccessResult {
    let evaluator = tsz_solver::operations::property::PropertyAccessEvaluator::new(db);
    evaluator.set_skip_this_binding(true);
    evaluator.resolve_property_access(obj_type, prop_name)
}

pub(crate) fn is_function_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_function_type(db, type_id)
}

pub(crate) fn tuple_element_type_union(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    tsz_solver::type_queries::get_tuple_element_type_union(db, type_id)
}

pub(crate) fn application_first_arg(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    tsz_solver::type_queries::get_type_application(db, type_id)?
        .args
        .first()
        .copied()
}

pub(crate) fn is_boolean_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_boolean_type(db, type_id)
}

pub(crate) fn is_number_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_number_type(db, type_id)
}

pub(crate) fn is_symbol_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_symbol_type(db, type_id)
}

pub(crate) fn is_bigint_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_bigint_type(db, type_id)
}

pub(crate) fn def_id(db: &dyn TypeDatabase, type_id: TypeId) -> Option<tsz_solver::def::DefId> {
    tsz_solver::type_queries::get_def_id(db, type_id)
}

pub(crate) fn type_parameter_constraint(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    tsz_solver::type_queries::get_type_parameter_constraint(db, type_id)
}

pub(crate) fn enum_def_id(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<tsz_solver::def::DefId> {
    tsz_solver::type_queries::get_enum_def_id(db, type_id)
}

pub(crate) fn function_shape(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<FunctionShape>> {
    tsz_solver::type_queries::get_function_shape(db, type_id)
}

/// Check if a type has a named property accessible on all branches.
///
/// For unions, returns true only when ALL members have the property.
/// Used by TS2702/TS2713 diagnostic distinction.
pub(crate) fn type_has_property(db: &dyn TypeDatabase, type_id: TypeId, name: &str) -> bool {
    tsz_solver::type_queries::type_has_property_by_str(db, type_id, name)
}

/// Check if a type is the polymorphic `this` type.
///
/// Used during property access resolution to suppress TS2339 when `this`
/// comes from a ThisType marker (e.g., Vue 2 Options API pattern).
pub(crate) fn is_this_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_this_type(db, type_id)
}

/// Check if a type contains `never` (e.g. an intersection reduced to `never`).
///
/// Used to detect cases where property access should return `error` to suppress
/// cascading diagnostics (matching tsc behavior for `never` types).
pub(crate) fn contains_never_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::contains_never_type_db(db, type_id)
}

/// Extract object and index types from an IndexAccess type (T[K]).
///
/// Returns `None` if `type_id` is not an `IndexAccess` type.
pub(crate) fn index_access_types(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<(TypeId, TypeId)> {
    tsz_solver::type_queries::get_index_access_types(db, type_id)
}

#[cfg(test)]
#[path = "../../tests/property_access_boundaries.rs"]
mod tests;
