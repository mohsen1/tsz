use tsz_solver::{ConditionalType, TypeDatabase, TypeId};

pub(crate) fn conditional_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<ConditionalType>> {
    tsz_solver::type_queries::get_conditional_type(db, type_id)
}
