//! Type API Module
//!
//! This module provides convenience wrappers around type queries
//! for use within the checker.

use crate::query_boundaries::type_api as query;
use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Check if a type is an object type.
    pub fn is_object_type(&self, ty: TypeId) -> bool {
        query::is_object_type(self.ctx.types, ty)
    }

    /// Check if a type is an array type.
    pub fn is_array_type(&self, ty: TypeId) -> bool {
        query::is_array_type(self.ctx.types, ty)
    }

    /// Check if a type is a tuple type.
    pub fn is_tuple_type(&self, ty: TypeId) -> bool {
        query::is_tuple_type(self.ctx.types, ty)
    }

    /// Check if a type is a literal type.
    pub fn is_literal_type(&self, ty: TypeId) -> bool {
        query::is_literal_type(self.ctx.types, ty)
    }
}
