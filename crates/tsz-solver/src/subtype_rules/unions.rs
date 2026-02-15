//! Union and intersection type subtype checking.
//!
//! This module handles subtyping for TypeScript's composite types:
//! - Union types (A | B | C) - source must be subtype of at least one member
//! - Intersection types (A & B & C) - source must be subtype of all members
//! - Distributivity rules between unions and intersections
//! - Type parameter compatibility in union/intersection contexts

use crate::types::*;
use crate::visitor::type_param_info;

use super::super::{SubtypeChecker, SubtypeFailureReason, SubtypeResult, TypeResolver};

impl<'a, R: TypeResolver> SubtypeChecker<'a, R> {
    /// Check if two types are equivalent (mutually subtypes).
    ///
    /// Type equivalence means bidirectional subtyping:
    /// `A ≡ B` iff `A <: B` AND `B <: A`
    ///
    /// ## Examples:
    /// - `string` ≡ `string` ✅ (reflexive)
    /// - `A | B` ≡ `B | A` ✅ (union commutes)
    /// - `T & U` ≡ `U & T` ✅ (intersection commutes)
    ///
    /// Note: For most type checking, unidirectional subtyping (`<:`) is used.
    /// Equivalence (`≡`) is primarily for type parameter constraints and exact matching.
    pub(crate) fn types_equivalent(&mut self, left: TypeId, right: TypeId) -> bool {
        self.check_subtype(left, right).is_true() && self.check_subtype(right, left).is_true()
    }

    /// Check if a type parameter is a subtype of a target type.
    ///
    /// Handles both type parameter vs type parameter and type parameter vs concrete type.
    /// Implements TypeScript's soundness rules for type parameter compatibility.
    ///
    /// ## TypeScript Soundness Rules:
    /// - Same type parameter by name → reflexive (always compatible)
    /// - Different type parameters → check constraint transitivity
    /// - Type parameter vs concrete → constraint must be subtype of concrete
    /// - Unconstrained type parameter → acts like `unknown` (top type)
    pub(crate) fn check_type_parameter_subtype(
        &mut self,
        s_info: &TypeParamInfo,
        target: TypeId,
    ) -> SubtypeResult {
        // Type parameter vs type parameter
        if let Some(t_info) = type_param_info(self.interner, target) {
            if s_info.name == t_info.name {
                return SubtypeResult::True;
            }
            if let Some(s_constraint) = s_info.constraint {
                if s_constraint == target {
                    return SubtypeResult::True;
                }
                if self.check_subtype(s_constraint, target).is_true() {
                    return SubtypeResult::True;
                }
            }
            return SubtypeResult::False;
        }

        // Type parameter vs concrete type
        if let Some(constraint) = s_info.constraint {
            return self.check_subtype(constraint, target);
        }

        // Unconstrained type parameter: use {} (empty object) as base constraint.
        // In TypeScript, an unconstrained type parameter's base constraint is {},
        // meaning it can be assigned to any object type with only optional properties.
        let empty_object = self.interner.object(Vec::new());
        self.check_subtype(empty_object, target)
    }

    /// Check subtype with optional method bivariance.
    ///
    /// When `allow_bivariant` is true, temporarily disables strict function types
    /// to allow bivariant parameter checking. This is used for method compatibility
    /// where TypeScript allows bivariance even in strict mode.
    ///
    /// ## Variance Modes:
    /// - **Contravariant (strict)**: `target <: source` - Function parameters in strict mode
    /// - **Bivariant (legacy)**: `target <: source OR source <: target` - Methods, legacy functions
    ///
    /// ## Example:
    /// ```typescript
    /// // Bivariant methods allow unsound but convenient assignments
    /// interface Animal { name: string; }
    /// interface Dog extends Animal { bark(): void; }
    /// class AnimalKeeper {
    ///   feed(animal: Animal) { ... }  // Contravariant parameter
    /// }
    /// class DogKeeper {
    ///   feed(dog: Dog) { ... }  // More specific
    /// }
    /// // DogKeeper.feed is assignable to AnimalKeeper.feed (bivariant)
    /// ```
    pub(crate) fn check_subtype_with_method_variance(
        &mut self,
        source: TypeId,
        target: TypeId,
        allow_bivariant: bool,
    ) -> SubtypeResult {
        if !allow_bivariant {
            return self.check_subtype(source, target);
        }

        // If we're already in bivariant mode, don't nest - just check normally
        // This prevents infinite recursion when methods contain other methods
        if !self.strict_function_types && self.allow_bivariant_param_count {
            return self.check_subtype(source, target);
        }

        let prev = self.strict_function_types;
        let prev_param_count = self.allow_bivariant_param_count;
        self.strict_function_types = false;
        self.allow_bivariant_param_count = true;
        let result = self.check_subtype(source, target);
        self.allow_bivariant_param_count = prev_param_count;
        self.strict_function_types = prev;
        result
    }

    /// Explain failure with method bivariance rules.
    pub(crate) fn explain_failure_with_method_variance(
        &mut self,
        source: TypeId,
        target: TypeId,
        allow_bivariant: bool,
    ) -> Option<SubtypeFailureReason> {
        if !allow_bivariant {
            return self.explain_failure(source, target);
        }

        // If we're already in bivariant mode, don't nest - just check normally
        // This prevents infinite recursion when methods contain other methods
        if !self.strict_function_types && self.allow_bivariant_param_count {
            return self.explain_failure(source, target);
        }

        let prev = self.strict_function_types;
        let prev_param_count = self.allow_bivariant_param_count;
        self.strict_function_types = false;
        self.allow_bivariant_param_count = true;
        let result = self.explain_failure(source, target);
        self.allow_bivariant_param_count = prev_param_count;
        self.strict_function_types = prev;
        result
    }
}
