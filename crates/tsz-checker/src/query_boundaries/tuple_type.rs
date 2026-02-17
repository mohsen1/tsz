use tsz_solver::{TypeDatabase, TypeId};

pub(crate) fn tuple_elements(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<Vec<tsz_solver::TupleElement>> {
    tsz_solver::type_queries::get_tuple_elements(db, type_id)
}
