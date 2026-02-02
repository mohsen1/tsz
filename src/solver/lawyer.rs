//! The "Lawyer" layer for TypeScript compatibility.
//!
//! This module implements the compatibility layer that sits between the public API
//! and the core structural subtype checking ("Judge" layer). It applies TypeScript-
//! specific business logic, including nuanced rules for `any` propagation.
//!
//! ## Judge vs. Lawyer Architecture (SOLVER.md Section 8)
//!
//! - **Judge (SubtypeChecker):** Implements strict, sound set theory semantics.
//!   It knows nothing about TypeScript legacy behavior.
//! - **Lawyer (AnyPropagationRules + CompatChecker):** Applies TypeScript-specific
//!   rules and delegates to the Judge with appropriate configuration.
//!
//! ## TypeScript Quirks Handled
//!
//! ### A. `any` Propagation (The Black Hole)
//! `any` violates the partial order of sets - it's both a subtype and supertype
//! of everything. The `AnyPropagationRules` struct handles this short-circuit.
//!
//! ### B. Function Variance
//! - **Strict mode (strictFunctionTypes):** Parameters are contravariant (sound)
//! - **Legacy mode:** Parameters are bivariant (unsound but backward-compatible)
//! - **Methods:** Always bivariant regardless of strictFunctionTypes
//!
//! ### C. Freshness (Excess Property Checking)
//! Object literals are "fresh" and trigger excess property checking.
//! Once assigned to a variable, they lose freshness and allow width subtyping.
//! Freshness is tracked on the TypeId via ObjectFlags, with object literals
//! interning to fresh shapes and widening removing the fresh flag. Sound Mode's
//! binding-level tracking lives in the Checker.
//!
//! ### D. The Void Exception
//! TypeScript allows `() => void` to match `() => T` for any T, because
//! the caller promises to ignore the return value.
//!
//! ### E. Weak Type Detection (TS2559)
//! Types with only optional properties require at least one common property
//! with the source type to prevent accidental assignment mistakes.
//!
//! The key principle is that `any` should NOT silence structural mismatches.
//! While `any` is TypeScript's escape hatch, we still want to catch real errors
//! even when `any` is involved.

use crate::solver::TypeDatabase;
use crate::solver::types::{TypeId, TypeKey};

/// Rules for `any` propagation in type checking.
///
/// In TypeScript, `any` is both a top type (everything is assignable to `any`)
/// and a bottom type (`any` is assignable to everything). However, `any` should
/// not be used to silence real structural mismatches.
///
/// This struct encapsulates the nuanced rules for when `any` is allowed to
/// suppress type errors and when it isn't.
pub struct AnyPropagationRules {
    /// Whether to allow `any` to silence structural mismatches.
    /// When false, `any` is treated more strictly and structural errors
    /// are still reported even when `any` is involved.
    pub(crate) allow_any_suppression: bool,
}

impl AnyPropagationRules {
    /// Create a new `AnyPropagationRules` with default settings.
    ///
    /// By default, `any` suppression is enabled for backward compatibility
    /// with existing TypeScript behavior.
    pub fn new() -> Self {
        AnyPropagationRules {
            allow_any_suppression: true,
        }
    }

    /// Create strict `AnyPropagationRules` where `any` does not silence
    /// structural mismatches.
    ///
    /// In strict mode, even when `any` is involved, the type checker will
    /// perform structural checking and report mismatches.
    pub fn strict() -> Self {
        AnyPropagationRules {
            allow_any_suppression: false,
        }
    }

    /// Set whether `any` is allowed to suppress structural mismatches.
    pub fn set_allow_any_suppression(&mut self, allow: bool) {
        self.allow_any_suppression = allow;
    }

    /// Check if `any` is allowed to suppress a type mismatch between
    /// `source` and `target`.
    ///
    /// This function implements the nuanced rules for when `any` should
    /// and should not silence errors.
    ///
    /// ## Rule: Any should NOT silence structural mismatches
    ///
    /// The key insight is that `any` is TypeScript's escape hatch, but we
    /// still want to catch real errors. This function determines if a
    /// specific case should allow `any` to suppress the error.
    ///
    /// ### Cases where `any` CAN suppress:
    /// - Direct assignment: `let x: any = someValue`
    /// - Direct assignment from `any`: `let x: SomeType = anyValue`
    /// - When explicitly opted-in via compiler flags
    ///
    /// ### Cases where `any` CANNOT suppress:
    /// - Property access mismatches on objects with `any` properties
    /// - Function call arguments when parameter is `any` but structural
    ///   mismatch exists in other arguments
    /// - Array/tuple element mismatches when container has `any` elements
    pub fn is_any_allowed_to_suppress(
        &self,
        source: TypeId,
        target: TypeId,
        interner: &dyn TypeDatabase,
    ) -> bool {
        // If suppression is globally disabled, `any` never suppresses
        if !self.allow_any_suppression {
            return false;
        }

        // Fast path: neither type is `any`
        let source_is_any = source == TypeId::ANY;
        let target_is_any = target == TypeId::ANY;
        if !source_is_any && !target_is_any {
            return false;
        }

        // At this point, at least one of the types is `any`
        // Now we need to check if this is a case where we should
        // allow suppression or if there's a structural mismatch we
        // want to catch anyway

        // Case 1: Direct assignment to/from `any` - allow suppression
        // This is the standard TypeScript behavior and is expected
        if source_is_any || target_is_any {
            // Check if there's a non-trivial structure that might indicate
            // a real error we want to catch despite the `any`
            if self.has_structural_mismatch_despite_any(source, target, interner) {
                // There's a structural mismatch we should report
                return false;
            }
            // Direct `any` involvement is OK
            return true;
        }

        false
    }

