//! Union Type Utilities Module
//!
//! Thin wrappers for union type queries, delegating to solver via `query_boundaries`.

use crate::query_boundaries::union_type as query;
use crate::state::CheckerState;
use tsz_solver::TypeId;

impl CheckerState<'_> {
    /// Get the members of a union type.
    ///
    /// Returns a vector of `TypeIds` representing all members of the union.
    /// Returns an empty vec if the type is not a union.
    pub fn get_union_members(&self, type_id: TypeId) -> Vec<TypeId> {
        query::union_members(self.ctx.types, type_id).unwrap_or_default()
    }
}
