//! Exact-identity construction for deferred conditional constraints.

use crate::construction::QueryDatabase;
use crate::types::{TypeData, TypeId};

/// Query-backed variant of
/// [`super::conditional_check_type_substituted_constraint`] that replaces only
/// the exact check-parameter identity.
///
/// The full exact rewriter traverses mapped and other deferred nodes and keeps
/// same-named foreign binders unchanged. Callers without a query database use
/// the legacy `TypeDatabase`-only helper instead.
pub fn conditional_check_type_substituted_constraint_exact(
    db: &dyn QueryDatabase,
    type_id: TypeId,
) -> Option<TypeId> {
    let type_db = db.as_type_database();
    let cond_id = crate::type_queries::get_conditional_type_id(type_db, type_id)?;
    let cond = type_db.conditional_type(cond_id);
    let constraint = crate::type_queries::get_base_constraint_of_type(type_db, cond.check_type);
    conditional_check_type_substituted_with_constraint_exact(db, type_id, constraint)
}

/// Query-backed exact-identity substitution using an explicitly exposed
/// constraint.
///
/// This composes resolver-owned constraint exposure with the exact rewriter:
/// only the conditional's check-parameter `TypeId` is replaced, while sibling
/// binders with the same spelling remain unchanged.
pub fn conditional_check_type_substituted_with_constraint_exact(
    db: &dyn QueryDatabase,
    type_id: TypeId,
    constraint: TypeId,
) -> Option<TypeId> {
    let type_db = db.as_type_database();
    let cond_id = crate::type_queries::get_conditional_type_id(type_db, type_id)?;
    let cond = type_db.conditional_type(cond_id);
    if !matches!(
        type_db.lookup(cond.check_type),
        Some(TypeData::TypeParameter(_))
    ) {
        return None;
    }

    if constraint == cond.check_type || constraint == TypeId::ERROR {
        return None;
    }
    let substituted = crate::instantiation::instantiate::substitute_exact_type(
        db,
        type_id,
        cond.check_type,
        constraint,
    );
    (substituted != type_id).then_some(substituted)
}
