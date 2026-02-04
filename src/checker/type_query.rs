//! Type Query Utilities Module
//!
//! This module contains type query utility methods for CheckerState
//! as part of Phase 2 architecture refactoring.
//!
//! The methods in this module handle:
//! - Typeof type query utilities
//! - Keyof type query utilities
//! - Type name resolution helpers
//!
//! This module extends CheckerState with utilities for type query
//! operations, providing cleaner APIs for typeof and keyof operations.

use crate::checker::state::CheckerState;
use crate::parser::NodeIndex;
use crate::solver::TypeId;
use crate::solver::type_queries::{LiteralTypeKind, classify_literal_type};

// =============================================================================
// Type Query Utilities
// =============================================================================

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Typeof Query Utilities
    // =========================================================================

    /// Check if a node represents a typeof type query.
    ///
    /// Returns true if the node is a TypeQuery node (typeof expression).
    pub fn is_typeof_query(&self, idx: NodeIndex) -> bool {
        use crate::parser::syntax_kind_ext::TYPE_QUERY;

        self.ctx
            .arena
            .get(idx)
            .map(|node| node.kind == TYPE_QUERY as u16)
            .unwrap_or(false)
    }

    /// Get the typeof type name for a given type.
    ///
    /// Returns the primitive type name that the typeof operator would return:
    /// - "undefined", "object", "boolean", "number", "bigint", "string", "symbol", "function"
    pub fn get_typeof_type_name_for_type(&self, type_id: TypeId) -> String {
        // First check TypeId constants
        if type_id == TypeId::UNDEFINED {
            return "undefined".to_string();
        }
        if type_id == TypeId::NULL {
            return "object".to_string();
        }
        if type_id == TypeId::BOOLEAN {
            return "boolean".to_string();
        }
        if type_id == TypeId::NUMBER {
            return "number".to_string();
        }
        if type_id == TypeId::STRING {
            return "string".to_string();
        }

        // Check for literal types
        match classify_literal_type(self.ctx.types, type_id) {
            LiteralTypeKind::String(_) => "string".to_string(),
            LiteralTypeKind::Number(_) => "number".to_string(),
            LiteralTypeKind::Boolean(_) => "boolean".to_string(),
            LiteralTypeKind::BigInt(_) => "bigint".to_string(),
            LiteralTypeKind::NotLiteral => {
                // Check for function types
                if self.is_callable_type(type_id) {
                    "function".to_string()
                } else if self.is_object_type(type_id) {
                    "object".to_string()
                } else if type_id == TypeId::ANY || type_id == TypeId::UNKNOWN {
                    "object".to_string() // Conservative fallback
                } else {
                    "object".to_string()
                }
            }
        }
    }

    // =========================================================================
    // Type Name Resolution
    // =========================================================================

    /// Get the display name of a type.
    ///
    /// Returns a human-readable name for the type, suitable for
    /// error messages and diagnostic output.
    pub fn get_type_display_name(&self, type_id: TypeId) -> String {
        self.format_type(type_id)
    }

    /// Check if a type is a primitive type.
    ///
    /// Returns true for: undefined, null, boolean, number, string.
    pub fn is_primitive_type(&self, type_id: TypeId) -> bool {
        type_id == TypeId::UNDEFINED
            || type_id == TypeId::NULL
            || type_id == TypeId::BOOLEAN
            || type_id == TypeId::NUMBER
            || type_id == TypeId::STRING
    }

    // =========================================================================
    // Type Query Helpers
    // =========================================================================

    /// Create a string literal type from a string value.
    ///
    /// This is a convenience helper for creating string literal types,
    /// commonly used in typeof and keyof operations.
    pub fn string_literal_type(&self, value: &str) -> TypeId {
        self.ctx.types.literal_string(value)
    }

    /// Get the typeof result as a type (string literal type).
    ///
    /// For a given type, returns the typeof result as a string literal type.
    /// For example, if the input type is `number`, returns the string literal type `"number"`.
    pub fn typeof_as_type(&self, type_id: TypeId) -> TypeId {
        let type_name = self.get_typeof_type_name_for_type(type_id);
        self.string_literal_type(&type_name)
    }
}
