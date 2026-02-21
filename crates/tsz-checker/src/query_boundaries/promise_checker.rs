use tsz_solver::{TypeDatabase, TypeId};

pub(crate) use super::common::{lazy_def_id, union_members};
pub(crate) use tsz_solver::type_queries_extended::PromiseTypeKind;

pub(crate) fn classify_promise_type(db: &dyn TypeDatabase, type_id: TypeId) -> PromiseTypeKind {
    tsz_solver::type_queries::classify_promise_type(db, type_id)
}

pub(crate) fn type_application(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<tsz_solver::TypeApplication>> {
    tsz_solver::type_queries::get_type_application(db, type_id)
}
