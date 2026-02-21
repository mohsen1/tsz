use tsz_common::Atom;
#[cfg(test)]
use tsz_solver::TupleElement;
use tsz_solver::{TypeDatabase, TypeId};

pub(crate) use super::common::{
    array_element_type, contains_type_parameters, intersection_members, is_mapped_type,
    is_string_type, is_type_parameter, object_shape_for_type as object_shape, tuple_elements,
    union_members,
};

pub(crate) fn extract_string_literal_keys(db: &dyn TypeDatabase, type_id: TypeId) -> Vec<Atom> {
    tsz_solver::type_queries::extract_string_literal_keys(db, type_id)
}

pub(crate) fn unwrap_readonly_deep(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    tsz_solver::type_queries::unwrap_readonly_deep(db, type_id)
}

pub(crate) fn is_only_null_or_undefined(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_only_null_or_undefined(db, type_id)
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

#[cfg(test)]
#[path = "../../tests/state_checking.rs"]
mod tests;
