use tsz_solver::TypeId;
use tsz_solver::construction::TypeDatabase;

/// True when `type_id` is structurally a `keyof` type.
pub(crate) fn is_keyof_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    super::common::is_keyof_type(db, type_id)
}

/// True when `type_id` is structurally an intersection type.
pub(crate) fn is_intersection_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    super::common::is_intersection_type(db, type_id)
}

/// True when some `TypeApplication` reachable from `type_id` (or from its
/// preserved display alias) has `TypeId::UNKNOWN` as a direct type argument.
pub(crate) fn type_application_args_contain_unknown(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> bool {
    tsz_solver::type_queries::type_application_args_contain_unknown(db, type_id)
}

/// True when any sub-type of `type_id` is either `Array<never>` (rendered as
/// `never[]`) or an indexed access whose object type is `never`
/// (rendered as `never[…]`).
pub(crate) fn type_contains_never_array_or_index_into_never(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> bool {
    tsz_solver::type_queries::type_contains_never_array_or_index_into_never(db, type_id)
}

/// True when any sub-type of `type_id` is a `KeyOf(_)` type.
pub(crate) fn type_contains_keyof_anywhere(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::type_contains_keyof_anywhere(db, type_id)
}

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

pub(crate) fn is_this_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_this_type(db, type_id)
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
    use tsz_solver::TupleElement;
    use tsz_solver::construction::TypeInterner;

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

    // A minimal `App<args>` for the predicate tests — the walker only needs
    // the wrapping application, not a real def behind the lazy base.
    fn make_app_with_args(db: &TypeInterner, args: Vec<TypeId>) -> TypeId {
        let base = db.lazy(tsz_solver::DefId(1));
        db.application(base, args)
    }

    #[test]
    fn type_application_args_contain_unknown_direct() {
        let db = TypeInterner::new();
        let app = make_app_with_args(&db, vec![TypeId::UNKNOWN, TypeId::STRING]);
        assert!(type_application_args_contain_unknown(&db, app));
    }

    #[test]
    fn type_application_args_contain_unknown_nested_in_outer_application() {
        // `Outer<Inner<unknown>>` — the inner application has UNKNOWN as a
        // direct arg, so the outer application's rendered form will include
        // the `<unknown>` substring.
        let db = TypeInterner::new();
        let inner = make_app_with_args(&db, vec![TypeId::UNKNOWN]);
        let outer = make_app_with_args(&db, vec![inner]);
        assert!(type_application_args_contain_unknown(&db, outer));
    }

    #[test]
    fn type_application_args_contain_unknown_array_of_unknown_is_negative() {
        // `App<unknown[]>` renders as `App<unknown[]>` — the substring
        // `<unknown>` does NOT appear (`<unknown[` instead). Match that.
        let db = TypeInterner::new();
        let inner_array = db.array(TypeId::UNKNOWN);
        let app = make_app_with_args(&db, vec![inner_array]);
        assert!(!type_application_args_contain_unknown(&db, app));
    }

    #[test]
    fn type_application_args_contain_unknown_tuple_of_unknown_is_negative() {
        let db = TypeInterner::new();
        let tup = db.tuple(vec![TupleElement {
            type_id: TypeId::UNKNOWN,
            name: None,
            optional: false,
            rest: false,
        }]);
        let app = make_app_with_args(&db, vec![tup]);
        assert!(!type_application_args_contain_unknown(&db, app));
    }

    #[test]
    fn type_application_args_contain_unknown_negative() {
        let db = TypeInterner::new();
        let app = make_app_with_args(&db, vec![TypeId::STRING, TypeId::NUMBER]);
        assert!(!type_application_args_contain_unknown(&db, app));
        assert!(!type_application_args_contain_unknown(&db, TypeId::STRING));
        assert!(!type_application_args_contain_unknown(&db, TypeId::UNKNOWN));
    }

    #[test]
    fn type_application_args_contain_unknown_through_union_member_application() {
        let db = TypeInterner::new();
        let inner_app = make_app_with_args(&db, vec![TypeId::UNKNOWN]);
        let unioned = db.union(vec![TypeId::STRING, inner_app]);
        assert!(type_application_args_contain_unknown(&db, unioned));
    }

    #[test]
    fn type_application_args_contain_unknown_through_object_property() {
        // `{ p: App<unknown> }` renders with `<unknown>` inside, so the
        // walker must descend into property types.
        let db = TypeInterner::new();
        let inner_app = make_app_with_args(&db, vec![TypeId::UNKNOWN]);
        let obj = db.object(vec![tsz_solver::PropertyInfo {
            name: db.intern_string("p"),
            type_id: inner_app,
            write_type: inner_app,
            ..Default::default()
        }]);
        assert!(type_application_args_contain_unknown(&db, obj));
    }

    #[test]
    fn type_contains_never_array_or_index_into_never_direct_array() {
        let db = TypeInterner::new();
        let never_arr = db.array(TypeId::NEVER);
        assert!(type_contains_never_array_or_index_into_never(
            &db, never_arr
        ));
    }

    #[test]
    fn type_contains_never_array_or_index_into_never_index_into_never() {
        let db = TypeInterner::new();
        let access = db.index_access(TypeId::NEVER, TypeId::STRING);
        assert!(type_contains_never_array_or_index_into_never(&db, access));
    }

    #[test]
    fn type_contains_never_array_or_index_into_never_in_tuple() {
        let db = TypeInterner::new();
        let never_arr = db.array(TypeId::NEVER);
        let tup = db.tuple(vec![TupleElement {
            type_id: never_arr,
            name: None,
            optional: false,
            rest: false,
        }]);
        assert!(type_contains_never_array_or_index_into_never(&db, tup));
    }

    #[test]
    fn type_contains_never_array_or_index_into_never_in_union() {
        let db = TypeInterner::new();
        let never_arr = db.array(TypeId::NEVER);
        let unioned = db.union(vec![TypeId::STRING, never_arr]);
        assert!(type_contains_never_array_or_index_into_never(&db, unioned));
    }

    #[test]
    fn type_contains_never_array_or_index_into_never_in_object_property() {
        let db = TypeInterner::new();
        let never_arr = db.array(TypeId::NEVER);
        let obj = db.object(vec![tsz_solver::PropertyInfo {
            name: db.intern_string("p"),
            type_id: never_arr,
            write_type: never_arr,
            ..Default::default()
        }]);
        assert!(type_contains_never_array_or_index_into_never(&db, obj));
    }

    #[test]
    fn type_contains_never_array_or_index_into_never_in_object_index_signature() {
        let db = TypeInterner::new();
        let never_arr = db.array(TypeId::NEVER);
        let shape = tsz_solver::ObjectShape {
            string_index: Some(tsz_solver::IndexSignature {
                key_type: TypeId::STRING,
                value_type: never_arr,
                readonly: false,
                param_name: None,
            }),
            ..Default::default()
        };
        let obj = db.object_with_index(shape);
        assert!(type_contains_never_array_or_index_into_never(&db, obj));
    }

    #[test]
    fn type_contains_never_array_or_index_into_never_negative() {
        let db = TypeInterner::new();
        let plain_arr = db.array(TypeId::STRING);
        assert!(!type_contains_never_array_or_index_into_never(
            &db, plain_arr
        ));
        assert!(!type_contains_never_array_or_index_into_never(
            &db,
            TypeId::STRING
        ));
        assert!(!type_contains_never_array_or_index_into_never(
            &db,
            TypeId::NEVER
        ));
        let access_into_string = db.index_access(TypeId::STRING, TypeId::NUMBER);
        assert!(!type_contains_never_array_or_index_into_never(
            &db,
            access_into_string
        ));
    }

    #[test]
    fn type_contains_keyof_anywhere_direct() {
        let db = TypeInterner::new();
        let keyof_string = db.keyof(TypeId::STRING);
        assert!(type_contains_keyof_anywhere(&db, keyof_string));
    }

    #[test]
    fn type_contains_keyof_anywhere_nested_in_union() {
        let db = TypeInterner::new();
        let keyof_number = db.keyof(TypeId::NUMBER);
        let unioned = db.union(vec![TypeId::STRING, keyof_number]);
        assert!(type_contains_keyof_anywhere(&db, unioned));
    }

    #[test]
    fn type_contains_keyof_anywhere_negative() {
        let db = TypeInterner::new();
        assert!(!type_contains_keyof_anywhere(&db, TypeId::STRING));
        assert!(!type_contains_keyof_anywhere(
            &db,
            db.union(vec![TypeId::STRING, TypeId::NUMBER])
        ));
    }
}
