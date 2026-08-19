use tsz_solver::TypeId;
use tsz_solver::construction::{QueryDatabase, TypeDatabase};
use tsz_solver::def::resolver::TypeResolver;

/// `T[K]` where `K extends keyof S` is a valid index (no `TS2536`) when `keyof S`
/// is a *deferred generic mapped index*. Gates the object and index to be
/// type-parameter-like and peels `keyof S` → `S`, then delegates the structural
/// decision to the solver
/// (`mapped_index_source_is_deferred_generic_keyof`, which owns the tsc rule).
pub(crate) fn indexed_access_is_deferred_generic_mapped_index<R: TypeResolver>(
    db: &dyn QueryDatabase,
    resolver: &R,
    object_type: TypeId,
    index_type: TypeId,
    index_constraint: Option<TypeId>,
) -> bool {
    let type_db = db.as_type_database();
    if !super::common::is_type_parameter_like(type_db, object_type)
        || !super::common::is_type_parameter_like(type_db, index_type)
    {
        return false;
    }
    let Some(constraint) = index_constraint else {
        return false;
    };
    let Some(inner) = super::common::keyof_inner_type(type_db, constraint) else {
        return false;
    };
    tsz_solver::type_queries::mapped_index_source_is_deferred_generic_keyof(db, resolver, inner)
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

pub(crate) fn is_generic_index_type(db: &dyn TypeDatabase, index_type: TypeId) -> bool {
    super::common::is_type_parameter(db, index_type)
        || super::common::keyof_inner_type(db, index_type).is_some()
        || super::common::is_index_access_type(db, index_type)
        || super::common::is_conditional_type(db, index_type)
        || super::common::is_generic_application(db, index_type)
        || super::common::union_members(db, index_type).is_some_and(|members| {
            members
                .iter()
                .any(|&member| is_generic_index_type(db, member))
        })
        || intersection_has_generic_index(db, index_type)
}

pub(crate) fn intersection_has_generic_index(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    super::common::intersection_members(db, type_id).is_some_and(|members| {
        members
            .iter()
            .any(|&member| is_generic_index_type(db, member))
    })
}

pub(crate) fn index_resolves_to_keyof_of_receiver<F>(
    db: &dyn TypeDatabase,
    index_type: TypeId,
    evaluated_receiver: TypeId,
    evaluate: &mut F,
) -> bool
where
    F: FnMut(TypeId) -> TypeId,
{
    if let Some(members) = super::common::intersection_members(db, index_type) {
        return members.iter().copied().any(|member| {
            index_resolves_to_keyof_of_receiver(db, member, evaluated_receiver, evaluate)
        });
    }
    if let Some(inner) = super::common::keyof_inner_type(db, index_type) {
        return evaluate(inner) == evaluated_receiver;
    }
    if let Some(param_info) = super::common::type_param_info(db, index_type)
        && let Some(constraint) = param_info.constraint
        && let Some(inner) = super::common::keyof_inner_type(db, constraint)
    {
        return evaluate(inner) == evaluated_receiver;
    }
    false
}

pub(crate) fn is_valid_index_for_type_param<F>(
    db: &dyn TypeDatabase,
    index_type: TypeId,
    type_param: TypeId,
    evaluate: &mut F,
) -> bool
where
    F: FnMut(TypeId) -> TypeId,
{
    if let Some(members) = super::common::intersection_members(db, index_type) {
        return members
            .iter()
            .copied()
            .any(|member| is_valid_index_for_type_param(db, member, type_param, evaluate));
    }
    if super::common::is_generic_application(db, index_type) {
        let evaluated = evaluate(index_type);
        if evaluated != index_type && evaluated != TypeId::ERROR {
            return is_valid_index_for_type_param(db, evaluated, type_param, evaluate);
        }
    }
    if let Some(keyof_inner) = super::common::keyof_inner_type(db, index_type) {
        return same_type_param_identity(db, keyof_inner, type_param)
            || super::common::type_param_info(db, type_param)
                .and_then(|param| param.constraint)
                .is_some_and(|constraint| same_type_param_identity(db, constraint, keyof_inner));
    }
    if let Some(param_info) = super::common::type_param_info(db, index_type)
        && let Some(constraint) = param_info.constraint
    {
        if let Some(keyof_inner) = super::common::keyof_inner_type(db, constraint) {
            return same_type_param_identity(db, keyof_inner, type_param)
                || super::common::type_param_info(db, type_param)
                    .and_then(|param| param.constraint)
                    .is_some_and(|constraint| {
                        same_type_param_identity(db, constraint, keyof_inner)
                    });
        }
        // `K extends keyof T`'s constraint may already have been reduced to its
        // evaluated key union at K's declaration site when T's own constraint
        // was concrete (`get_keyof_type` eagerly evaluates `keyof` there), so
        // the syntactic `KeyOf(T)` shape checked above no longer exists on
        // `constraint`. Recognize that reduced form structurally instead: it is
        // exactly what evaluating `keyof T` produces right now.
        let deferred_keyof = db.keyof(type_param);
        let evaluated_keyof = evaluate(deferred_keyof);
        if evaluated_keyof != deferred_keyof && evaluate(constraint) == evaluated_keyof {
            return true;
        }
    }
    false
}

pub(crate) fn same_type_param_identity(db: &dyn TypeDatabase, left: TypeId, right: TypeId) -> bool {
    left == right
        || super::common::type_param_info(db, left)
            .zip(super::common::type_param_info(db, right))
            .is_some_and(|(l, r)| l.name == r.name)
}

pub(crate) fn type_contains_same_type_param_identity<F>(
    db: &dyn TypeDatabase,
    ty: TypeId,
    type_param: TypeId,
    evaluate: &mut F,
) -> bool
where
    F: FnMut(TypeId) -> TypeId,
{
    if same_type_param_identity(db, ty, type_param) {
        return true;
    }
    if let Some(inner) = super::common::keyof_inner_type(db, ty)
        && type_contains_same_type_param_identity(db, inner, type_param, evaluate)
    {
        return true;
    }
    if let Some((object_type, index_type)) = super::common::index_access_types(db, ty)
        && (type_contains_same_type_param_identity(db, object_type, type_param, evaluate)
            || type_contains_same_type_param_identity(db, index_type, type_param, evaluate))
    {
        return true;
    }
    if let Some(members) = super::common::union_members(db, ty)
        && members
            .iter()
            .any(|&member| type_contains_same_type_param_identity(db, member, type_param, evaluate))
    {
        return true;
    }
    if let Some(members) = super::common::intersection_members(db, ty)
        && members
            .iter()
            .any(|&member| type_contains_same_type_param_identity(db, member, type_param, evaluate))
    {
        return true;
    }
    if let Some(param_info) = super::common::type_param_info(db, ty)
        && let Some(constraint) = param_info.constraint
        && type_contains_same_type_param_identity(db, constraint, type_param, evaluate)
    {
        return true;
    }
    if super::common::is_generic_application(db, ty) {
        let evaluated = evaluate(ty);
        if evaluated != ty
            && evaluated != TypeId::ERROR
            && type_contains_same_type_param_identity(db, evaluated, type_param, evaluate)
        {
            return true;
        }
    }
    false
}

pub(crate) fn generic_index_mentions_transformed_current_type_param<F>(
    db: &dyn TypeDatabase,
    index_type: TypeId,
    type_param: TypeId,
    evaluate: &mut F,
) -> bool
where
    F: FnMut(TypeId) -> TypeId,
{
    if let Some(keyof_inner) = super::common::keyof_inner_type(db, index_type) {
        return !same_type_param_identity(db, keyof_inner, type_param)
            && type_contains_same_type_param_identity(db, keyof_inner, type_param, evaluate);
    }
    if let Some(param_info) = super::common::type_param_info(db, index_type)
        && let Some(constraint) = param_info.constraint
    {
        return generic_index_mentions_transformed_current_type_param(
            db, constraint, type_param, evaluate,
        );
    }
    if let Some(members) = super::common::union_members(db, index_type) {
        return members.iter().any(|&member| {
            generic_index_mentions_transformed_current_type_param(db, member, type_param, evaluate)
        });
    }
    if let Some(members) = super::common::intersection_members(db, index_type) {
        return members.iter().any(|&member| {
            generic_index_mentions_transformed_current_type_param(db, member, type_param, evaluate)
        });
    }
    if super::common::is_generic_application(db, index_type) {
        let evaluated = evaluate(index_type);
        if evaluated != index_type && evaluated != TypeId::ERROR {
            return generic_index_mentions_transformed_current_type_param(
                db, evaluated, type_param, evaluate,
            );
        }
    }
    false
}

pub(crate) fn keyof_source_type_param(
    db: &dyn TypeDatabase,
    index_type: TypeId,
    type_param: TypeId,
) -> Option<TypeId> {
    if let Some(keyof_inner) = super::common::keyof_inner_type(db, index_type)
        && super::common::is_type_parameter(db, keyof_inner)
        && keyof_inner != type_param
    {
        return Some(keyof_inner);
    }
    if let Some(param_info) = super::common::type_param_info(db, index_type)
        && let Some(constraint) = param_info.constraint
        && let Some(keyof_inner) = super::common::keyof_inner_type(db, constraint)
        && super::common::is_type_parameter(db, keyof_inner)
        && keyof_inner != type_param
    {
        return Some(keyof_inner);
    }
    None
}

pub(crate) fn is_generic_key_space(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if super::common::keyof_inner_type(db, type_id).is_some()
        || super::common::is_type_parameter(db, type_id)
    {
        return true;
    }
    if let Some(members) = super::common::union_members(db, type_id) {
        return members
            .iter()
            .all(|&member| is_generic_key_space(db, member));
    }
    if let Some(members) = super::common::intersection_members(db, type_id) {
        return members
            .iter()
            .all(|&member| is_generic_key_space(db, member));
    }
    false
}
