use tsz_solver::TypeId;

pub(crate) fn union_members(
    db: &dyn tsz_solver::TypeDatabase,
    type_id: TypeId,
) -> Option<Vec<TypeId>> {
    tsz_solver::type_queries::get_union_members(db, type_id)
}

pub(crate) fn is_type_parameter_type(db: &dyn tsz_solver::TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_type_parameter(db, type_id)
}