    /// Check if there's a structural mismatch that should be reported
    /// even though `any` is involved.
    ///
    /// This implements the core logic of "Any should NOT silence
    /// structural mismatches."
    ///
    /// In default mode (legacy TypeScript behavior), `any` is allowed
    /// to suppress most errors. In strict mode, `any` does not suppress
    /// structural errors for complex types.
    fn has_structural_mismatch_despite_any(
        &self,
        source: TypeId,
        target: TypeId,
        interner: &dyn TypeDatabase,
    ) -> bool {
        // If both are `any`, there's no mismatch to report
        if source == TypeId::ANY && target == TypeId::ANY {
            return false;
        }

        // In non-strict mode, we allow `any` to suppress errors
        // (legacy TypeScript behavior)
        if self.allow_any_suppression {
            return false;
        }

        // In strict mode, check if the non-`any` type has interesting structure
        // that we should validate
        let non_any_type = if source == TypeId::ANY {
            target
        } else {
            source
        };

        // Look at the structure of the non-`any` type
        match interner.lookup(non_any_type) {
            Some(TypeKey::Object(shape_id)) => {
                let shape = interner.object_shape(shape_id);
                // Objects with properties should be validated even with `any`
                !shape.properties.is_empty()
            }
            Some(TypeKey::ObjectWithIndex(shape_id)) => {
                let shape = interner.object_shape(shape_id);
                // Objects with index signatures might still have structure to check
                !shape.properties.is_empty()
                    || shape.string_index.is_some()
                    || shape.number_index.is_some()
            }
            Some(TypeKey::Array(_)) => {
                // Arrays have structure (element type) that matters
                true
            }
            Some(TypeKey::Tuple(_)) => {
                // Tuples have significant structure
                true
            }
            Some(TypeKey::Function(_)) | Some(TypeKey::Callable(_)) => {
                // Functions have signatures that matter
                true
            }
            _ => false,
        }
    }

    /// Get the subtype check result considering `any` propagation rules.
    ///
    /// This is the main entry point that decides whether to:
    /// 1. Allow `any` to suppress the check (return true)
    /// 2. Delegate to the Judge for structural checking
    ///
    /// Returns `Some(result)` if the `any` rules decide the outcome,
    /// or `None` if the check should be delegated to the structural checker.
    pub fn check_any_propagation(
        &self,
        source: TypeId,
        target: TypeId,
        interner: &dyn TypeDatabase,
    ) -> Option<bool> {
        // Check if either type is `any`
        let source_is_any = source == TypeId::ANY;
        let target_is_any = target == TypeId::ANY;

        if !source_is_any && !target_is_any {
            // No `any` involved - delegate to structural checker
            return None;
        }

        // `any` is involved - check if suppression is allowed
        if self.is_any_allowed_to_suppress(source, target, interner) {
            Some(true)
        } else {
            // `any` is present but shouldn't suppress - delegate to structural checker
            None
        }
    }
}

impl Default for AnyPropagationRules {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// TypeScript Quirks Summary
// =============================================================================

/// Summary of TypeScript quirks handled by the Lawyer layer.
///
/// This struct provides documentation and helper methods for understanding
/// and configuring the various TypeScript compatibility behaviors.
pub struct TypeScriptQuirks;

impl TypeScriptQuirks {
    /// List of all TypeScript quirks handled by the Lawyer layer.
    pub const QUIRKS: &'static [(&'static str, &'static str)] = &[
        (
            "any-propagation",
            "any is both top and bottom type (assignable to/from everything)",
        ),
        (
            "function-bivariance",
            "Function parameters are bivariant in legacy mode",
        ),
        (
            "method-bivariance",
            "Methods are always bivariant regardless of strictFunctionTypes",
        ),
        ("void-return", "() => void accepts () => T for any T"),
        (
            "weak-types",
            "Objects with only optional properties require common properties (TS2559)",
        ),
        (
            "freshness",
            "Object literals trigger excess property checking",
        ),
        (
            "empty-object",
            "{} accepts any non-nullish value including primitives",
        ),
        (
            "null-undefined",
            "null and undefined are assignable to everything without strictNullChecks",
        ),
        (
            "bivariant-rest",
            "Rest parameters of any/unknown are treated as bivariant",
        ),
    ];
}

#[cfg(test)]
#[path = "tests/lawyer_tests.rs"]
mod tests;
