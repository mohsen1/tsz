//! Type Computation Module
//!
//! This module contains type computation methods for CheckerState
//! as part of Phase 2 architecture refactoring.
//!
//! The methods in this module handle:
//! - Type computation helpers
//! - Type relationship queries
//! - Type format utilities
//!
//! This module extends CheckerState with additional methods for type-related
//! operations, providing cleaner APIs for common patterns.

use crate::checker::state::CheckerState;
use crate::parser::NodeIndex;
use crate::solver::TypeId;

// =============================================================================
// Type Computation Methods
// =============================================================================

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Core Type Computation
    // =========================================================================

    /// Get the type of a node with a fallback.
    ///
    /// Returns the computed type, or the fallback if the computed type is ERROR.
    pub fn get_type_of_node_or(&mut self, idx: NodeIndex, fallback: TypeId) -> TypeId {
        let ty = self.get_type_of_node(idx);
        if ty == TypeId::ERROR { fallback } else { ty }
    }

    // =========================================================================
    // Type Relationship Queries
    // =========================================================================

    /// Check if a type is the error type.
    ///
    /// Returns true if the type is TypeId::ERROR.
    pub fn is_error_type(&self, ty: TypeId) -> bool {
        ty == TypeId::ERROR
    }

    /// Check if a type is the any type.
    ///
    /// Returns true if the type is TypeId::ANY.
    pub fn is_any_type(&self, ty: TypeId) -> bool {
        ty == TypeId::ANY
    }

    /// Check if a type is the unknown type.
    ///
    /// Returns true if the type is TypeId::UNKNOWN.
    pub fn is_unknown_type(&self, ty: TypeId) -> bool {
        ty == TypeId::UNKNOWN
    }

    /// Check if a type is the undefined type.
    ///
    /// Returns true if the type is TypeId::UNDEFINED.
    pub fn is_undefined_type(&self, ty: TypeId) -> bool {
        ty == TypeId::UNDEFINED
    }

    /// Check if a type is the void type.
    ///
    /// Returns true if the type is TypeId::VOID.
    pub fn is_void_type(&self, ty: TypeId) -> bool {
        ty == TypeId::VOID
    }

    /// Check if a type is the null type.
    ///
    /// Returns true if the type is TypeId::NULL.
    pub fn is_null_type(&self, ty: TypeId) -> bool {
        ty == TypeId::NULL
    }

    /// Check if a type is a nullable type (null or undefined).
    ///
    /// Returns true if the type is null or undefined.
    pub fn is_nullable_type(&self, ty: TypeId) -> bool {
        ty == TypeId::NULL || ty == TypeId::UNDEFINED
    }

    /// Check if a type is a never type.
    ///
    /// Returns true if the type is TypeId::NEVER.
    pub fn is_never_type(&self, ty: TypeId) -> bool {
        ty == TypeId::NEVER
    }

    // =========================================================================
    // Type Format Utilities
    // =========================================================================

    /// Format a type for display in error messages.
    ///
    /// This is a convenience wrapper that calls the internal format_type method.
    pub fn format_type_for_display(&self, ty: TypeId) -> String {
        self.format_type(ty)
    }

    /// Format a type for display, with optional simplification.
    ///
    /// If `simplify` is true, complex types are simplified for readability.
    pub fn format_type_simplified(&self, ty: TypeId, simplify: bool) -> String {
        // For now, just use the regular formatting
        // A future enhancement could add simplification logic
        if simplify {
            self.format_type(ty)
        } else {
            self.format_type(ty)
        }
    }

    // =========================================================================
    // Type Checking Helpers
    // =========================================================================

    /// Check if a type is assignable to another type.
    ///
    /// This is a convenience wrapper around `is_assignable_to`.
    pub fn check_is_assignable(&mut self, source: TypeId, target: TypeId) -> bool {
        self.is_assignable_to(source, target)
    }

    /// Check if a type is identical to another type.
    ///
    /// This performs a strict equality check on TypeIds.
    pub fn is_same_type(&self, ty1: TypeId, ty2: TypeId) -> bool {
        ty1 == ty2
    }

    /// Check if a type is a function type.
    ///
    /// Returns true if the type represents a callable function.
    pub fn is_function_type(&self, ty: TypeId) -> bool {
        let key = self.ctx.types.lookup(ty);
        matches!(key, Some(crate::solver::TypeKey::Callable(_)))
    }

    /// Check if a type is an object type.
    ///
    /// Returns true if the type represents an object or class.
    pub fn is_object_type(&self, ty: TypeId) -> bool {
        let key = self.ctx.types.lookup(ty);
        matches!(
            key,
            Some(crate::solver::TypeKey::Object(_) | crate::solver::TypeKey::ObjectWithIndex(_))
        )
    }

    /// Check if a type is an array type.
    ///
    /// Returns true if the type represents an array.
    pub fn is_array_type(&self, ty: TypeId) -> bool {
        // Check if it's a reference to the Array interface or an array literal
        // For now, this is a simplified check
        let type_str = self.format_type(ty);
        type_str.contains("[]") || type_str.starts_with("Array<")
    }

    /// Check if a type is a tuple type.
    ///
    /// Returns true if the type represents a tuple.
    pub fn is_tuple_type(&self, ty: TypeId) -> bool {
        let key = self.ctx.types.lookup(ty);
        matches!(key, Some(crate::solver::TypeKey::Tuple(_)))
    }

    /// Check if a type is a union type.
    ///
    /// Returns true if the type is a union of multiple types.
    pub fn is_union_type(&self, ty: TypeId) -> bool {
        let key = self.ctx.types.lookup(ty);
        matches!(key, Some(crate::solver::TypeKey::Union(_)))
    }

    /// Check if a type is an intersection type.
    ///
    /// Returns true if the type is an intersection of multiple types.
    pub fn is_intersection_type(&self, ty: TypeId) -> bool {
        let key = self.ctx.types.lookup(ty);
        matches!(key, Some(crate::solver::TypeKey::Intersection(_)))
    }

    // =========================================================================
    // Special Type Utilities
    // =========================================================================

    /// Get the element type of an array type.
    ///
    /// Returns the type of elements in the array, or ANY if not an array.
    pub fn get_array_element_type(&self, _array_ty: TypeId) -> TypeId {
        // This is a simplified implementation
        // The full version would extract the element type from array types
        TypeId::ANY
    }

    /// Get the return type of a function type.
    ///
    /// Returns the return type of a function, or ANY if not a function.
    pub fn get_function_return_type(&self, _func_ty: TypeId) -> TypeId {
        // This is a simplified implementation
        // The full version would extract the return type from callable types
        TypeId::ANY
    }
}
