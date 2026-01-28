//! Indexed Access Type Utilities Module
//!
//! This module contains indexed access type utility methods for CheckerState
//! as part of Phase 2 architecture refactoring.
//!
//! The methods in this module handle:
//! - Indexed access type detection and validation
//! - Object type and key type extraction
//! - Indexed access type evaluation helpers
//! - String and number literal index handling
//!
//! This module extends CheckerState with utilities for indexed access type
//! operations, providing cleaner APIs for T[K] type checking.

use crate::checker::state::CheckerState;
use crate::solver::TypeId;
use crate::solver::type_queries::get_index_access_types;

// =============================================================================
// Indexed Access Type Utilities
// =============================================================================

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Indexed Access Type Detection
    // =========================================================================

    /// Check if a type is an indexed access type.
    ///
    /// Returns true for types like `T[K]` (object type with index access).
    pub fn is_indexed_access_type(&self, type_id: TypeId) -> bool {
        get_index_access_types(self.ctx.types, type_id).is_some()
    }

    // =========================================================================
    // Indexed Access Type Component Extraction
    // =========================================================================

    /// Get the object type from an indexed access type.
    ///
    /// Returns the `T` in `T[K]`, or None if not an indexed access type.
    pub fn get_indexed_access_object_type(&self, type_id: TypeId) -> Option<TypeId> {
        get_index_access_types(self.ctx.types, type_id).map(|(obj, _)| obj)
    }

    /// Get the index key type from an indexed access type.
    ///
    /// Returns the `K` in `T[K]`, or None if not an indexed access type.
    pub fn get_indexed_access_key_type(&self, type_id: TypeId) -> Option<TypeId> {
        get_index_access_types(self.ctx.types, type_id).map(|(_, key)| key)
    }

    /// Get both components from an indexed access type.
    ///
    /// Returns `(object_type, key_type)` for `T[K]`, or None if not an indexed access type.
    pub fn get_indexed_access_components(&self, type_id: TypeId) -> Option<(TypeId, TypeId)> {
        get_index_access_types(self.ctx.types, type_id)
    }

    // =========================================================================
    // Indexed Access Type Analysis
    // =========================================================================

    /// Check if an indexed access uses a string literal key.
    ///
    /// Returns true if the key type is a string literal (e.g., `T["prop"]`).
    pub fn is_string_indexed_access(&self, type_id: TypeId) -> bool {
        match self.get_indexed_access_key_type(type_id) {
            Some(key_type) => self.is_string_literal_type(key_type),
            None => false,
        }
    }

    /// Check if an indexed access uses a number literal key.
    ///
    /// Returns true if the key type is a number literal (e.g., `T[0]`).
    pub fn is_number_indexed_access(&self, type_id: TypeId) -> bool {
        match self.get_indexed_access_key_type(type_id) {
            Some(key_type) => self.is_number_literal_type(key_type),
            None => false,
        }
    }

    /// Check if an indexed access uses a primitive key type.
    ///
    /// Returns true if the key type is string or number.
    pub fn is_primitive_indexed_access(&self, type_id: TypeId) -> bool {
        match self.get_indexed_access_key_type(type_id) {
            Some(key_type) => key_type == TypeId::STRING || key_type == TypeId::NUMBER,
            None => false,
        }
    }

    /// Get the string literal key value from an indexed access.
    ///
    /// Returns the string value if the key is a string literal, or None otherwise.
    pub fn get_string_key_value(&self, type_id: TypeId) -> Option<String> {
        match self.get_indexed_access_key_type(type_id) {
            Some(key_type) => self.get_string_literal_value(key_type),
            None => None,
        }
    }

    /// Get the number literal key value from an indexed access.
    ///
    /// Returns the number value if the key is a number literal, or None otherwise.
    pub fn get_number_key_value(&self, type_id: TypeId) -> Option<f64> {
        match self.get_indexed_access_key_type(type_id) {
            Some(key_type) => self.get_number_literal_value(key_type),
            None => None,
        }
    }

    // =========================================================================
    // Indexed Access Type Evaluation Helpers
    // =========================================================================

    /// Resolve a property access indexed access type.
    ///
    /// For `T["prop"]`, attempts to resolve to the property type.
    /// Returns the property type if found, or None otherwise.
    pub fn resolve_property_access(&self, type_id: TypeId) -> Option<TypeId> {
        if let (Some(object_type), Some(key_value)) = (
            self.get_indexed_access_object_type(type_id),
            self.get_string_key_value(type_id),
        ) {
            self.get_object_property_type(object_type, &key_value)
        } else {
            None
        }
    }

    /// Resolve a numeric index access type.
    ///
    /// For `T[0]` on tuple/array types, attempts to resolve to the element type.
    /// Returns the element type if found, or None otherwise.
    pub fn resolve_numeric_access(&self, type_id: TypeId) -> Option<TypeId> {
        if let (Some(object_type), Some(key_value)) = (
            self.get_indexed_access_object_type(type_id),
            self.get_number_key_value(type_id),
        ) {
            // Check if it's a tuple first
            if self.is_tuple_type(object_type) {
                return self.get_tuple_element_type(object_type, key_value as usize);
            }
            // Check if it's an array
            if self.is_array_type(object_type) {
                return self
                    .get_array_element_type_or(object_type, TypeId::UNKNOWN)
                    .into();
            }
        }
        None
    }

    /// Resolve an indexed access to its result type.
    ///
    /// This is a general-purpose helper that attempts to resolve T[K]
    /// to the actual type, handling property access, numeric access, and index signatures.
    pub fn resolve_indexed_access(&self, type_id: TypeId) -> Option<TypeId> {
        // Try property access first (string literal keys)
        if let Some(result) = self.resolve_property_access(type_id) {
            return Some(result);
        }

        // Try numeric access (number literal keys)
        if let Some(result) = self.resolve_numeric_access(type_id) {
            return Some(result);
        }

        // Try index signatures (string/number keys)
        if let Some((object_type, key_type)) = self.get_indexed_access_components(type_id) {
            // Check for string index signature
            if key_type == TypeId::STRING || self.is_string_literal_type(key_type) {
                if let Some(index_type) = self.get_string_index_type(object_type) {
                    return Some(index_type);
                }
            }

            // Check for number index signature
            if key_type == TypeId::NUMBER || self.is_number_literal_type(key_type) {
                if let Some(index_type) = self.get_number_index_type(object_type) {
                    return Some(index_type);
                }
            }
        }

        None
    }

    // =========================================================================
    // Indexed Access Type Compatibility
    // =========================================================================

    /// Check if an indexed access type is valid.
    ///
    /// Returns true if the object type can be indexed with the key type.
    pub fn is_valid_indexed_access(&self, type_id: TypeId) -> bool {
        // Must be an indexed access type
        if !self.is_indexed_access_type(type_id) {
            return false;
        }

        let (object_type, key_type) = match self.get_indexed_access_components(type_id) {
            Some(components) => components,
            None => return false,
        };

        // Check if object type can be indexed
        if self.is_object_type(object_type) || self.is_object_with_index(object_type) {
            // Objects can have property or index access
            return true;
        }

        if self.is_tuple_type(object_type) {
            // Tuples can have numeric index access
            return self.is_number_literal_type(key_type);
        }

        if self.is_array_type(object_type) {
            // Arrays can have numeric or string index access
            return key_type == TypeId::NUMBER
                || key_type == TypeId::STRING
                || self.is_number_literal_type(key_type);
        }

        false
    }

    /// Check if two indexed access types are compatible.
    ///
    /// Returns true if both are indexed access types and have compatible structure.
    pub fn indexed_access_types_compatible(&mut self, access1: TypeId, access2: TypeId) -> bool {
        // Both must be indexed access types
        if !self.is_indexed_access_type(access1) || !self.is_indexed_access_type(access2) {
            return false;
        }

        // Check subtype relationship
        self.is_assignable_to(access1, access2)
    }
}
