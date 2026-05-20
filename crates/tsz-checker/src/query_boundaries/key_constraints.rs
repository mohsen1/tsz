use tsz_solver::{TypeDatabase, TypeId};

pub(crate) fn is_symbol_only_key_constraint(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id == TypeId::SYMBOL || tsz_solver::type_queries::is_unique_symbol_type(db, type_id) {
        return true;
    }

    tsz_solver::type_queries::get_union_members(db, type_id).is_some_and(|members| {
        !members.is_empty()
            && members
                .iter()
                .all(|&member| is_symbol_only_key_constraint(db, member))
    })
}
