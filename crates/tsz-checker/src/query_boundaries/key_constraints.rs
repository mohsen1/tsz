use tsz_solver::{TypeDatabase, TypeId};

/// Returns `true` when `type_id` is a conditional type whose condition is concrete.
/// Such a conditional can be evaluated to one of its branches deterministically.
pub(crate) fn conditional_type_has_concrete_condition(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> bool {
    tsz_solver::type_queries::get_conditional_type(db, type_id).is_some_and(|cond| {
        !tsz_solver::type_queries::contains_type_parameters_db(db, cond.check_type)
            && !tsz_solver::type_queries::contains_type_parameters_db(db, cond.extends_type)
    })
}

/// Returns `true` iff `type_id` has free type parameters preventing its key
/// space from being concretely determined.
///
/// For a mapped type `{ [K in Constraint]: Template }`, only `Constraint`
/// governs the key space — `K`/`P` in the template is a **bound** variable
/// and must not be treated as a free inference variable. For all other types
/// this falls back to `contains_type_parameters`.
pub(crate) fn type_has_free_type_parameters_for_key_space(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> bool {
    if let Some(mapped) = tsz_solver::type_queries::get_mapped_type(db, type_id) {
        return tsz_solver::type_queries::contains_type_parameters_db(db, mapped.constraint);
    }
    tsz_solver::type_queries::contains_type_parameters_db(db, type_id)
}

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
