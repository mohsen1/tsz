use tsz_solver::{TupleElement, TypeDatabase, TypeId};

pub(crate) fn union_members_for_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<Vec<TypeId>> {
    tsz_solver::type_queries::get_union_members(db, type_id)
}

pub(crate) fn tuple_elements_for_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<Vec<TupleElement>> {
    tsz_solver::type_queries::get_tuple_elements(db, type_id)
}

pub(crate) fn are_types_mutually_subtype_with_env(
    db: &dyn TypeDatabase,
    env: &tsz_solver::TypeEnvironment,
    left: TypeId,
    right: TypeId,
    strict_null_checks: bool,
) -> bool {
    types_are_subtype_with_env(db, env, left, right, strict_null_checks)
        || types_are_subtype_with_env(db, env, right, left, strict_null_checks)
}

fn types_are_subtype_with_env(
    db: &dyn TypeDatabase,
    env: &tsz_solver::TypeEnvironment,
    source: TypeId,
    target: TypeId,
    strict_null_checks: bool,
) -> bool {
    let mut flags = 0u16;
    if strict_null_checks {
        flags |= tsz_solver::RelationCacheKey::FLAG_STRICT_NULL_CHECKS;
    }

    tsz_solver::query_relation_with_resolver(
        db,
        env,
        source,
        target,
        tsz_solver::RelationKind::Subtype,
        tsz_solver::RelationPolicy::from_flags(flags),
        tsz_solver::RelationContext::default(),
    )
    .is_related()
}
