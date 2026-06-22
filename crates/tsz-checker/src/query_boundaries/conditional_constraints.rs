use tsz_solver::TypeId;
use tsz_solver::construction::TypeDatabase;

/// Base constraint of a deferred conditional, computed as the union of its two
/// branch results (tsc's `getBaseConstraintOfType` of a conditional). Used to
/// validate an index-access key / assertion source against a deferred
/// conditional without forcing the conditional itself.
pub(crate) fn conditional_branch_union_constraint(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<TypeId> {
    tsz_solver::type_queries::conditional_branch_union_constraint(db, type_id)
}

/// Apparent base constraint of a deferred conditional type (tsc's
/// `getDefaultConstraintOfConditionalType`): the union of its inferred
/// true-branch and false-branch result types. `None` when `type_id` is not a
/// deferred conditional. Used to validate an indexed-access key / assertion
/// source against the conditional's key space (tsc resolves the object/source
/// through `getApparentType`, which uses this constraint).
pub(crate) fn conditional_default_constraint(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<TypeId> {
    tsz_solver::type_queries::get_conditional_default_constraint(db, type_id)
}
