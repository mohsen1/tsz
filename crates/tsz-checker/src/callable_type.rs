//! Callable Type Utilities Module
//!
//! Thin wrappers for callable type queries, delegating to solver via `query_boundaries`.

use crate::query_boundaries::callable_type as query;
use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Check if a type has any call signature.
    ///
    /// Call signatures allow a type to be called as a function.
    pub fn has_call_signature(&self, type_id: TypeId) -> bool {
        query::has_call_signatures(self.ctx.types, type_id)
    }
}
