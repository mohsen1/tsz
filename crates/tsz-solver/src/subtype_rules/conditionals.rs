//! Conditional type subtype checking.
//!
//! This module handles subtyping for TypeScript's conditional types:
//! - `T extends U ? X : Y`
//! - Distributive conditional types
//! - Branch compatibility checking

use crate::types::*;

use super::super::{SubtypeChecker, SubtypeResult, TypeResolver};

impl<'a, R: TypeResolver> SubtypeChecker<'a, R> {
    /// Check conditional type to conditional type subtyping.
    ///
    /// Validates that two conditional types are equivalent in their structure and
    /// that their true/false branches are subtype-compatible.
    ///
    /// ## Conditional Type Structure:
    /// ```typescript
    /// T extends U ? X : Y
    /// ```
    ///
    /// ## Subtyping Rules:
    /// 1. **Distributive flags must match**: Both must be distributive or non-distributive
    /// 2. **Check type must be equivalent**: `check_type` parameters must be the same
    /// 3. **Extends type must be equivalent**: `extends_type` must match structurally
    /// 4. **Branch compatibility**: Both true and false branches must be compatible
    ///
    /// ## Examples:
    /// - `T extends string ? number : boolean` ≡ `T extends string ? number : boolean` ✅
    /// - `T extends U ? number` ≢ `T extends U ? string` ❌ (different branches)
    pub(crate) fn check_conditional_subtype(
        &mut self,
        source: &ConditionalType,
        target: &ConditionalType,
    ) -> SubtypeResult {
        if source.is_distributive != target.is_distributive {
            return SubtypeResult::False;
        }

        if !self.types_equivalent(source.check_type, target.check_type) {
            return SubtypeResult::False;
        }

        if !self.types_equivalent(source.extends_type, target.extends_type) {
            return SubtypeResult::False;
        }

        if self
            .check_subtype(source.true_type, target.true_type)
            .is_true()
            && self
                .check_subtype(source.false_type, target.false_type)
                .is_true()
        {
            SubtypeResult::True
        } else {
            SubtypeResult::False
        }
    }

    /// Check if both branches of a conditional type are subtypes of target.
    ///
    /// When checking `T extends U ? X : Y <: target`, we need to verify that:
    /// - Both the true branch (X) and false branch (Y) are subtypes of target
    ///
    /// This is used when the source is a conditional type and we need to check
    /// if it can be used where the target type is expected.
    ///
    /// ## Logic:
    /// - `X <: target` AND `Y <: target` => True
    /// - Otherwise => False
    ///
    /// ## Examples:
    /// ```typescript
    /// // Both branches are strings
    /// type T = boolean extends true ? "yes" : "no";
    /// let x: string = null as T;  // ✅ "yes" <: string and "no" <: string
    ///
    /// // Branches have different types
    /// type U = boolean extends true ? "yes" : 42;
    /// let y: string = null as U;  // ❌ 42 is not <: string
    /// ```
    pub(crate) fn conditional_branches_subtype(
        &mut self,
        cond: &ConditionalType,
        target: TypeId,
    ) -> SubtypeResult {
        if self.check_subtype(cond.true_type, target).is_true()
            && self.check_subtype(cond.false_type, target).is_true()
        {
            SubtypeResult::True
        } else {
            SubtypeResult::False
        }
    }

    /// Check if source is a subtype of both branches of a conditional type.
    ///
    /// When checking `source <: (T extends U ? X : Y)`, we need to verify that:
    /// - Source is a subtype of both the true branch (X) and false branch (Y)
    ///
    /// This is used when the target is a conditional type and we need to check
    /// if the source can be assigned to it regardless of which branch is selected.
    ///
    /// ## Logic:
    /// - `source <: X` AND `source <: Y` => True
    /// - Otherwise => False
    ///
    /// ## Examples:
    /// ```typescript
    /// // Source is compatible with both branches
    /// type T = boolean extends true ? string : number;
    /// let x: T = "hello";  // ❌ "hello" is not <: number
    ///
    /// // Source is `never` (bottom type)
    /// type U = boolean extends true ? string : number;
    /// let y: U = null as never;  // ✅ never <: string and never <: number
    /// ```
    pub(crate) fn subtype_of_conditional_target(
        &mut self,
        source: TypeId,
        target: &ConditionalType,
    ) -> SubtypeResult {
        if self.check_subtype(source, target.true_type).is_true()
            && self.check_subtype(source, target.false_type).is_true()
        {
            SubtypeResult::True
        } else {
            SubtypeResult::False
        }
    }
}
