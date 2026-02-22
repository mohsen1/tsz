use tsz_solver::{FunctionShape, TypeDatabase, TypeId};

pub(crate) use super::common::{
    array_element_type, callable_shape_for_type as callable_shape, is_string_type,
};

pub(crate) fn is_function_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_function_type(db, type_id)
}

pub(crate) fn unwrap_readonly(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    tsz_solver::type_queries::unwrap_readonly(db, type_id)
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

pub(crate) fn function_shape(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<FunctionShape>> {
    tsz_solver::type_queries::get_function_shape(db, type_id)
}

#[cfg(test)]
#[path = "../../tests/property_access_boundaries.rs"]
mod tests;
