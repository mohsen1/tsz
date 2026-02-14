use tsz_solver::{TypeDatabase, TypeId};

pub(crate) fn is_object_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_object_type(db, type_id)
}

pub(crate) fn is_array_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_array_type(db, type_id)
}

pub(crate) fn is_tuple_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_tuple_type(db, type_id)
}

pub(crate) fn is_literal_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_literal_type(db, type_id)
}
