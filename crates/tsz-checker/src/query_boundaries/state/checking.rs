use tsz_common::Atom;
#[cfg(test)]
use tsz_solver::TupleElement;
use tsz_solver::{TypeDatabase, TypeId};

pub(crate) use super::super::common::{
    array_element_type, callable_shape_for_type as callable_shape, intersection_members,
    is_mapped_type, is_string_type, is_type_parameter_like, is_unit_type,
    object_shape_for_type as object_shape, tuple_elements, union_members,
};

pub(crate) fn extract_string_literal_keys(db: &dyn TypeDatabase, type_id: TypeId) -> Vec<Atom> {
    tsz_solver::type_queries::extract_string_literal_keys(db, type_id)
}

pub(crate) fn keyof_target(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    tsz_solver::type_queries::get_keyof_type(db, type_id)
}

pub(crate) fn unwrap_readonly_deep(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    tsz_solver::type_queries::unwrap_readonly_deep(db, type_id)
}

/// Strict type parameter check: matches `TypeParameter` and `Infer` only.
///
/// Unlike `is_type_parameter_like` (which also matches `BoundParameter`),
/// this returns true only for free type parameters. Used in readonly
/// checking to detect generic indexed writes on unconstrained type params.
pub(crate) fn is_type_parameter(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::is_type_parameter(db, type_id)
}

pub(crate) fn is_object_like_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_object_like_type(db, type_id)
}

pub(crate) fn find_property_in_object_by_str(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    property: &str,
) -> Option<tsz_solver::PropertyInfo> {
    tsz_solver::type_queries::find_property_in_object_by_str(db, type_id, property)
}

pub(crate) fn has_type_query_for_symbol<F>(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    target_sym_id: u32,
    resolve_lazy: F,
) -> bool
where
    F: FnMut(TypeId) -> TypeId,
{
    tsz_solver::type_queries::has_type_query_for_symbol(db, type_id, target_sym_id, resolve_lazy)
}

pub(crate) fn needs_env_eval(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(
        tsz_solver::type_queries::classify_for_assignability_eval(db, type_id),
        tsz_solver::type_queries::AssignabilityEvalKind::NeedsEnvEval
    )
}

pub(crate) fn type_parameter_constraint(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    tsz_solver::type_queries::get_type_parameter_constraint(db, type_id)
}

pub(crate) fn instantiate_mapped_template_for_property(
    db: &dyn TypeDatabase,
    template: TypeId,
    type_param_name: Atom,
    key_literal: TypeId,
) -> TypeId {
    tsz_solver::type_queries::instantiate_mapped_template_for_property(
        db,
        template,
        type_param_name,
        key_literal,
    )
}

pub(crate) fn collect_finite_mapped_property_names(
    db: &dyn TypeDatabase,
    mapped_id: tsz_solver::MappedTypeId,
) -> Option<rustc_hash::FxHashSet<Atom>> {
    tsz_solver::type_queries::collect_finite_mapped_property_names(db, mapped_id)
}

pub(crate) fn get_finite_mapped_property_type(
    db: &dyn TypeDatabase,
    mapped_id: tsz_solver::MappedTypeId,
    property_name: &str,
) -> Option<TypeId> {
    tsz_solver::type_queries::get_finite_mapped_property_type(db, mapped_id, property_name)
}

#[cfg(test)]
#[path = "../../../tests/state_checking.rs"]
mod tests;
