use tsz_solver::TypeId;
use tsz_solver::construction::TypeDatabase;
use tsz_solver::relations::subtype::TypeResolver;

pub(crate) use super::common::{intersection_members, is_type_parameter_like, union_members};

pub(crate) fn is_object_like_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_object_like_type(db, type_id)
}

pub(crate) fn get_index_access_types(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<(TypeId, TypeId)> {
    tsz_solver::type_queries::get_index_access_types(db, type_id)
}

pub(crate) fn evaluate_type_with_resolver<R: TypeResolver>(
    db: &dyn TypeDatabase,
    resolver: &R,
    type_id: TypeId,
) -> TypeId {
    let mut evaluator = tsz_solver::computation::TypeEvaluator::with_resolver(db, resolver);
    evaluator.evaluate(type_id)
}
