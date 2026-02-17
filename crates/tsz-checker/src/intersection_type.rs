//! Intersection Type Utilities Module
//!
//! Thin wrappers for intersection type queries, delegating to solver via `query_boundaries`.

use crate::query_boundaries::intersection_type as query;
use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Get the members of an intersection type.
    ///
    /// Returns a vector of `TypeIds` representing all members of the intersection.
    /// Returns an empty vec if the type is not an intersection.
    pub fn get_intersection_members(&self, type_id: TypeId) -> Vec<TypeId> {
        query::intersection_members(self.ctx.types, type_id).unwrap_or_default()
    }
}
