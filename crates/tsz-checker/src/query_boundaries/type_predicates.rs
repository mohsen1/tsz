use tsz_solver::{TypeDatabase, TypeId};

pub(crate) fn contains_conditional_with_application_extends(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> bool {
    tsz_solver::type_queries::contains_conditional_with_application_extends(db, type_id)
}

pub(crate) fn type_predicate_type_assignable_to_parameter_with<F>(
    db: &dyn TypeDatabase,
    predicate_type: TypeId,
    param_type: TypeId,
    mut is_assignable: F,
) -> bool
where
    F: FnMut(TypeId, TypeId) -> bool,
{
    if predicate_type == param_type || is_assignable(predicate_type, param_type) {
        return true;
    }

    intersection_member_assignable_to_parameter(
        db,
        predicate_type,
        param_type,
        &mut is_assignable,
        &mut Vec::new(),
    )
}

fn intersection_member_assignable_to_parameter<F>(
    db: &dyn TypeDatabase,
    predicate_type: TypeId,
    param_type: TypeId,
    is_assignable: &mut F,
    seen: &mut Vec<TypeId>,
) -> bool
where
    F: FnMut(TypeId, TypeId) -> bool,
{
    if seen.contains(&predicate_type) {
        return false;
    }
    seen.push(predicate_type);

    let Some(members) = crate::query_boundaries::common::intersection_members(db, predicate_type)
    else {
        return false;
    };

    for member in members {
        if member == param_type
            || is_assignable(member, param_type)
            || intersection_member_assignable_to_parameter(
                db,
                member,
                param_type,
                is_assignable,
                seen,
            )
        {
            return true;
        }
    }

    false
}
