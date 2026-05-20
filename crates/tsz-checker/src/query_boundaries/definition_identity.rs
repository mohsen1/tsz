//! Boundary aliases for stable solver definition identity.

use tsz_solver::{TypeDatabase, TypeId};

pub(crate) type DefId = tsz_solver::def::DefId;

pub(crate) fn is_lazy_def_identity(db: &dyn TypeDatabase, type_id: TypeId, def_id: DefId) -> bool {
    tsz_solver::type_queries::get_lazy_def_id(db, type_id) == Some(def_id)
}
