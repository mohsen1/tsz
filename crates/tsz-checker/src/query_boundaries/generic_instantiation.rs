use tsz_solver::construction::QueryDatabase;
use tsz_solver::{TypeId, TypeParamInfo, computation as c};

pub(crate) fn instantiate_type(
    db: &dyn QueryDatabase,
    type_id: TypeId,
    substitution: &c::TypeSubstitution,
) -> TypeId {
    c::instantiate_type_cached(db.as_type_database(), Some(db), type_id, substitution)
}

pub(crate) fn instantiate_generic(
    db: &dyn QueryDatabase,
    type_id: TypeId,
    type_params: &[TypeParamInfo],
    type_args: &[TypeId],
) -> TypeId {
    c::instantiate_generic_cached(
        db.as_type_database(),
        Some(db),
        type_id,
        type_params,
        type_args,
    )
}
