//! Tuple Type Utilities Module
//!
//! Thin wrappers for tuple type queries, delegating to solver via `query_boundaries`.

use crate::query_boundaries::tuple_type as query;
use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Get the type of a tuple element at a specific index.
    ///
    /// Returns the element type if the index is valid and this is a tuple,
    /// or None otherwise.
    pub fn get_tuple_element_type(&self, tuple_type: TypeId, index: usize) -> Option<TypeId> {
        query::tuple_elements(self.ctx.types, tuple_type)
            .and_then(|elements| elements.get(index).map(|elem| elem.type_id))
    }
}
