use tsz_solver::{ObjectShape, TupleElement, TypeDatabase, TypeId};

pub(crate) fn union_members_for_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<Vec<TypeId>> {
    tsz_solver::type_queries::get_union_members(db, type_id)
}

pub(crate) fn object_shape_for_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<ObjectShape>> {
    tsz_solver::type_queries::get_object_shape(db, type_id)
}

pub(crate) fn tuple_elements_for_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<Vec<TupleElement>> {
    tsz_solver::type_queries::get_tuple_elements(db, type_id)
}
