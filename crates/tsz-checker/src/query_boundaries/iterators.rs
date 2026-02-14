use tsz_solver::{TypeDatabase, TypeId};

pub(crate) fn function_return_type(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    tsz_solver::type_queries::get_function_return_type(db, type_id)
}

pub(crate) fn is_promise_like(db: &dyn tsz_solver::QueryDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_promise_like(db, type_id)
}

pub(crate) fn is_valid_for_in_target(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_valid_for_in_target(db, type_id)
}
