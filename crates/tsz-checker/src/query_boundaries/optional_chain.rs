//! Query-boundary helpers for optional-chain continuation result shaping.

use crate::query_boundaries::common::TypeDatabase;
use tsz_solver::TypeId;

pub(crate) fn add_undefined_if_missing(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    if crate::query_boundaries::common::type_contains_undefined(db, type_id) {
        type_id
    } else {
        crate::query_boundaries::common::union_with_undefined(db, type_id)
    }
}
