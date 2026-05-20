use tsz_solver::TypeId;
use tsz_solver::construction::TypeDatabase;

pub(crate) fn is_top_level_error_or_error_union_member(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> bool {
    tsz_solver::is_error_type(db, type_id)
        || tsz_solver::type_queries::get_union_members(db, type_id).is_some_and(|members| {
            members
                .iter()
                .any(|&member| tsz_solver::is_error_type(db, member))
        })
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use tsz_solver::TypeInterner;

    #[test]
    fn top_level_error_or_error_union_member_detects_error_shapes() {
        let db = TypeInterner::new();
        let error_union = db.union(vec![TypeId::STRING, TypeId::ERROR]);
        let non_error_union = db.union(vec![TypeId::STRING, TypeId::NUMBER]);

        assert!(is_top_level_error_or_error_union_member(&db, TypeId::ERROR));
        assert!(is_top_level_error_or_error_union_member(&db, error_union));
        assert!(!is_top_level_error_or_error_union_member(
            &db,
            TypeId::STRING
        ));
        assert!(!is_top_level_error_or_error_union_member(
            &db,
            non_error_union
        ));
    }
}
