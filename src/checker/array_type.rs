//! Array Type Utilities Module
//!
//! This module contains array type utility methods for CheckerState
//! as part of Phase 2 architecture refactoring.
//!
//! The methods in this module handle:
//! - Array type validation and analysis
//! - Array element type checking
//! - Array type compatibility
//! - Array type predicates
//!
//! This module extends CheckerState with utilities for array type
//! operations, providing cleaner APIs for array type checking.

use crate::checker::state::CheckerState;
use crate::solver::type_queries;
use crate::solver::{TypeId, TypeKey};

// =============================================================================
// Array Type Utilities
// =============================================================================

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Array Type Detection
    // =========================================================================

    /// Check if a type is a mutable (non-readonly) array type.
    ///
    /// Returns true for `T[]` types.
    pub fn is_mutable_array_type(&self, type_id: TypeId) -> bool {
        type_queries::is_array_type(self.ctx.types, type_id)
    }

    // =========================================================================
    // Array Element Type Extraction
    // =========================================================================

    /// Get the element type of an array type, with a fallback.
    ///
    /// Returns the element type if this is an array type,
    /// or the provided fallback type otherwise.
    pub fn get_array_element_type_or(&self, type_id: TypeId, fallback: TypeId) -> TypeId {
        type_queries::get_array_element_type(self.ctx.types, type_id).unwrap_or(fallback)
    }

    // =========================================================================
    // Array Type Compatibility
    // =========================================================================

    /// Check if two array types are compatible.
    ///
    /// Returns true if both are arrays and their element types are compatible.
    pub fn array_types_compatible(&mut self, array1: TypeId, array2: TypeId) -> bool {
        // Both must be arrays
        let elem1 = match type_queries::get_array_element_type(self.ctx.types, array1) {
            Some(e) => e,
            None => return false,
        };

        let elem2 = match type_queries::get_array_element_type(self.ctx.types, array2) {
            Some(e) => e,
            None => return false,
        };

        // Check element type assignability
        self.is_assignable_to(elem1, elem2)
    }

    /// Check if an array type is assignable to another type.
    ///
    /// This is a convenience wrapper that combines array type checking
    /// with element type assignability.
    pub fn is_array_assignable_to(&mut self, array_type: TypeId, target_type: TypeId) -> bool {
        if !self.is_mutable_array_type(array_type) {
            return false;
        }

        // Use subtype checking for proper assignability
        self.is_assignable_to(array_type, target_type)
    }

    // =========================================================================
    // Array Type Creation
    // =========================================================================

    /// Create an array type from an element type.
    pub fn create_array_type(&self, element_type: TypeId) -> TypeId {
        self.ctx.types.intern(TypeKey::Array(element_type))
    }

    // =========================================================================
    // Array Type Analysis
    // =========================================================================

    /// Check if an array type contains only primitive elements.
    ///
    /// Returns true if the array element type is a primitive type.
    pub fn is_primitive_array(&self, type_id: TypeId) -> bool {
        type_queries::get_array_element_type(self.ctx.types, type_id)
            .map(|element_type| self.is_primitive_type(element_type))
            .unwrap_or(false)
    }

    /// Check if an array type contains only literal elements.
    ///
    /// Returns true if the array element type is a literal type.
    pub fn is_literal_array(&self, type_id: TypeId) -> bool {
        type_queries::get_array_element_type(self.ctx.types, type_id)
            .map(|element_type| self.is_literal_type(element_type))
            .unwrap_or(false)
    }

    /// Check if an array type contains union elements.
    ///
    /// Returns true if the array element type is a union type.
    pub fn is_union_array(&self, type_id: TypeId) -> bool {
        type_queries::get_array_element_type(self.ctx.types, type_id)
            .map(|element_type| type_queries::is_union_type(self.ctx.types, element_type))
            .unwrap_or(false)
    }

    /// Check if an array type is homogeneous (all elements same type).
    ///
    /// Returns false if the element type is a union or tuple type.
    pub fn is_homogeneous_array(&self, type_id: TypeId) -> bool {
        type_queries::get_array_element_type(self.ctx.types, type_id)
            .map(|element_type| !type_queries::is_union_type(self.ctx.types, element_type))
            .unwrap_or(false)
    }

    /// Get the common element type if array is homogeneous.
    ///
    /// Returns Some(element_type) if the array has a single element type,
    /// or None if it's a union array or not an array.
    pub fn get_homogeneous_element_type(&self, type_id: TypeId) -> Option<TypeId> {
        type_queries::get_array_element_type(self.ctx.types, type_id)
            .filter(|&element_type| !type_queries::is_union_type(self.ctx.types, element_type))
    }
}
