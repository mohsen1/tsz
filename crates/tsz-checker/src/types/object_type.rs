//! Object Type Utilities Module
//!
//! Thin wrappers for object type queries, delegating to solver via `query_boundaries`.

use crate::query_boundaries::class_type::object_shape_for_type;
use crate::state::CheckerState;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Get the type of a property by name.
    ///
    /// Returns the property type if found, or None otherwise.
    fn get_object_property_type(&self, object_type: TypeId, property_name: &str) -> Option<TypeId> {
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
    /// Returns true if the property is declared optional anywhere it is visible
    /// on the receiver's apparent type — including base interfaces reached
    /// through `extends` heritage and members surfaced only through a deferred
    /// `Application` / intersection / union receiver. Delegates to the solver's
    /// heritage-aware [`tsz_solver::objects::property_is_optional`] so a
    /// base-only optional property is not misread as required (which would emit
    /// a false TS2790 on `delete`).
    pub fn is_property_optional(&self, object_type: TypeId, property_name: &str) -> bool {
        let name_atom = self.ctx.types.intern_string(property_name);
        tsz_solver::objects::property_is_optional(object_type, name_atom, self.ctx.types, &self.ctx)
    }
}
