use tsz_solver::{ObjectShape, TypeDatabase, TypeId};

pub(crate) use tsz_solver::type_queries::{
    AssignabilityEvalKind, ExcessPropertiesKind, TypeTraversalKind,
};

pub(crate) fn classify_for_traversal(db: &dyn TypeDatabase, type_id: TypeId) -> TypeTraversalKind {
    tsz_solver::type_queries::classify_for_traversal(db, type_id)
}

pub(crate) fn classify_for_assignability_eval(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> AssignabilityEvalKind {
    tsz_solver::type_queries::classify_for_assignability_eval(db, type_id)
}

pub(crate) fn is_callable_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_callable_type(db, type_id)
}

pub(crate) fn object_shape_for_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<ObjectShape>> {
    tsz_solver::type_queries::get_object_shape(db, type_id)
}

pub(crate) fn classify_for_excess_properties(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> ExcessPropertiesKind {
    tsz_solver::type_queries::classify_for_excess_properties(db, type_id)
}
