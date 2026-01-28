//! Enum Type Checking Utilities Module
//!
//! This module contains enum type checking utility methods for CheckerState
//! as part of Phase 2 architecture refactoring.
//!
//! The methods in this module handle:
//! - Enum type detection and validation
//! - Enum member type checking
//! - Enum assignability rules
//! - Const enum vs regular enum handling
//!
//! This module extends CheckerState with utilities for enum-related
//! type checking operations.

use crate::binder::symbol_flags;
use crate::checker::state::CheckerState;
use crate::solver::TypeId;

// =============================================================================
// Enum Type Checking Utilities
// =============================================================================

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Enum Type Detection
    // =========================================================================

    /// Check if a type is an enum type.
    ///
    /// Returns true if the type represents a TypeScript enum.
    pub fn is_enum_type(&self, type_id: TypeId) -> bool {
        if let Some(sym_ref) = crate::solver::type_queries::get_ref_symbol(self.ctx.types, type_id)
        {
            if let Some(symbol) = self
                .ctx
                .binder
                .get_symbol(crate::binder::SymbolId(sym_ref.0))
            {
                (symbol.flags & symbol_flags::ENUM) != 0
            } else {
                false
            }
        } else {
            false
        }
    }

    /// Check if a type is a const enum type.
    ///
    /// Const enums are fully inlined and cannot be accessed at runtime.
    pub fn is_const_enum_type(&self, type_id: TypeId) -> bool {
        if let Some(sym_ref) = crate::solver::type_queries::get_ref_symbol(self.ctx.types, type_id)
        {
            if let Some(symbol) = self
                .ctx
                .binder
                .get_symbol(crate::binder::SymbolId(sym_ref.0))
            {
                (symbol.flags & symbol_flags::ENUM) != 0
                    && (symbol.flags & symbol_flags::CONST_ENUM) != 0
            } else {
                false
            }
        } else {
            false
        }
    }

    /// Check if a type is a regular (non-const) enum type.
    pub fn is_regular_enum_type(&self, type_id: TypeId) -> bool {
        self.is_enum_type(type_id) && !self.is_const_enum_type(type_id)
    }

    // =========================================================================
    // Enum Member Type Utilities
    // =========================================================================

    /// Check if a type could be an enum member type.
    ///
    /// Enum members can be:
    /// - String literals (for string enums)
    /// - Numeric literals (for numeric enums)
    /// - Computed values (for heterogeneous enums)
    pub fn is_enum_member_type(&self, type_id: TypeId) -> bool {
        use crate::solver::type_queries::LiteralTypeKind;

        // Check for string or number literals
        match crate::solver::type_queries::classify_literal_type(self.ctx.types, type_id) {
            LiteralTypeKind::String(_) | LiteralTypeKind::Number(_) => return true,
            _ => {}
        }

        // Check for union type
        if crate::solver::type_queries::is_union_type(self.ctx.types, type_id) {
            return true;
        }

        // Check for primitive types
        type_id == TypeId::STRING || type_id == TypeId::NUMBER
    }

    /// Get the base member type for an enum (string or number).
    ///
    /// Returns:
    /// - STRING for string enums
    /// - NUMBER for numeric enums
    /// - UNION for heterogeneous enums
    /// - UNKNOWN if the enum kind cannot be determined
    pub fn get_enum_member_base_type(&self, type_id: TypeId) -> TypeId {
        // If this is already a primitive type, return it
        if type_id == TypeId::STRING || type_id == TypeId::NUMBER {
            return type_id;
        }

        // Check if it's a union - indicates heterogeneous enum
        if crate::solver::type_queries::is_union_type(self.ctx.types, type_id) {
            return type_id; // Return the union as-is
        }

        // Check if it's a Ref type
        if crate::solver::type_queries::get_ref_symbol(self.ctx.types, type_id).is_some() {
            // For enum types, we need to check the member types
            // Default to STRING for string enums, NUMBER for numeric enums
            return TypeId::UNKNOWN;
        }

        TypeId::UNKNOWN
    }

    // =========================================================================
    // Enum Assignability
    // =========================================================================

    /// Check if enum member types are compatible.
    ///
    /// TypeScript allows enum members to be compared if they are from
    /// compatible enum types (string enum members are string-compatible,
    /// number enum members are number-compatible).
    pub fn enum_members_compatible(&self, enum_type1: TypeId, enum_type2: TypeId) -> bool {
        // If both are the same enum type, they're compatible
        if enum_type1 == enum_type2 {
            return true;
        }

        // Check if both are string enums or both are number enums
        let base1 = self.get_enum_member_base_type(enum_type1);
        let base2 = self.get_enum_member_base_type(enum_type2);

        (base1 == TypeId::STRING && base2 == TypeId::STRING)
            || (base1 == TypeId::NUMBER && base2 == TypeId::NUMBER)
    }

    /// Check if an enum type is assignable to another type.
    ///
    /// Enums are assignable to:
    /// - Their exact enum type
    /// - The primitive type (string/number) for literal enum members
    pub fn is_enum_assignable_to(&mut self, enum_type: TypeId, target_type: TypeId) -> bool {
        // Exact type match
        if enum_type == target_type {
            return true;
        }

        // Check if target is the base primitive type
        let base_type = self.get_enum_member_base_type(enum_type);
        if base_type == target_type {
            return true;
        }

        // Use subtype checking for more complex cases
        self.is_assignable_to(enum_type, target_type)
    }

    // =========================================================================
    // Enum Expression Utilities
    // =========================================================================

    /// Check if an enum access is allowed (not a const enum).
    ///
    /// Const enums cannot be accessed as values at runtime - they are fully inlined.
    /// This check prevents runtime errors from accessing const enum members.
    pub fn is_enum_access_allowed(&self, enum_type: TypeId) -> bool {
        !self.is_const_enum_type(enum_type)
    }

    /// Get the type of an enum member access.
    ///
    /// For enum members, this returns the literal type of the member
    /// (e.g., "Red" for `Color.Red` in a string enum).
    pub fn get_enum_member_access_type(&self, enum_type: TypeId) -> TypeId {
        self.get_enum_member_base_type(enum_type)
    }
}
