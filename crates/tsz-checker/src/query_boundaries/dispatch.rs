use tsz_solver::TypeId;
use tsz_solver::construction::{QueryDatabase, TypeDatabase};
use tsz_solver::def::DefId;
use tsz_solver::relations::subtype::TypeResolver;

pub(crate) use super::common::{
    get_base_constraint_of_type, intersection_members, is_type_parameter_like, union_members,
};

pub(crate) fn is_object_like_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_object_like_type(db, type_id)
}

pub(crate) fn is_callable_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_callable_type(db, type_id)
}

pub(crate) fn get_index_access_types(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<(TypeId, TypeId)> {
    tsz_solver::type_queries::get_index_access_types(db, type_id)
}

pub(crate) fn evaluate_type_with_resolver<R: TypeResolver>(
    db: &dyn QueryDatabase,
    resolver: &R,
    type_id: TypeId,
) -> TypeId {
    let mut evaluator =
        tsz_solver::computation::TypeEvaluator::with_resolver(db.as_type_database(), resolver)
            .with_query_db(db);
    evaluator.evaluate(type_id)
}

pub(crate) fn generator_context_application(
    db: &dyn TypeDatabase,
    generator_def: DefId,
    yield_type: TypeId,
    result_type: TypeId,
    next_type: TypeId,
) -> TypeId {
    let generator_base = db.lazy(generator_def);
    db.application(generator_base, vec![yield_type, result_type, next_type])
}

pub(crate) fn yield_star_array_context(db: &dyn TypeDatabase, yield_type: TypeId) -> TypeId {
    db.array(yield_type)
}
