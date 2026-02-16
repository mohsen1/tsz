use tsz_solver::TypeId;

pub(crate) use tsz_solver::type_queries_extended::ConstructorCheckKind;

pub(crate) fn union_members(
    db: &dyn tsz_solver::TypeDatabase,
    type_id: TypeId,
) -> Option<Vec<TypeId>> {
    tsz_solver::type_queries::get_union_members(db, type_id)
}

pub(crate) fn has_construct_signatures(db: &dyn tsz_solver::TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::has_construct_signatures(db, type_id)
}

pub(crate) fn classify_for_constructor_check(
    db: &dyn tsz_solver::TypeDatabase,
    type_id: TypeId,
) -> ConstructorCheckKind {
    tsz_solver::type_queries_extended::classify_for_constructor_check(db, type_id)
}

pub(crate) fn callable_shape_for_type(
    db: &dyn tsz_solver::TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<tsz_solver::CallableShape>> {
    tsz_solver::type_queries::get_callable_shape(db, type_id)
}

pub(crate) fn has_function_shape(db: &dyn tsz_solver::TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::get_function_shape(db, type_id).is_some()
}
