use tsz_solver::{CallableShape, TypeDatabase, TypeId};

pub(crate) use tsz_solver::type_queries::TypeArgumentExtractionKind;

pub(crate) fn classify_for_type_argument_extraction(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> TypeArgumentExtractionKind {
    tsz_solver::type_queries::classify_for_type_argument_extraction(db, type_id)
}

pub(crate) fn contains_type_parameters(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::contains_type_parameters_db(db, type_id)
}

pub(crate) fn callable_shape_for_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<CallableShape>> {
    tsz_solver::type_queries::get_callable_shape(db, type_id)
}
