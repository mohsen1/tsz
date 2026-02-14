use tsz_solver::{ObjectShape, TypeDatabase, TypeId};

pub(crate) fn is_object_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_object_type(db, type_id)
}

pub(crate) fn object_shape_for_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<ObjectShape>> {
    tsz_solver::type_queries::get_object_shape(db, type_id)
}
