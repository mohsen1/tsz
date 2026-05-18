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
    type_has_free_type_parameters_for_key_space_inner(db, type_id, 0)
}

fn type_has_free_type_parameters_for_key_space_inner(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    depth: usize,
) -> bool {
    if depth > 32 {
        return tsz_solver::type_queries::contains_type_parameters_db(db, type_id);
    }

    if let Some(mapped) = tsz_solver::type_queries::get_mapped_type(db, type_id) {
        return type_has_free_type_parameters_for_key_space_inner(db, mapped.constraint, depth + 1)
            || mapped.name_type.is_some_and(|name_type| {
                type_has_free_type_parameters_for_key_space_inner(db, name_type, depth + 1)
            });
    }

    if let Some(members) = tsz_solver::type_queries::get_intersection_members(db, type_id) {
        return members.iter().any(|&member| {
            type_has_free_type_parameters_for_key_space_inner(db, member, depth + 1)
        });
    }

    if let Some(members) = tsz_solver::type_queries::get_union_members(db, type_id) {
        return members.iter().any(|&member| {
            type_has_free_type_parameters_for_key_space_inner(db, member, depth + 1)
        });
    }

    if let Some(app) = tsz_solver::type_queries::get_type_application(db, type_id) {
        return type_has_free_type_parameters_for_key_space_inner(db, app.base, depth + 1)
            || app
                .args
                .iter()
                .any(|&arg| type_has_free_type_parameters_for_key_space_inner(db, arg, depth + 1));
    }

    if let Some((object_type, index_type)) =
        tsz_solver::type_queries::get_index_access_types(db, type_id)
    {
        return type_has_free_type_parameters_for_key_space_inner(db, object_type, depth + 1)
            || type_has_free_type_parameters_for_key_space_inner(db, index_type, depth + 1);
    }

    if let Some(cond) = tsz_solver::type_queries::get_conditional_type(db, type_id) {
        return type_has_free_type_parameters_for_key_space_inner(db, cond.check_type, depth + 1)
            || type_has_free_type_parameters_for_key_space_inner(db, cond.extends_type, depth + 1)
            || type_has_free_type_parameters_for_key_space_inner(db, cond.true_type, depth + 1)
            || type_has_free_type_parameters_for_key_space_inner(db, cond.false_type, depth + 1);
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
