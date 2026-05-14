use tsz_solver::{TypeDatabase, TypeId};

pub(crate) fn originates_from_remapped_mapped_type_with_evaluator(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    evaluate: &mut dyn FnMut(TypeId) -> TypeId,
) -> bool {
    let mut visited = rustc_hash::FxHashSet::default();
    originates_from_remapped_mapped_type_inner(db, type_id, &mut visited, evaluate)
}

fn originates_from_remapped_mapped_type_inner(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    visited: &mut rustc_hash::FxHashSet<TypeId>,
    evaluate: &mut dyn FnMut(TypeId) -> TypeId,
) -> bool {
    if !visited.insert(type_id) {
        return false;
    }

    if let Some(alias) = db.get_display_alias(type_id)
        && alias != type_id
        && originates_from_remapped_mapped_type_inner(db, alias, visited, evaluate)
    {
        return true;
    }

    if let Some(mapped) = tsz_solver::type_queries::get_mapped_type(db, type_id) {
        return mapped.name_type.is_some();
    }

    if let Some(app) = tsz_solver::type_queries::get_type_application(db, type_id)
        && (originates_from_remapped_mapped_type_inner(db, app.base, visited, evaluate)
            || app
                .args
                .iter()
                .copied()
                .any(|arg| originates_from_remapped_mapped_type_inner(db, arg, visited, evaluate)))
    {
        return true;
    }

    if let Some((object, index)) = tsz_solver::type_queries::get_index_access_types(db, type_id)
        && (originates_from_remapped_mapped_type_inner(db, object, visited, evaluate)
            || originates_from_remapped_mapped_type_inner(db, index, visited, evaluate))
    {
        return true;
    }

    if let Some(members) = tsz_solver::type_queries::get_union_members(db, type_id)
        && members
            .iter()
            .copied()
            .any(|member| originates_from_remapped_mapped_type_inner(db, member, visited, evaluate))
    {
        return true;
    }

    if let Some(members) = tsz_solver::type_queries::get_intersection_members(db, type_id)
        && members
            .iter()
            .copied()
            .any(|member| originates_from_remapped_mapped_type_inner(db, member, visited, evaluate))
    {
        return true;
    }

    let evaluated = evaluate(type_id);
    evaluated != type_id
        && originates_from_remapped_mapped_type_inner(db, evaluated, visited, evaluate)
}
