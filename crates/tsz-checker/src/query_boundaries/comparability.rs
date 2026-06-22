//! Checker-facing comparability/overlap query boundaries.

use super::common::TypeDatabase;
use tsz_solver::TypeId;

/// Reduce an instantiable indexed access `Obj[Idx]` to its base constraint for
/// the comparability/overlap relation only (TS2678/TS2367). Non-`IndexAccess`
/// inputs and irreducible accesses are returned unchanged. This is kept separate
/// from the shared base-constraint query so the reduction does not leak onto
/// assignment narrowing or constraint validation.
pub(crate) fn reduce_index_access_to_base_constraint(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> TypeId {
    tsz_solver::type_queries::reduce_index_access_to_base_constraint(db, type_id)
}
