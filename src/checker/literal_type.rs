//! Literal Type Utilities Module
//!
//! This module contains literal type utility methods for CheckerState
//! as part of Phase 2 architecture refactoring.
//!
//! The methods in this module handle:
//! - Literal type detection (string, number, boolean literals)
//! - Literal type creation and manipulation
//! - Literal type widening (const vs let)
//! - Template literal type handling
//!
//! This module extends CheckerState with utilities for literal type
//! operations, providing cleaner APIs for literal type checking.

use crate::checker::state::CheckerState;
use crate::solver::TypeId;
use crate::solver::type_queries::{
    LiteralTypeKind, classify_literal_type, create_boolean_literal_type,
    create_number_literal_type, create_string_literal_type,
    get_boolean_literal_value as solver_get_boolean_literal_value,
    get_number_literal_value as solver_get_number_literal_value, get_string_literal_atom,
    is_boolean_literal, is_number_literal, is_string_literal,
};

// =============================================================================
// Literal Type Utilities
// =============================================================================

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Literal Type Detection
    // =========================================================================

    /// Check if a type is a string literal type.
    ///
    /// Returns true for types like `"hello"`, `"world"`, etc.
    pub fn is_string_literal_type(&self, type_id: TypeId) -> bool {
        is_string_literal(self.ctx.types, type_id)
    }

    /// Check if a type is a number literal type.
    ///
    /// Returns true for types like `0`, `1`, `42`, `3.14`, etc.
    pub fn is_number_literal_type(&self, type_id: TypeId) -> bool {
        is_number_literal(self.ctx.types, type_id)
    }

    /// Check if a type is a boolean literal type.
    ///
    /// Returns true for types `true` or `false`.
    pub fn is_boolean_literal_type(&self, type_id: TypeId) -> bool {
        is_boolean_literal(self.ctx.types, type_id)
    }

    // Note: `is_literal_type` is defined in type_computation.rs

    // =========================================================================
    // Literal Type Creation
    // =========================================================================

    /// Create a string literal type from a string value.
    ///
    /// Creates a type representing a specific string literal.
    pub fn create_string_literal(&self, value: &str) -> TypeId {
        create_string_literal_type(self.ctx.types, value)
    }

    /// Create a number literal type from a numeric value.
    ///
    /// Creates a type representing a specific number literal.
    pub fn create_number_literal(&self, value: f64) -> TypeId {
        create_number_literal_type(self.ctx.types, value)
    }

    /// Create a boolean literal type.
    ///
    /// Creates the type for `true` or `false`.
    pub fn create_boolean_literal(&self, value: bool) -> TypeId {
        create_boolean_literal_type(self.ctx.types, value)
    }

    // =========================================================================
    // Literal Type Value Extraction
    // =========================================================================

    /// Get the string value from a string literal type.
    ///
    /// Returns the string value if the type is a string literal,
    /// or None otherwise.
    pub fn get_string_literal_value(&self, type_id: TypeId) -> Option<String> {
        get_string_literal_atom(self.ctx.types, type_id)
            .map(|atom| self.ctx.types.resolve_atom_ref(atom).to_string())
    }

    /// Get the numeric value from a number literal type.
    ///
    /// Returns the number value if the type is a number literal,
    /// or None otherwise.
    pub fn get_number_literal_value(&self, type_id: TypeId) -> Option<f64> {
        solver_get_number_literal_value(self.ctx.types, type_id)
    }

    /// Get the boolean value from a boolean literal type.
    ///
    /// Returns the boolean value if the type is a boolean literal,
    /// or None otherwise.
    pub fn get_boolean_literal_value(&self, type_id: TypeId) -> Option<bool> {
        solver_get_boolean_literal_value(self.ctx.types, type_id)
    }

    // Note: `widen_literal_type` is defined in state.rs

    // =========================================================================
    // Literal Type Comparison
    // =========================================================================

    /// Check if two literal types represent the same value.
    ///
    /// For example, `"hello"` and `"hello"` return true,
    /// but `"hello"` and `"world"` return false.
    pub fn literal_types_equal(&self, type1: TypeId, type2: TypeId) -> bool {
        // Same TypeId means they're the same type
        if type1 == type2 {
            return true;
        }

        // Both must be literal types
        if !self.is_literal_type(type1) || !self.is_literal_type(type2) {
            return false;
        }

        // Check if they're the same kind of literal using the classification helper
        match (
            classify_literal_type(self.ctx.types, type1),
            classify_literal_type(self.ctx.types, type2),
        ) {
            (LiteralTypeKind::String(a1), LiteralTypeKind::String(a2)) => a1 == a2,
            (LiteralTypeKind::Number(n1), LiteralTypeKind::Number(n2)) => {
                // Compare using bits for proper f64 equality (handles NaN, -0.0, etc.)
                n1.to_bits() == n2.to_bits()
            }
            (LiteralTypeKind::Boolean(b1), LiteralTypeKind::Boolean(b2)) => b1 == b2,
            (LiteralTypeKind::BigInt(a1), LiteralTypeKind::BigInt(a2)) => a1 == a2,
            _ => false,
        }
    }
}
