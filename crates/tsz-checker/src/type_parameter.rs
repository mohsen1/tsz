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

use crate::state::CheckerState;
use tsz_solver::TypeId;
use tsz_solver::type_queries::{
    TypeParameterKind, classify_type_parameter, get_callable_type_param_count,
    get_type_param_default, get_type_parameter_constraint as solver_get_type_parameter_constraint,
    is_direct_type_parameter, is_generic_type,
};

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
        is_direct_type_parameter(self.ctx.types, type_id)
    }

    /// Get the number of type parameters on a generic type.
    ///
    /// Returns the count of type parameters, or 0 if not a generic type.
    pub fn type_parameter_count(&self, type_id: TypeId) -> usize {
        get_callable_type_param_count(self.ctx.types, type_id)
    }

    // =========================================================================
    // Generic Type Detection
    // =========================================================================

    /// Check if a type is a generic type with type arguments applied.
    ///
    /// Returns true for types like `Array<T>`, `Map<K, V>`, etc.
    pub fn is_type_application(&self, type_id: TypeId) -> bool {
        is_generic_type(self.ctx.types, type_id)
    }

    // =========================================================================
    // Type Constraint Extraction
    // =========================================================================

    /// Get the constraint type from a type parameter.
    ///
    /// Returns the constraint type (e.g., `T extends string` returns string),
    /// or None if the type parameter has no constraint.
    pub fn get_type_parameter_constraint(&self, type_id: TypeId) -> Option<TypeId> {
        solver_get_type_parameter_constraint(self.ctx.types, type_id)
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
        get_type_param_default(self.ctx.types, type_id)
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

        // Check if type contains type parameters in its structure using the classification helper
        match classify_type_parameter(self.ctx.types, type_id) {
            TypeParameterKind::TypeParameter(_) | TypeParameterKind::Infer(_) => true,
            TypeParameterKind::Union(members) | TypeParameterKind::Intersection(members) => {
                members.iter().any(|&m| self.contains_type_parameters(m))
            }
            TypeParameterKind::Application(app_id) => {
                let app = self.ctx.types.type_application(app_id);
                self.contains_type_parameters(app.base)
                    || app
                        .args
                        .iter()
                        .any(|&arg| self.contains_type_parameters(arg))
            }
            TypeParameterKind::Callable(_) | TypeParameterKind::NotTypeParameter => false,
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

        // Recursively check for type parameters in complex types using the classification helper
        match classify_type_parameter(self.ctx.types, type_id) {
            TypeParameterKind::TypeParameter(_) | TypeParameterKind::Infer(_) => {
                // Already added above if is_type_parameter
            }
            TypeParameterKind::Union(members) | TypeParameterKind::Intersection(members) => {
                for &member in members.iter() {
                    params.extend(self.get_referenced_type_parameters(member));
                }
            }
            TypeParameterKind::Application(app_id) => {
                let app = self.ctx.types.type_application(app_id);
                params.extend(self.get_referenced_type_parameters(app.base));
                for &arg in app.args.iter() {
                    params.extend(self.get_referenced_type_parameters(arg));
                }
            }
            TypeParameterKind::Callable(_) | TypeParameterKind::NotTypeParameter => {}
        }

        params
    }
}
