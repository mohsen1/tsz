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
use crate::solver::{LiteralValue, OrderedFloat, TypeId, TypeKey};

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
        matches!(
            self.ctx.types.lookup(type_id),
            Some(TypeKey::Literal(LiteralValue::String(_)))
        )
    }

    /// Check if a type is a number literal type.
    ///
    /// Returns true for types like `0`, `1`, `42`, `3.14`, etc.
    pub fn is_number_literal_type(&self, type_id: TypeId) -> bool {
        matches!(
            self.ctx.types.lookup(type_id),
            Some(TypeKey::Literal(LiteralValue::Number(_)))
        )
    }

    /// Check if a type is a boolean literal type.
    ///
    /// Returns true for types `true` or `false`.
    pub fn is_boolean_literal_type(&self, type_id: TypeId) -> bool {
        matches!(
            self.ctx.types.lookup(type_id),
            Some(TypeKey::Literal(LiteralValue::Boolean(_)))
        )
    }

    // Note: `is_literal_type` is defined in type_computation.rs

    // =========================================================================
    // Literal Type Creation
    // =========================================================================

    /// Create a string literal type from a string value.
    ///
    /// Creates a type representing a specific string literal.
    pub fn create_string_literal(&self, value: &str) -> TypeId {
        let atom = self.ctx.types.intern_string(value);
        self.ctx
            .types
            .intern(TypeKey::Literal(LiteralValue::String(atom)))
    }

    /// Create a number literal type from a numeric value.
    ///
    /// Creates a type representing a specific number literal.
    pub fn create_number_literal(&self, value: f64) -> TypeId {
        self.ctx
            .types
            .intern(TypeKey::Literal(LiteralValue::Number(OrderedFloat(value))))
    }

    /// Create a boolean literal type.
    ///
    /// Creates the type for `true` or `false`.
    pub fn create_boolean_literal(&self, value: bool) -> TypeId {
        self.ctx
            .types
            .intern(TypeKey::Literal(LiteralValue::Boolean(value)))
    }

    // =========================================================================
    // Literal Type Value Extraction
    // =========================================================================

    /// Get the string value from a string literal type.
    ///
    /// Returns the string value if the type is a string literal,
    /// or None otherwise.
    pub fn get_string_literal_value(&self, type_id: TypeId) -> Option<String> {
        match self.ctx.types.lookup(type_id) {
            Some(TypeKey::Literal(LiteralValue::String(atom))) => {
                Some(self.ctx.types.resolve_atom_ref(atom).to_string())
            }
            _ => None,
        }
    }

    /// Get the numeric value from a number literal type.
    ///
    /// Returns the number value if the type is a number literal,
    /// or None otherwise.
    pub fn get_number_literal_value(&self, type_id: TypeId) -> Option<f64> {
        match self.ctx.types.lookup(type_id) {
            Some(TypeKey::Literal(LiteralValue::Number(value))) => Some(value.0),
            _ => None,
        }
    }

    /// Get the boolean value from a boolean literal type.
    ///
    /// Returns the boolean value if the type is a boolean literal,
    /// or None otherwise.
    pub fn get_boolean_literal_value(&self, type_id: TypeId) -> Option<bool> {
        match self.ctx.types.lookup(type_id) {
            Some(TypeKey::Literal(LiteralValue::Boolean(value))) => Some(value),
            _ => None,
        }
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

        // Check if they're the same kind of literal
        match (self.ctx.types.lookup(type1), self.ctx.types.lookup(type2)) {
            (
                Some(TypeKey::Literal(LiteralValue::String(a1))),
                Some(TypeKey::Literal(LiteralValue::String(a2))),
            ) => a1 == a2,
            (
                Some(TypeKey::Literal(LiteralValue::Number(n1))),
                Some(TypeKey::Literal(LiteralValue::Number(n2))),
            ) => n1 == n2,
            (
                Some(TypeKey::Literal(LiteralValue::Boolean(b1))),
                Some(TypeKey::Literal(LiteralValue::Boolean(b2))),
            ) => b1 == b2,
            _ => false,
        }
    }
}
