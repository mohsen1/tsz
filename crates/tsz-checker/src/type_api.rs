//! Type API Module

use crate::query_boundaries::type_api as query;
use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Check if a type is an array type.
    pub fn is_array_type(&self, ty: TypeId) -> bool {
        query::is_array_type(self.ctx.types, ty)
    }
}
