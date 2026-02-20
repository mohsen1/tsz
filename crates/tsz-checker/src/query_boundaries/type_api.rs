use tsz_solver::{TypeDatabase, TypeId};

pub(crate) fn is_array_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_array_type(db, type_id)
}
