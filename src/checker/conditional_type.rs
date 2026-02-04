//! Conditional Type Utilities Module
//!
//! This module contains conditional type utility methods for CheckerState
//! as part of Phase 2 architecture refactoring.
//!
//! The methods in this module handle:
//! - Conditional type detection and validation
//! - Conditional type component extraction (check, extends, true, false branches)
//! - Distributive conditional type handling
//! - Conditional type evaluation helpers
//!
//! This module extends CheckerState with utilities for conditional type
//! operations, providing cleaner APIs for conditional type checking.

use crate::checker::state::CheckerState;
use crate::solver::TypeId;

// =============================================================================
// Conditional Type Utilities
// =============================================================================

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Conditional Type Component Extraction
    // =========================================================================

    /// Get the check type from a conditional type.
    ///
    /// Returns the `T` in `T extends U ? X : Y`, or None if not a conditional.
    pub fn get_conditional_check_type(&self, type_id: TypeId) -> Option<TypeId> {
        crate::solver::type_queries::get_conditional_type(self.ctx.types, type_id)
            .map(|cond| cond.check_type)
    }

    /// Get the extends type from a conditional type.
    ///
    /// Returns the `U` in `T extends U ? X : Y`, or None if not a conditional.
    pub fn get_conditional_extends_type(&self, type_id: TypeId) -> Option<TypeId> {
        crate::solver::type_queries::get_conditional_type(self.ctx.types, type_id)
            .map(|cond| cond.extends_type)
    }

    /// Get the true branch type from a conditional type.
    ///
    /// Returns the `X` in `T extends U ? X : Y`, or None if not a conditional.
    pub fn get_conditional_true_type(&self, type_id: TypeId) -> Option<TypeId> {
        crate::solver::type_queries::get_conditional_type(self.ctx.types, type_id)
            .map(|cond| cond.true_type)
    }

    /// Get the false branch type from a conditional type.
    ///
    /// Returns the `Y` in `T extends U ? X : Y`, or None if not a conditional.
    pub fn get_conditional_false_type(&self, type_id: TypeId) -> Option<TypeId> {
        crate::solver::type_queries::get_conditional_type(self.ctx.types, type_id)
            .map(|cond| cond.false_type)
    }

    // =========================================================================
    // Conditional Type Properties
    // =========================================================================

    /// Check if a conditional type is distributive.
    ///
    /// Distributive conditionals automatically distribute over unions:
    /// `(A | B) extends C ? X : Y` becomes `(A extends C ? X : Y) | (B extends C ? X : Y)`
    pub fn is_distributive_conditional(&self, type_id: TypeId) -> bool {
        crate::solver::type_queries::get_conditional_type(self.ctx.types, type_id)
            .map(|cond| cond.is_distributive)
            .unwrap_or(false)
    }

    // =========================================================================
    // Conditional Type Evaluation Helpers
    // =========================================================================

    /// Get both branch types from a conditional type.
    ///
    /// Returns (true_type, false_type) if this is a conditional, or None otherwise.
    pub fn get_conditional_branches(&self, type_id: TypeId) -> Option<(TypeId, TypeId)> {
        crate::solver::type_queries::get_conditional_type(self.ctx.types, type_id)
            .map(|cond| (cond.true_type, cond.false_type))
    }

    /// Get the check and extends types from a conditional type.
    ///
    /// Returns (check_type, extends_type) if this is a conditional, or None otherwise.
    pub fn get_conditional_check(&self, type_id: TypeId) -> Option<(TypeId, TypeId)> {
        crate::solver::type_queries::get_conditional_type(self.ctx.types, type_id)
            .map(|cond| (cond.check_type, cond.extends_type))
    }

    // =========================================================================
    // Conditional Type Analysis
    // =========================================================================

    /// Check if a conditional type depends on a type parameter.
    ///
    /// Returns true if the check type or extends type contains the given type parameter.
    pub fn conditional_depends_on(&self, type_id: TypeId, type_param: TypeId) -> bool {
        if let (Some(check_type), Some(extends_type)) = (
            self.get_conditional_check_type(type_id),
            self.get_conditional_extends_type(type_id),
        ) {
            check_type == type_param || extends_type == type_param
        } else {
            false
        }
    }

    /// Check if a conditional type is a type guard predicate.
    ///
    /// Type guards are conditionals that return true/false for type narrowing,
    /// like `T is string` or `T extends any ? T : never`.
    pub fn is_type_guard_conditional(&self, type_id: TypeId) -> bool {
        if let (Some(true_type), Some(false_type)) = (
            self.get_conditional_true_type(type_id),
            self.get_conditional_false_type(type_id),
        ) {
            // Common type guard patterns:
            // - T is U ? X : Y (explicit type guard)
            // - T extends any ? T : never (identity on true)
            true_type == TypeId::ANY || false_type == TypeId::NEVER
        } else {
            false
        }
    }

    /// Check if a conditional type is a never-to-never transformation.
    ///
    /// Returns true if both branches return never (eliminates the type).
    pub fn is_never_conditional(&self, type_id: TypeId) -> bool {
        if let (Some(true_type), Some(false_type)) = (
            self.get_conditional_true_type(type_id),
            self.get_conditional_false_type(type_id),
        ) {
            true_type == TypeId::NEVER && false_type == TypeId::NEVER
        } else {
            false
        }
    }

    /// Check if a conditional type is an identity transformation.
    ///
    /// Returns true if the conditional returns the check type unchanged
    /// (e.g., `T extends any ? T : never` or similar patterns).
    pub fn is_identity_conditional(&self, type_id: TypeId) -> bool {
        if let (Some(check_type), Some(true_type), Some(false_type)) = (
            self.get_conditional_check_type(type_id),
            self.get_conditional_true_type(type_id),
            self.get_conditional_false_type(type_id),
        ) {
            // Identity: returns T on true, never on false
            check_type == true_type && false_type == TypeId::NEVER
        } else {
            false
        }
    }

    /// Get the result type of evaluating a conditional type.
    ///
    /// This is a helper that attempts to resolve the conditional
    /// by checking if check_type extends extends_type.
    /// Returns the appropriate branch type if resolvable, or None otherwise.
    pub fn evaluate_conditional(&mut self, type_id: TypeId) -> Option<TypeId> {
        if let (Some(check_type), Some(extends_type), Some(true_type), Some(false_type)) = (
            self.get_conditional_check_type(type_id),
            self.get_conditional_extends_type(type_id),
            self.get_conditional_true_type(type_id),
            self.get_conditional_false_type(type_id),
        ) {
            // Check if check_type extends extends_type
            if self.is_assignable_to(check_type, extends_type) {
                Some(true_type)
            } else {
                Some(false_type)
            }
        } else {
            None
        }
    }
}
