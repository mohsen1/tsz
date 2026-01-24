//! Tuple Type Utilities Module
//!
//! This module contains tuple type utility methods for CheckerState
//! as part of Phase 2 architecture refactoring.
//!
//! The methods in this module handle:
//! - Tuple type detection and validation
//! - Tuple element type extraction and manipulation
//! - Tuple type compatibility checking
//! - Optional and rest element handling
//!
//! This module extends CheckerState with utilities for tuple type
//! operations, providing cleaner APIs for tuple type checking.

use crate::checker::state::CheckerState;
use crate::solver::{TypeId, TypeKey};

// =============================================================================
// Tuple Type Utilities
// =============================================================================

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Tuple Type Detection
    // =========================================================================

    /// Get the number of elements in a tuple type.
    ///
    /// Returns 0 if the type is not a tuple.
    pub fn tuple_element_count(&self, type_id: TypeId) -> usize {
        match self.ctx.types.lookup(type_id) {
            Some(TypeKey::Tuple(list_id)) => {
                let elements = self.ctx.types.tuple_list(list_id);
                elements.len()
            }
            _ => 0,
        }
    }

    // =========================================================================
    // Tuple Element Type Extraction
    // =========================================================================

    /// Get the type of a tuple element at a specific index.
    ///
    /// Returns the element type if the index is valid and this is a tuple,
    /// or None otherwise.
    pub fn get_tuple_element_type(&self, tuple_type: TypeId, index: usize) -> Option<TypeId> {
        match self.ctx.types.lookup(tuple_type) {
            Some(TypeKey::Tuple(list_id)) => {
                let elements = self.ctx.types.tuple_list(list_id);
                elements.get(index).map(|elem| elem.type_id)
            }
            _ => None,
        }
    }

    /// Get the type of a tuple element at a specific index, with a fallback.
    ///
    /// Returns the element type if the index is valid and this is a tuple,
    /// or the provided fallback type otherwise.
    pub fn get_tuple_element_type_or(
        &self,
        tuple_type: TypeId,
        index: usize,
        fallback: TypeId,
    ) -> TypeId {
        self.get_tuple_element_type(tuple_type, index)
            .unwrap_or(fallback)
    }

    /// Get all element types from a tuple type.
    ///
    /// Returns a vector of TypeIds representing all elements in order.
    /// Returns an empty vec if the type is not a tuple.
    pub fn get_tuple_element_types(&self, tuple_type: TypeId) -> Vec<TypeId> {
        match self.ctx.types.lookup(tuple_type) {
            Some(TypeKey::Tuple(list_id)) => {
                let elements = self.ctx.types.tuple_list(list_id);
                elements.iter().map(|elem| elem.type_id).collect()
            }
            _ => Vec::new(),
        }
    }

    // =========================================================================
    // Tuple Element Properties
    // =========================================================================

    /// Check if a tuple element at a specific index is optional.
    ///
    /// Returns true if the element has an optional flag.
    pub fn is_tuple_element_optional(&self, tuple_type: TypeId, index: usize) -> bool {
        match self.ctx.types.lookup(tuple_type) {
            Some(TypeKey::Tuple(list_id)) => {
                let elements = self.ctx.types.tuple_list(list_id);
                elements
                    .get(index)
                    .map(|elem| elem.optional)
                    .unwrap_or(false)
            }
            _ => false,
        }
    }

    /// Check if a tuple has any optional elements.
    ///
    /// Returns true if any element in the tuple is optional.
    pub fn tuple_has_optional_elements(&self, tuple_type: TypeId) -> bool {
        match self.ctx.types.lookup(tuple_type) {
            Some(TypeKey::Tuple(list_id)) => {
                let elements = self.ctx.types.tuple_list(list_id);
                elements.iter().any(|elem| elem.optional)
            }
            _ => false,
        }
    }

    /// Check if a tuple has a rest element.
    ///
    /// Returns true if the last element is a rest element (e.g., ...string[]).
    pub fn tuple_has_rest_element(&self, tuple_type: TypeId) -> bool {
        match self.ctx.types.lookup(tuple_type) {
            Some(TypeKey::Tuple(list_id)) => {
                let elements = self.ctx.types.tuple_list(list_id);
                elements.last().map(|elem| elem.rest).unwrap_or(false)
            }
            _ => false,
        }
    }

    /// Get the rest element type from a tuple.
    ///
    /// Returns the rest element type if present, or None otherwise.
    pub fn get_tuple_rest_element_type(&self, tuple_type: TypeId) -> Option<TypeId> {
        match self.ctx.types.lookup(tuple_type) {
            Some(TypeKey::Tuple(list_id)) => {
                let elements = self.ctx.types.tuple_list(list_id);
                elements
                    .last()
                    .filter(|elem| elem.rest)
                    .map(|elem| elem.type_id)
            }
            _ => None,
        }
    }

    /// Check if a tuple element at a specific index is named.
    ///
    /// Returns true if the element has a name (e.g., `[name: string]`).
    pub fn is_tuple_element_named(&self, tuple_type: TypeId, index: usize) -> bool {
        match self.ctx.types.lookup(tuple_type) {
            Some(TypeKey::Tuple(list_id)) => {
                let elements = self.ctx.types.tuple_list(list_id);
                elements
                    .get(index)
                    .map(|elem| elem.name.is_some())
                    .unwrap_or(false)
            }
            _ => false,
        }
    }

    /// Get the name of a tuple element at a specific index.
    ///
    /// Returns the element name if present and named, or None otherwise.
    pub fn get_tuple_element_name(&self, tuple_type: TypeId, index: usize) -> Option<String> {
        match self.ctx.types.lookup(tuple_type) {
            Some(TypeKey::Tuple(list_id)) => {
                let elements = self.ctx.types.tuple_list(list_id);
                elements.get(index).and_then(|elem| {
                    elem.name
                        .map(|atom| self.ctx.types.resolve_atom_ref(atom).to_string())
                })
            }
            _ => None,
        }
    }

    // =========================================================================
    // Tuple Type Compatibility
    // =========================================================================

    /// Check if two tuple types are compatible.
    ///
    /// Returns true if both are tuples and have compatible element types.
    pub fn tuple_types_compatible(&mut self, tuple1: TypeId, tuple2: TypeId) -> bool {
        // Both must be tuples
        let is_t1_tuple = matches!(self.ctx.types.lookup(tuple1), Some(TypeKey::Tuple(_)));
        let is_t2_tuple = matches!(self.ctx.types.lookup(tuple2), Some(TypeKey::Tuple(_)));

        if !is_t1_tuple || !is_t2_tuple {
            return false;
        }

        // Check subtype relationship
        self.is_assignable_to(tuple1, tuple2)
    }

    /// Check if a tuple type is assignable to another type.
    ///
    /// This is a convenience wrapper that combines tuple type checking
    /// with element type assignability.
    pub fn is_tuple_assignable_to(&mut self, tuple_type: TypeId, target_type: TypeId) -> bool {
        let is_tuple = matches!(self.ctx.types.lookup(tuple_type), Some(TypeKey::Tuple(_)));
        if !is_tuple {
            return false;
        }

        // Use subtype checking for proper assignability
        self.is_assignable_to(tuple_type, target_type)
    }

    // =========================================================================
    // Tuple Type Analysis
    // =========================================================================

    /// Get the minimum length of a tuple (excluding optional elements).
    ///
    /// Returns the count of non-optional elements before the first optional
    /// or rest element.
    pub fn get_tuple_min_length(&self, tuple_type: TypeId) -> usize {
        match self.ctx.types.lookup(tuple_type) {
            Some(TypeKey::Tuple(list_id)) => {
                let elements = self.ctx.types.tuple_list(list_id);
                // Count elements until we hit an optional or rest element
                elements
                    .iter()
                    .take_while(|elem| !elem.optional && !elem.rest)
                    .count()
            }
            _ => 0,
        }
    }

    /// Get the fixed-length portion of a tuple type.
    ///
    /// Returns the number of elements before any rest element.
    pub fn get_tuple_fixed_length(&self, tuple_type: TypeId) -> usize {
        match self.ctx.types.lookup(tuple_type) {
            Some(TypeKey::Tuple(list_id)) => {
                let elements = self.ctx.types.tuple_list(list_id);
                // Count elements until we hit a rest element
                elements.iter().take_while(|elem| !elem.rest).count()
            }
            _ => 0,
        }
    }

    /// Check if a tuple is a homogeneous array-like tuple.
    ///
    /// Returns true if all elements have the same type (e.g., `[number, number]`).
    pub fn is_homogeneous_tuple(&self, tuple_type: TypeId) -> bool {
        match self.ctx.types.lookup(tuple_type) {
            Some(TypeKey::Tuple(list_id)) => {
                let elements = self.ctx.types.tuple_list(list_id);
                if elements.is_empty() {
                    return true;
                }
                let first_type = elements[0].type_id;
                elements.iter().all(|elem| elem.type_id == first_type)
            }
            _ => false,
        }
    }

    /// Get the common element type if tuple is homogeneous.
    ///
    /// Returns Some(element_type) if all elements have the same type,
    /// or None otherwise.
    pub fn get_homogeneous_tuple_element_type(&self, tuple_type: TypeId) -> Option<TypeId> {
        if self.is_homogeneous_tuple(tuple_type) {
            match self.ctx.types.lookup(tuple_type) {
                Some(TypeKey::Tuple(list_id)) => {
                    let elements = self.ctx.types.tuple_list(list_id);
                    if !elements.is_empty() {
                        return Some(elements[0].type_id);
                    }
                }
                _ => {}
            }
        }
        None
    }

    /// Check if a tuple is empty.
    ///
    /// Returns true for the empty tuple type `[]`.
    pub fn is_empty_tuple(&self, tuple_type: TypeId) -> bool {
        match self.ctx.types.lookup(tuple_type) {
            Some(TypeKey::Tuple(list_id)) => {
                let elements = self.ctx.types.tuple_list(list_id);
                elements.is_empty()
            }
            _ => false,
        }
    }
}
