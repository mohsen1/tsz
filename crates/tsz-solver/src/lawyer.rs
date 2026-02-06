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
//! ### F. Nominality Overrides (The "Brand" Check)
//!
//! TypeScript is primarily structurally typed, but has specific exceptions where
//! nominality is enforced. These are "escape hatches" from structural subtyping
//! that prevent unsound or surprising assignments.
//!
//! #### F.1. Enum Nominality (TS2322)
//! Enum members are nominally typed, not structurally.
//!
//! **Rule**: `EnumA.Member1` is NOT assignable to `EnumB.Member2` even if both
//! have the same underlying value (e.g., both are `0`).
//!
//! **Implementation**:
//! - Enum members are wrapped in `TypeKey::Enum(def_id, literal_type)`
//! - The `def_id` provides nominal identity (which enum)
//! - The `literal_type` preserves the value (for assignability checks)
//! - `enum_assignability_override` in `CompatChecker` enforces this rule
//!
//! **Examples**:
//! ```typescript
//! enum E { A = 0, B = 1 }
//! enum F { A = 0, B = 1 }
//!
//! let x: E.A = E.B;        // ❌ TS2322: different members
//! let y: E.A = F.A;        // ❌ TS2322: different enums
//! let z: E.A = 0;          // ✅ OK: numeric enum to number
//! let w: number = E.A;     // ✅ OK: numeric enum to number
//! ```
//!
//! #### F.2. Private/Protected Brands (TS2322)
//! Classes with private/protected members behave nominally, not structurally.
//!
//! **Rule**: Two classes with the same private member signature are NOT compatible
//! unless they share the same declaration (or one extends the other).
//!
//! **Rationale**: Private members create a "brand" that distinguishes otherwise
//! structurally identical types. This prevents accidentally mixing objects that
//! happen to have the same shape but represent different concepts.
//!
//! **Implementation**:
//! - `private_brand_assignability_override` in `CompatChecker`
//! - Uses `SymbolId` comparison to verify private members originate from same declaration
//! - Subclasses inherit the parent's private brand (are compatible)
//! - Public members remain structural (do not create brands)
//!
//! **Examples**:
//! ```typescript
//! class A { private x: number = 1; }
//! class B { private x: number = 1; }
//!
//! let a: A = new B();        // ❌ TS2322: separate private declarations
//! let b: B = new A();        // ❌ TS2322: separate private declarations
//!
//! class C extends A {}
//! let c: A = new C();        // ✅ OK: subclass inherits brand
//! ```
//!
//! #### F.3. Constructor Accessibility (TS2673, TS2674)
//! Classes with private/protected constructors cannot be instantiated from
//! invalid scopes.
//!
//! **Rule**:
//! - `private constructor()`: Only accessible within the class declaration
//! - `protected constructor()`: Only accessible within the class or subclasses
//! - `public constructor()` or no modifier: Accessible everywhere (default)
//!
//! **Implementation**:
//! - `constructor_accessibility_override` in `CompatChecker`
//! - Checks constructor symbol flags when assigning class type to constructable
//! - Validates scope (inside class, subclass, or external)
//!
//! **Examples**:
//! ```typescript
//! class A { private constructor() {} }
//! let a = new A();           // ❌ TS2673: private constructor
//! A.staticCreate();          // ✅ OK: inside class
//!
//! class B { protected constructor() {} }
//! class C extends B { constructor() { super(); } }
//! let b = new B();           // ❌ TS2674: protected constructor
//! let c = new C();           // ✅ OK: subclass access
//! ```
//!
//! ### Why These Override The Judge
//!
//! The **Judge** (SubtypeChecker) implements sound, structural set theory semantics.
//! It would correctly determine that `class A { private x }` and `class B { private x }`
//! have the same shape and are structurally compatible.
//!
//! The **Lawyer** (CompatChecker) steps in and says "Wait, TypeScript says these
//! are incompatible because of the private brand." This is TypeScript-specific
//! legacy behavior that violates soundness principles for practical/ergonomic reasons.
//!
//! **Key Principle**: The Lawyer never makes types MORE compatible. It only
//! makes them LESS compatible by adding restrictions on top of the Judge's
//! structural analysis.
//!
//! The key principle is that `any` should NOT silence structural mismatches.
//! While `any` is TypeScript's escape hatch, we still want to catch real errors
//! even when `any` is involved.

use crate::AnyPropagationMode;

/// Rules for `any` propagation in type checking.
///
/// In TypeScript, `any` is both a top type (everything is assignable to `any`)
/// and a bottom type (`any` is assignable to everything). This struct captures
/// whether `any` is allowed to suppress nested structural mismatches by
/// configuring the subtype engine's propagation mode.
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

    /// Return the propagation mode for `any` handling in the subtype engine.
    pub fn any_propagation_mode(&self) -> AnyPropagationMode {
        if self.allow_any_suppression {
            AnyPropagationMode::All
        } else {
            AnyPropagationMode::TopLevelOnly
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
