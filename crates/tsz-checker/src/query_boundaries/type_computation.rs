use tsz_solver::TypeId;

pub(crate) fn union_members(
    db: &dyn tsz_solver::TypeDatabase,
    type_id: TypeId,
) -> Option<Vec<TypeId>> {
    tsz_solver::type_queries::get_union_members(db, type_id)
}
