use tsz_solver::{TypeDatabase, TypeId};

pub(crate) use super::super::common::{callable_shape_for_type, contains_type_parameters};

/// Check if a type is a bare type parameter (`TypeParameter` or `Infer`).
pub(crate) fn is_bare_type_parameter(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    tsz_solver::visitor::is_type_parameter(db, type_id)
}

/// Get the base constraint of a type for TS2344 checking.
///
/// For `TypeParameter` with constraint: returns the constraint.
/// For `TypeParameter` without constraint: returns `UNKNOWN`.
/// For all other types (including `Infer`): returns the type unchanged.
pub(crate) fn base_constraint_of_type(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    tsz_solver::type_queries::get_base_constraint_of_type(db, type_id)
}

/// Get the object and index types of an `IndexAccess` type.
pub(crate) fn index_access_components(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<(TypeId, TypeId)> {
    tsz_solver::type_queries::get_index_access_types(db, type_id)
}

/// Get the extends type and false type of a conditional type.
///
/// Returns `Some((extends_type, false_type))` if the type is a `Conditional`.
/// Used for TS2344 constraint checking: for `Extract<T, C>` (i.e., `T extends C ? T : never`),
/// the result is always a subtype of `C`, so if `C` satisfies the required constraint,
/// the TS2344 check should be skipped.
pub(crate) fn conditional_type_components(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<(TypeId, TypeId)> {
    let cond_id = tsz_solver::type_queries::get_conditional_type_id(db, type_id)?;
    let cond = db.get_conditional(cond_id);
    Some((cond.extends_type, cond.false_type))
}
