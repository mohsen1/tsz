//! Callable Type Utilities Module
//!
//! This module contains callable type utility methods for CheckerState
//! as part of Phase 2 architecture refactoring.
//!
//! The methods in this module handle:
//! - Callable type detection (function, callable interfaces)
//! - Call signature extraction and validation
//! - Function type parameter checking
//! - Callable overload detection
//! - This/binding type handling
//!
//! This module extends CheckerState with utilities for callable type
//! operations, providing cleaner APIs for function type checking.

use crate::checker::state::CheckerState;
use crate::solver::TypeId;
use crate::solver::type_queries::get_callable_shape;

// =============================================================================
// Callable Type Utilities
// =============================================================================

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Callable Type Detection
    // =========================================================================

    /// Check if a type has any call signature.
    ///
    /// Call signatures allow a type to be called as a function.
    pub fn has_call_signature(&self, type_id: TypeId) -> bool {
        if let Some(shape) = get_callable_shape(self.ctx.types, type_id) {
            !shape.call_signatures.is_empty()
        } else {
            false
        }
    }

    /// Get the number of call signatures for a callable type.
    ///
    /// Multiple call signatures indicate function overloading.
    pub fn call_signature_count(&self, type_id: TypeId) -> usize {
        if let Some(shape) = get_callable_shape(self.ctx.types, type_id) {
            shape.call_signatures.len()
        } else {
            0
        }
    }

    /// Check if a callable type is overloaded.
    ///
    /// Returns true if the type has more than one call signature.
    pub fn is_overloaded_callable(&self, type_id: TypeId) -> bool {
        self.call_signature_count(type_id) > 1
    }

    // =========================================================================
    // Callable Type Properties
    // =========================================================================

    /// Check if a callable type has properties.
    ///
    /// Some callable types (like Function) have additional properties
    /// beyond their call signatures.
    pub fn callable_has_properties(&self, type_id: TypeId) -> bool {
        if let Some(shape) = get_callable_shape(self.ctx.types, type_id) {
            !shape.properties.is_empty()
        } else {
            false
        }
    }

    /// Check if a callable type has an index signature.
    ///
    /// Returns true if the callable has a string or number index signature.
    pub fn callable_has_index_signature(&self, type_id: TypeId) -> bool {
        if let Some(shape) = get_callable_shape(self.ctx.types, type_id) {
            shape.string_index.is_some() || shape.number_index.is_some()
        } else {
            false
        }
    }

    // =========================================================================
    // Function Type Utilities
    // =========================================================================

    /// Check if a type is a generic function type.
    ///
    /// Returns true if the callable has type parameters.
    pub fn is_generic_callable(&self, type_id: TypeId) -> bool {
        if let Some(shape) = get_callable_shape(self.ctx.types, type_id) {
            // Check if any call signature has type parameters
            shape
                .call_signatures
                .iter()
                .any(|sig| !sig.type_params.is_empty())
        } else {
            false
        }
    }

    // =========================================================================
    // Callable Type Compatibility
    // =========================================================================

    /// Check if two callable types have compatible signatures.
    ///
    /// Returns true if both types are callable and have at least
    /// one compatible signature.
    pub fn callable_signatures_compatible(&mut self, type1: TypeId, type2: TypeId) -> bool {
        // Both must be callable
        if !self.has_call_signature(type1) || !self.has_call_signature(type2) {
            return false;
        }

        // Check subtype relationship
        self.is_assignable_to(type1, type2)
    }

    /// Get the more specific callable type from two candidates.
    ///
    /// Returns the type that is a subtype of the other, or the first type
    /// if they are unrelated.
    pub fn more_specific_callable(&mut self, type1: TypeId, type2: TypeId) -> TypeId {
        // If type1 is assignable to type2, type1 is more specific
        if self.is_assignable_to(type1, type2) {
            return type1;
        }
        // If type2 is assignable to type1, type2 is more specific
        if self.is_assignable_to(type2, type1) {
            return type2;
        }
        // Unrelated types, return type1 as default
        type1
    }

    // =========================================================================
    // Callable Type Creation Helpers
    // =========================================================================

    /// Create a basic function type from parameter and return types.
    ///
    /// This is a convenience method for creating simple function types.
    /// For more complex functions, use the full type builder.
    pub fn create_function_type(&self, params: Vec<TypeId>, return_type: TypeId) -> TypeId {
        use crate::solver::{CallSignature, CallableShape, ParamInfo};

        let signature = CallSignature {
            type_params: vec![],
            params: params
                .into_iter()
                .map(|p| ParamInfo {
                    name: None,
                    type_id: p,
                    optional: false,
                    rest: false,
                })
                .collect(),
            this_type: None,
            return_type,
            type_predicate: None,
            is_method: false,
        };

        let shape = CallableShape {
            call_signatures: vec![signature],
            construct_signatures: vec![],
            properties: vec![],
            string_index: None,
            number_index: None,
            symbol: None,
        };

        self.ctx.types.callable(shape)
    }
}
