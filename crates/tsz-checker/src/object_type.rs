//! Object Type Utilities Module
//!
//! Thin wrappers for object type queries, delegating to solver via `query_boundaries`.

use crate::query_boundaries::object_type::object_shape_for_type;
use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Get the type of a property by name.
    ///
    /// Returns the property type if found, or None otherwise.
    pub fn get_object_property_type(
        &self,
        object_type: TypeId,
        property_name: &str,
    ) -> Option<TypeId> {
        let shape = object_shape_for_type(self.ctx.types, object_type)?;
        let name_atom = self.ctx.types.intern_string(property_name);
        shape
            .properties
            .iter()
            .find(|prop| prop.name == name_atom)
            .map(|prop| prop.type_id)
    }

    /// Check if an object has a specific property.
    ///
    /// Returns true if the property exists on the object.
    pub fn object_has_property(&self, object_type: TypeId, property_name: &str) -> bool {
        self.get_object_property_type(object_type, property_name)
            .is_some()
    }

    /// Check if a property is optional.
    ///
    /// Returns true if the property is marked as optional.
    pub fn is_property_optional(&self, object_type: TypeId, property_name: &str) -> bool {
        if let Some(shape) = object_shape_for_type(self.ctx.types, object_type) {
            let name_atom = self.ctx.types.intern_string(property_name);
            shape
                .properties
                .iter()
                .find(|prop| prop.name == name_atom)
                .is_some_and(|prop| prop.optional)
        } else {
            false
        }
    }
}
