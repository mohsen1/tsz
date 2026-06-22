use tsz_solver::{TypeId, construction::TypeDatabase};

/// Construction half of tsc's `getConstraintFromConditionalType`: substitute a
/// deferred conditional's type-parameter check type with its own base constraint
/// and return the substituted (still unevaluated) conditional.
pub(crate) fn check_type_substituted_constraint(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<TypeId> {
    tsz_solver::type_queries::conditional_check_type_substituted_constraint(db, type_id)
}
