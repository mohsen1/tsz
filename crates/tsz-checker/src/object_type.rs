//! Object Type Utilities Module
//!
//! This module contains object type utility methods for CheckerState
//! as part of Phase 2 architecture refactoring.
//!
//! The methods in this module handle:
//! - Object type detection and validation
//! - Property type extraction and checking
//! - Index signature handling
//! - Object type compatibility
//!
//! This module extends CheckerState with utilities for object type
//! operations, providing cleaner APIs for object type checking.

use crate::state::CheckerState;
use tsz_solver::TypeId;
use tsz_solver::type_queries;

// =============================================================================
// Object Type Utilities
// =============================================================================

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Object Type Detection
    // =========================================================================

    /// Check if a type is an object type with properties.
    ///
    /// Returns true for regular object types.
    pub fn is_object_with_properties(&self, type_id: TypeId) -> bool {
        type_queries::is_object_type(self.ctx.types, type_id)
    }

    /// Check if a type is an object type with index signatures.
    ///
    /// Returns true for objects with string or number index signatures.
    pub fn is_object_with_index(&self, type_id: TypeId) -> bool {
        // Check if it's an object type with index signatures by looking at the shape
        if let Some(shape) = type_queries::get_object_shape(self.ctx.types, type_id) {
            shape.string_index.is_some() || shape.number_index.is_some()
        } else {
            false
        }
    }

    /// Get the number of properties in an object type.
    ///
    /// Returns 0 if the type is not an object.
    pub fn object_property_count(&self, type_id: TypeId) -> usize {
        type_queries::get_object_shape(self.ctx.types, type_id)
            .map(|shape| shape.properties.len())
            .unwrap_or(0)
    }

    // =========================================================================
    // Property Type Extraction
    // =========================================================================

    /// Get the type of a property by name.
    ///
    /// Returns the property type if found, or None otherwise.
    pub fn get_object_property_type(
        &self,
        object_type: TypeId,
        property_name: &str,
    ) -> Option<TypeId> {
        let shape = type_queries::get_object_shape(self.ctx.types, object_type)?;
        let name_atom = self.ctx.types.intern_string(property_name);
        shape
            .properties
            .iter()
            .find(|prop| prop.name == name_atom)
            .map(|prop| prop.type_id)
    }

    /// Get the type of a property by name, with a fallback.
    ///
    /// Returns the property type if found, or the provided fallback.
    pub fn get_object_property_type_or(
        &self,
        object_type: TypeId,
        property_name: &str,
        fallback: TypeId,
    ) -> TypeId {
        self.get_object_property_type(object_type, property_name)
            .unwrap_or(fallback)
    }

    /// Check if an object has a specific property.
    ///
    /// Returns true if the property exists on the object.
    pub fn object_has_property(&self, object_type: TypeId, property_name: &str) -> bool {
        self.get_object_property_type(object_type, property_name)
            .is_some()
    }

    /// Check if an object has any properties.
    ///
    /// Returns true if the object has at least one property.
    pub fn object_has_properties(&self, type_id: TypeId) -> bool {
        self.object_property_count(type_id) > 0
    }

    // =========================================================================
    // Property Property Checks
    // =========================================================================

    /// Check if a property is optional.
    ///
    /// Returns true if the property is marked as optional.
    pub fn is_property_optional(&self, object_type: TypeId, property_name: &str) -> bool {
        if let Some(shape) = type_queries::get_object_shape(self.ctx.types, object_type) {
            let name_atom = self.ctx.types.intern_string(property_name);
            shape
                .properties
                .iter()
                .find(|prop| prop.name == name_atom)
                .map(|prop| prop.optional)
                .unwrap_or(false)
        } else {
            false
        }
    }

    /// Check if a property is readonly.
    ///
    /// Returns true if the property is marked as readonly.
    pub fn is_object_property_readonly(&self, object_type: TypeId, property_name: &str) -> bool {
        if let Some(shape) = type_queries::get_object_shape(self.ctx.types, object_type) {
            let name_atom = self.ctx.types.intern_string(property_name);
            shape
                .properties
                .iter()
                .find(|prop| prop.name == name_atom)
                .map(|prop| prop.readonly)
                .unwrap_or(false)
        } else {
            false
        }
    }

    /// Check if a property is a method.
    ///
    /// Returns true if the property is marked as a method.
    pub fn is_property_method(&self, object_type: TypeId, property_name: &str) -> bool {
        if let Some(shape) = type_queries::get_object_shape(self.ctx.types, object_type) {
            let name_atom = self.ctx.types.intern_string(property_name);
            shape
                .properties
                .iter()
                .find(|prop| prop.name == name_atom)
                .map(|prop| prop.is_method)
                .unwrap_or(false)
        } else {
            false
        }
    }

    /// Check if an object has any optional properties.
    ///
    /// Returns true if any property on the object is optional.
    pub fn object_has_optional_properties(&self, type_id: TypeId) -> bool {
        type_queries::get_object_shape(self.ctx.types, type_id)
            .map(|shape| shape.properties.iter().any(|prop| prop.optional))
            .unwrap_or(false)
    }

    /// Check if an object has any readonly properties.
    ///
    /// Returns true if any property on the object is readonly.
    pub fn object_has_readonly_properties(&self, type_id: TypeId) -> bool {
        type_queries::get_object_shape(self.ctx.types, type_id)
            .map(|shape| shape.properties.iter().any(|prop| prop.readonly))
            .unwrap_or(false)
    }

    // =========================================================================
    // Index Signature Handling
    // =========================================================================

    /// Check if an object has a string index signature.
    ///
    /// Returns true for objects like `{ [key: string]: T }`.
    pub fn object_has_string_index(&self, type_id: TypeId) -> bool {
        type_queries::get_object_shape(self.ctx.types, type_id)
            .map(|shape| shape.string_index.is_some())
            .unwrap_or(false)
    }

    /// Check if an object has a number index signature.
    ///
    /// Returns true for objects like `{ [key: number]: T }`.
    pub fn object_has_number_index(&self, type_id: TypeId) -> bool {
        type_queries::get_object_shape(self.ctx.types, type_id)
            .map(|shape| shape.number_index.is_some())
            .unwrap_or(false)
    }

    /// Check if an object has any index signature.
    ///
    /// Returns true if the object has either a string or number index signature.
    pub fn object_has_index_signature(&self, type_id: TypeId) -> bool {
        self.object_has_string_index(type_id) || self.object_has_number_index(type_id)
    }

    /// Get the string index signature type from an object.
    ///
    /// Returns the string index type if present, or None otherwise.
    pub fn get_string_index_type(&self, object_type: TypeId) -> Option<TypeId> {
        type_queries::get_object_shape(self.ctx.types, object_type)
            .and_then(|shape| shape.string_index.as_ref().map(|sig| sig.value_type))
    }

    /// Get the number index signature type from an object.
    ///
    /// Returns the number index type if present, or None otherwise.
    pub fn get_number_index_type(&self, object_type: TypeId) -> Option<TypeId> {
        type_queries::get_object_shape(self.ctx.types, object_type)
            .and_then(|shape| shape.number_index.as_ref().map(|sig| sig.value_type))
    }

    // =========================================================================
    // Object Type Compatibility
    // =========================================================================

    /// Check if two object types are compatible.
    ///
    /// Returns true if both are objects and have compatible structure.
    pub fn object_types_compatible(&mut self, object1: TypeId, object2: TypeId) -> bool {
        // Both must be object types
        let is_o1_object = tsz_solver::type_queries::is_object_type(self.ctx.types, object1);
        let is_o2_object = tsz_solver::type_queries::is_object_type(self.ctx.types, object2);

        if !is_o1_object || !is_o2_object {
            return false;
        }

        // Check subtype relationship
        self.is_assignable_to(object1, object2)
    }

    /// Check if an object type is assignable to another type.
    ///
    /// This is a convenience wrapper that combines object type checking
    /// with structure compatibility.
    pub fn is_object_assignable_to(&mut self, object_type: TypeId, target_type: TypeId) -> bool {
        let is_object = tsz_solver::type_queries::is_object_type(self.ctx.types, object_type);
        if !is_object {
            return false;
        }

        // Use subtype checking for proper assignability
        self.is_assignable_to(object_type, target_type)
    }

    // =========================================================================
    // Object Type Analysis
    // =========================================================================

    /// Check if an object is empty (has no properties).
    ///
    /// Returns true for object types with no properties.
    pub fn is_empty_object(&self, type_id: TypeId) -> bool {
        self.object_property_count(type_id) == 0
    }

    /// Check if an object is a dictionary-like object.
    ///
    /// Returns true if the object has an index signature but few named properties.
    pub fn is_dictionary_object(&self, type_id: TypeId) -> bool {
        self.object_has_index_signature(type_id)
    }

    /// Get all property names from an object type.
    ///
    /// Returns a vector of property names in order.
    pub fn get_object_property_names(&self, object_type: TypeId) -> Vec<String> {
        type_queries::get_object_shape(self.ctx.types, object_type)
            .map(|shape| {
                shape
                    .properties
                    .iter()
                    .map(|prop| self.ctx.types.resolve_atom_ref(prop.name).to_string())
                    .collect()
            })
            .unwrap_or_default()
    }
}
