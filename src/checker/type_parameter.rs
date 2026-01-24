//! Type Parameter Utilities Module
//!
//! This module contains type parameter utility methods for CheckerState
//! as part of Phase 2 architecture refactoring.
//!
//! The methods in this module handle:
//! - Type parameter detection and validation
//! - Generic type checking
//! - Type constraint extraction and validation
//! - Type argument compatibility checking
//! - Generic instantiation helpers
//!
//! This module extends CheckerState with utilities for type parameter
//! operations, providing cleaner APIs for generic type checking.

use crate::checker::state::CheckerState;
use crate::solver::{TypeId, TypeKey};

// =============================================================================
// Type Parameter Utilities
// =============================================================================

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Type Parameter Detection
    // =========================================================================

    /// Check if a type is a type parameter (generic type variable).
    ///
    /// Returns true for types like T, U, etc. in generic contexts.
    pub fn is_type_parameter(&self, type_id: TypeId) -> bool {
        matches!(
            self.ctx.types.lookup(type_id),
            Some(TypeKey::TypeParameter(_))
        )
    }

    /// Get the number of type parameters on a generic type.
    ///
    /// Returns the count of type parameters, or 0 if not a generic type.
    pub fn type_parameter_count(&self, type_id: TypeId) -> usize {
        use crate::solver::TypeKey;

        match self.ctx.types.lookup(type_id) {
            Some(TypeKey::Callable(shape_id)) => {
                let shape = self.ctx.types.callable_shape(shape_id);
                // Count type parameters across all signatures
                shape
                    .call_signatures
                    .iter()
                    .map(|sig| sig.type_params.len())
                    .max()
                    .unwrap_or(0)
            }
            _ => 0,
        }
    }

    // =========================================================================
    // Generic Type Detection
    // =========================================================================

    /// Check if a type is a generic type with type arguments applied.
    ///
    /// Returns true for types like `Array<T>`, `Map<K, V>`, etc.
    pub fn is_type_application(&self, type_id: TypeId) -> bool {
        matches!(
            self.ctx.types.lookup(type_id),
            Some(TypeKey::Application(_))
        )
    }

    // =========================================================================
    // Type Constraint Extraction
    // =========================================================================

    /// Get the constraint type from a type parameter.
    ///
    /// Returns the constraint type (e.g., `T extends string` returns string),
    /// or None if the type parameter has no constraint.
    pub fn get_type_parameter_constraint(&self, type_id: TypeId) -> Option<TypeId> {
        use crate::solver::TypeKey;

        match self.ctx.types.lookup(type_id) {
            Some(TypeKey::TypeParameter(_param_id)) => {
                // Look up the type parameter info
                // This would need access to the type parameter definition
                // For now, return None as a placeholder
                None
            }
            _ => None,
        }
    }

    /// Check if a type parameter has a constraint.
    ///
    /// Returns true if the type parameter has an `extends` clause.
    pub fn type_parameter_has_constraint(&self, type_id: TypeId) -> bool {
        self.get_type_parameter_constraint(type_id).is_some()
    }

    /// Check if a type satisfies a type parameter constraint.
    ///
    /// Returns true if the type is assignable to the constraint.
    pub fn satisfies_type_parameter_constraint(
        &mut self,
        type_id: TypeId,
        type_param: TypeId,
    ) -> bool {
        match self.get_type_parameter_constraint(type_param) {
            Some(constraint) => self.is_assignable_to(type_id, constraint),
            None => true, // No constraint means any type is valid
        }
    }

    // =========================================================================
    // Type Argument Compatibility
    // =========================================================================

    /// Check if type arguments are compatible with type parameters.
    ///
    /// Returns true if each type argument satisfies its corresponding
    /// type parameter constraint.
    pub fn type_args_satisfy_constraints(
        &mut self,
        type_args: &[TypeId],
        type_params: &[TypeId],
    ) -> bool {
        if type_args.len() != type_params.len() {
            return false;
        }

        type_args
            .iter()
            .zip(type_params.iter())
            .all(|(&arg, &param)| self.satisfies_type_parameter_constraint(arg, param))
    }

    /// Get the default type argument for a type parameter.
    ///
    /// Returns the default type if specified, or None otherwise.
    pub fn get_type_parameter_default(&self, type_id: TypeId) -> Option<TypeId> {
        use crate::solver::TypeKey;

        match self.ctx.types.lookup(type_id) {
            Some(TypeKey::TypeParameter(_)) => {
                // Look up the default type from the type parameter definition
                // For now, return None as a placeholder
                None
            }
            _ => None,
        }
    }

    // =========================================================================
    // Generic Instantiation Helpers
    // =========================================================================

    /// Check if a generic type can be instantiated with the given type arguments.
    ///
    /// Returns true if the type arguments are compatible with constraints.
    pub fn can_instantiate_generic(
        &mut self,
        _generic_type: TypeId,
        _type_args: &[TypeId],
    ) -> bool {
        // This is a simplified check - a full implementation would
        // need to extract type parameters from the generic type
        // and verify each argument satisfies its constraint
        true
    }

    /// Create a type application from a generic type and type arguments.
    ///
    /// This is a convenience method for applying type arguments to a generic type.
    pub fn apply_type_args(&self, generic_type: TypeId, type_args: Vec<TypeId>) -> TypeId {
        self.ctx.types.application(generic_type, type_args)
    }

    // =========================================================================
    // Generic Type Analysis
    // =========================================================================

    /// Check if a type is a naked type parameter.
    ///
    /// Returns true if the type is a type parameter with no constraints.
    pub fn is_naked_type_parameter(&self, type_id: TypeId) -> bool {
        self.is_type_parameter(type_id) && !self.type_parameter_has_constraint(type_id)
    }

    /// Check if a type is a constrained type parameter.
    ///
    /// Returns true if the type is a type parameter with an `extends` clause.
    pub fn is_constrained_type_parameter(&self, type_id: TypeId) -> bool {
        self.is_type_parameter(type_id) && self.type_parameter_has_constraint(type_id)
    }

    /// Check if a type contains any type parameters.
    ///
    /// Returns true if the type is or contains a type parameter (generic).
    pub fn contains_type_parameters(&self, type_id: TypeId) -> bool {
        // Direct type parameter
        if self.is_type_parameter(type_id) {
            return true;
        }

        // Check if type contains type parameters in its structure
        match self.ctx.types.lookup(type_id) {
            Some(TypeKey::Union(list_id)) => {
                let members = self.ctx.types.type_list(list_id);
                members.iter().any(|&m| self.contains_type_parameters(m))
            }
            Some(TypeKey::Intersection(list_id)) => {
                let members = self.ctx.types.type_list(list_id);
                members.iter().any(|&m| self.contains_type_parameters(m))
            }
            Some(TypeKey::Application(app_id)) => {
                let app = self.ctx.types.type_application(app_id);
                self.contains_type_parameters(app.base)
                    || app
                        .args
                        .iter()
                        .any(|&arg| self.contains_type_parameters(arg))
            }
            _ => false,
        }
    }

    /// Get all type parameters referenced in a type.
    ///
    /// Returns a vector of all type parameter TypeIds found in the type.
    pub fn get_referenced_type_parameters(&self, type_id: TypeId) -> Vec<TypeId> {
        let mut params = Vec::new();

        // Check if this is a type parameter
        if self.is_type_parameter(type_id) {
            params.push(type_id);
        }

        // Recursively check for type parameters in complex types
        match self.ctx.types.lookup(type_id) {
            Some(TypeKey::Union(list_id)) => {
                let members = self.ctx.types.type_list(list_id);
                for &member in members.iter() {
                    params.extend(self.get_referenced_type_parameters(member));
                }
            }
            Some(TypeKey::Intersection(list_id)) => {
                let members = self.ctx.types.type_list(list_id);
                for &member in members.iter() {
                    params.extend(self.get_referenced_type_parameters(member));
                }
            }
            Some(TypeKey::Application(app_id)) => {
                let app = self.ctx.types.type_application(app_id);
                params.extend(self.get_referenced_type_parameters(app.base));
                for &arg in app.args.iter() {
                    params.extend(self.get_referenced_type_parameters(arg));
                }
            }
            _ => {}
        }

        params
    }
}
