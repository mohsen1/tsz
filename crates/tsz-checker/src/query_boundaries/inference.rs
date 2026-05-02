//! Query-boundary wrappers for generic inference helpers.

use crate::query_boundaries::common::QueryDatabase;
use tsz_common::interner::Atom;
use tsz_solver::{TypeId, TypeSubstitution};

pub(crate) fn instantiate_type_with_infer(
    db: &dyn QueryDatabase,
    type_id: TypeId,
    substitution: &TypeSubstitution,
) -> TypeId {
    tsz_solver::instantiate_type_with_infer_cached(
        db.as_type_database(),
        Some(db),
        type_id,
        substitution,
    )
}

pub(crate) fn collect_infer_bindings(
    db: &dyn tsz_solver::TypeDatabase,
    type_id: TypeId,
) -> Vec<(Atom, TypeId)> {
    tsz_solver::collect_infer_bindings(db, type_id)
}
