use tsz_solver::{TypeDatabase, TypeId};

pub(crate) fn has_call_signatures(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::has_call_signatures(db, type_id)
}
