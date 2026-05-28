use tsz_solver::TypeId;
use tsz_solver::construction::TypeDatabase;

pub(crate) use super::common::is_compiler_managed_type;

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

/// Primitive index-key types: the only types tsc treats as valid bare keys for
/// `keyof`/index-signature constraint membership.
const PRIMITIVE_INDEX_KEYS: [TypeId; 3] = [TypeId::STRING, TypeId::NUMBER, TypeId::SYMBOL];

/// Which of `string` / `number` / `symbol` a base index-key type admits.
///
/// A primitive key is admitted when one of the candidate `forms` is that key
/// directly or has it as a union member. Callers pass the raw base and, where a
/// `keyof`/indexed-access base only decomposes into a `Union` after evaluation,
/// the evaluated base as well, so the structural decision covers both shapes.
///
/// This is the structural replacement for matching the rendered `"string |
/// number"` / `"string | number | symbol"` text: that match is per-base rather
/// than per-key, so it would falsely admit `symbol` when the base is only
/// `string | number`. Operating on `TypeId` structure keeps the decision
/// per-key and independent of how a type is spelled or aliased.
pub(crate) fn present_primitive_index_keys(db: &dyn TypeDatabase, forms: &[TypeId]) -> Vec<TypeId> {
    let form_members: Vec<Option<Vec<TypeId>>> = forms
        .iter()
        .map(|&form| tsz_solver::type_queries::get_union_members(db, form))
        .collect();

    PRIMITIVE_INDEX_KEYS
        .into_iter()
        .filter(|primitive_key| {
            forms.contains(primitive_key)
                || form_members.iter().any(|members| {
                    members
                        .as_ref()
                        .is_some_and(|members| members.contains(primitive_key))
                })
        })
        .collect()
}

/// True when any of `string` / `number` / `symbol` is admitted by the base
/// index-key `forms`. See [`present_primitive_index_keys`].
pub(crate) fn base_admits_any_primitive_index_key(db: &dyn TypeDatabase, forms: &[TypeId]) -> bool {
    !present_primitive_index_keys(db, forms).is_empty()
}

pub(crate) fn is_this_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_this_type(db, type_id)
}

/// Whether `type_id` is a `unique symbol` type (`TypeData::UniqueSymbol`).
pub(crate) fn is_unique_symbol_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::type_queries::is_unique_symbol_type(db, type_id)
}

pub(crate) fn is_recursive_type_reference(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::recursive_index(db, type_id).is_some()
}

pub(crate) fn contains_recursive_operation_application(
    db: &dyn TypeDatabase,
    def_store: &tsz_solver::def::DefinitionStore,
    type_id: TypeId,
) -> bool {
    tsz_solver::type_queries::contains_recursive_operation_application_db(db, def_store, type_id)
}

pub(crate) fn is_recursive_operation_application(
    db: &dyn TypeDatabase,
    def_store: &tsz_solver::def::DefinitionStore,
    type_id: TypeId,
) -> bool {
    tsz_solver::type_queries::is_recursive_operation_application_db(db, def_store, type_id)
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

    #[test]
    fn present_primitive_index_keys_detects_direct_primitive_keys() {
        let db = TypeInterner::new();
        assert_eq!(
            present_primitive_index_keys(&db, &[TypeId::STRING]),
            vec![TypeId::STRING]
        );
        assert_eq!(
            present_primitive_index_keys(&db, &[TypeId::NUMBER]),
            vec![TypeId::NUMBER]
        );
        assert_eq!(
            present_primitive_index_keys(&db, &[TypeId::SYMBOL]),
            vec![TypeId::SYMBOL]
        );
    }

    #[test]
    fn present_primitive_index_keys_is_per_key_not_per_base() {
        // The core regression guard: `string | number` admits string and number
        // but must NOT admit symbol. A rendered-string match on the *base*
        // ("string | number") would have falsely admitted symbol.
        let db = TypeInterner::new();
        let string_number = db.union(vec![TypeId::STRING, TypeId::NUMBER]);
        let present = present_primitive_index_keys(&db, &[string_number]);
        assert!(present.contains(&TypeId::STRING));
        assert!(present.contains(&TypeId::NUMBER));
        assert!(!present.contains(&TypeId::SYMBOL));
    }

    #[test]
    fn present_primitive_index_keys_is_independent_of_union_spelling() {
        // Order/spelling of the union members must not change the structural
        // answer (no rendered-text dependence).
        let db = TypeInterner::new();
        let forward = db.union(vec![TypeId::STRING, TypeId::NUMBER, TypeId::SYMBOL]);
        let reversed = db.union(vec![TypeId::SYMBOL, TypeId::NUMBER, TypeId::STRING]);
        let mut a = present_primitive_index_keys(&db, &[forward]);
        let mut b = present_primitive_index_keys(&db, &[reversed]);
        a.sort_by_key(|t| t.0);
        b.sort_by_key(|t| t.0);
        assert_eq!(a, b);
        assert_eq!(a.len(), 3);
    }

    #[test]
    fn present_primitive_index_keys_ignores_non_primitive_members() {
        let db = TypeInterner::new();
        let mixed = db.union(vec![TypeId::STRING, TypeId::BOOLEAN, TypeId::OBJECT]);
        assert_eq!(
            present_primitive_index_keys(&db, &[mixed]),
            vec![TypeId::STRING]
        );

        let non_primitive = db.union(vec![TypeId::BOOLEAN, TypeId::OBJECT]);
        assert!(present_primitive_index_keys(&db, &[non_primitive]).is_empty());
        assert!(present_primitive_index_keys(&db, &[TypeId::BOOLEAN]).is_empty());
    }

    #[test]
    fn present_primitive_index_keys_recovers_key_from_any_form() {
        // A primitive present only in a later (e.g. evaluated) form is still
        // recognized — this is the raw + evaluated base coverage callers rely on.
        let db = TypeInterner::new();
        let evaluated = db.union(vec![TypeId::NUMBER, TypeId::SYMBOL]);
        let present = present_primitive_index_keys(&db, &[TypeId::BOOLEAN, evaluated]);
        assert!(present.contains(&TypeId::NUMBER));
        assert!(present.contains(&TypeId::SYMBOL));
        assert!(!present.contains(&TypeId::STRING));
    }

    #[test]
    fn base_admits_any_primitive_index_key_matches_presence() {
        let db = TypeInterner::new();
        let string_number = db.union(vec![TypeId::STRING, TypeId::NUMBER]);
        let non_primitive = db.union(vec![TypeId::BOOLEAN, TypeId::OBJECT]);

        assert!(base_admits_any_primitive_index_key(&db, &[string_number]));
        assert!(base_admits_any_primitive_index_key(&db, &[TypeId::SYMBOL]));
        assert!(!base_admits_any_primitive_index_key(&db, &[non_primitive]));
        assert!(!base_admits_any_primitive_index_key(
            &db,
            &[TypeId::BOOLEAN]
        ));
    }
}
