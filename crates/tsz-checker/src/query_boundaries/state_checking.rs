use tsz_common::Atom;
use tsz_solver::{ObjectShape, TupleElement, TypeDatabase, TypeId};

pub(crate) fn union_members(db: &dyn TypeDatabase, type_id: TypeId) -> Option<Vec<TypeId>> {
    tsz_solver::type_queries::get_union_members(db, type_id)
}

pub(crate) fn intersection_members(db: &dyn TypeDatabase, type_id: TypeId) -> Option<Vec<TypeId>> {
    tsz_solver::type_queries::get_intersection_members(db, type_id)
}

pub(crate) fn is_type_parameter(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_type_parameter(db, type_id)
}

pub(crate) fn is_mapped_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_mapped_type(db, type_id)
}

pub(crate) fn contains_type_parameters(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::contains_type_parameters_db(db, type_id)
}

pub(crate) fn is_string_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_string_type(db, type_id)
}

pub(crate) fn extract_string_literal_keys(db: &dyn TypeDatabase, type_id: TypeId) -> Vec<Atom> {
    tsz_solver::type_queries::extract_string_literal_keys(db, type_id)
}

pub(crate) fn array_element_type(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    tsz_solver::type_queries::get_array_element_type(db, type_id)
}

pub(crate) fn object_shape(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<ObjectShape>> {
    tsz_solver::type_queries::get_object_shape(db, type_id)
}

pub(crate) fn tuple_elements(db: &dyn TypeDatabase, type_id: TypeId) -> Option<Vec<TupleElement>> {
    tsz_solver::type_queries::get_tuple_elements(db, type_id)
}

pub(crate) fn unwrap_readonly_deep(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    tsz_solver::type_queries::unwrap_readonly_deep(db, type_id)
}

#[cfg(test)]
#[path = "../../tests/state_checking.rs"]
mod tests;
