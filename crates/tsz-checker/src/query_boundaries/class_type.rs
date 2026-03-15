use tsz_solver::{TypeDatabase, TypeId};

pub(crate) use super::common::{
    callable_shape_for_type, construct_signatures_for_type, has_function_shape,
    intersection_members, is_generic_mapped_type, is_generic_type, object_shape_for_type,
};

pub(crate) fn function_shape(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<tsz_solver::FunctionShape>> {
    tsz_solver::type_queries::get_function_shape(db, type_id)
}

pub(crate) fn type_includes_undefined(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::type_includes_undefined(db, type_id)
}

pub(crate) fn type_parameter_constraint(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    tsz_solver::type_queries::get_type_parameter_constraint(db, type_id)
}
