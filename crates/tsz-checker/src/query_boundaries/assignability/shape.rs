//! Indexed-access surface normalization shape probes.
//!
//! These helpers own the low-level type-shape questions asked while normalizing
//! indexed-access surfaces *before* an assignability relation runs (the TS2322 /
//! TS2345 pipeline). Routing them through this boundary — rather than the
//! catch-all `query_boundaries::common` module — keeps the relation-adjacent
//! normalization steps visibly owned by the assignability boundary, so
//! reviewers can tell which shape probes are part of the relation pipeline from
//! generic type queries. They delegate to the existing solver queries
//! internally; the point is ownership and ratcheting, not new semantics.

use tsz_solver::TypeId;
use tsz_solver::construction::TypeDatabase;
use tsz_solver::type_queries::TypeIdList;

/// Detect an indexed-access type (`T[K]`) during assignability normalization.
///
/// Used to decide whether a surface should be driven through
/// `evaluate_type_for_assignability` before the relation runs. Identical in
/// behavior to the shared solver predicate; the dedicated name marks it as the
/// assignability-pipeline entry point.
pub(crate) fn is_index_access_for_assignability(db: &dyn TypeDatabase, ty: TypeId) -> bool {
    tsz_solver::type_queries::is_index_access_type(db, ty)
}

/// Peel union members during assignability normalization.
///
/// Returns the interned member list (a zero-copy [`TypeIdList`] view) when `ty`
/// is a union so each member can be normalized independently, or `None`
/// otherwise. Mirrors `query_boundaries::common::union_members` but is owned by
/// the assignability boundary for the relation-preparation path.
pub(crate) fn union_members_for_assignability(
    db: &dyn TypeDatabase,
    ty: TypeId,
) -> Option<TypeIdList> {
    tsz_solver::type_queries::get_union_members(db, ty)
}
