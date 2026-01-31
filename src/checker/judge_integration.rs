//! Judge Integration for the Checker
//!
//! This module provides integration between the Checker and the Solver's Judge trait.
//! It serves as a bridge during the migration from direct SubtypeChecker/CompatChecker
//! usage to the query-based Judge API.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────┐
//! │   Checker   │  Uses Judge for type queries
//! └──────┬──────┘
//!        │
//!        ▼
//! ┌─────────────┐
//! │    Judge    │  Pure type algebra (is_subtype, evaluate, classify)
//! └──────┬──────┘
//!        │
//!        ▼
//! ┌─────────────┐
//! │   Lawyer    │  TypeScript-specific compatibility (CompatChecker)
//! └─────────────┘
//! ```
//!
//! ## Migration Path
//!
//! 1. Add `with_judge()` helper to CheckerState for scoped Judge usage
//! 2. Refactor `is_subtype_of` to use `Judge.is_subtype()`
//! 3. Keep `CompatChecker` for assignability (Lawyer layer)
//! 4. Eventually integrate Salsa for memoization

use crate::checker::state::CheckerState;
use crate::solver::TypeId;
use crate::solver::judge::{DefaultJudge, Judge, JudgeConfig};

// =============================================================================
// Judge Integration for CheckerState
// =============================================================================

impl<'a> CheckerState<'a> {
    /// Execute a closure with a configured Judge instance.
    ///
    /// The Judge provides pure type algebra operations (is_subtype, evaluate, etc.)
    /// without TypeScript-specific quirks. For assignability checking with TS rules,
    /// use `is_assignable_to` which goes through the Lawyer (CompatChecker) layer.
    ///
    /// ## Example
    ///
    /// ```ignore
    /// // Check subtype relationship
    /// let is_sub = self.with_judge(|judge| judge.is_subtype(source, target));
    ///
    /// // Classify type for iteration
    /// let kind = self.with_judge(|judge| judge.classify_iterable(type_id));
    /// ```
    pub fn with_judge<R, F>(&self, f: F) -> R
    where
        F: FnOnce(&DefaultJudge<'_>) -> R,
    {
        let env = self.ctx.type_env.borrow();
        let config = JudgeConfig {
            strict_null_checks: self.ctx.strict_null_checks(),
            strict_function_types: self.ctx.strict_function_types(),
            exact_optional_property_types: self.ctx.exact_optional_property_types(),
            no_unchecked_indexed_access: self.ctx.no_unchecked_indexed_access(),
        };
        let judge = DefaultJudge::new(self.ctx.types, &*env, config);
        f(&judge)
    }

    /// Check if source is a subtype of target using the Judge.
    ///
    /// This is the pure type algebra check without TypeScript-specific rules.
    /// For assignability with TS rules, use `is_assignable_to`.
    pub fn judge_is_subtype(&self, source: TypeId, target: TypeId) -> bool {
        self.with_judge(|judge| judge.is_subtype(source, target))
    }

    /// Evaluate a type using the Judge.
    ///
    /// Expands meta-types (conditionals, mapped types, etc.) to their concrete forms.
    pub fn judge_evaluate(&self, type_id: TypeId) -> TypeId {
        self.with_judge(|judge| judge.evaluate(type_id))
    }

    /// Classify a type's iterable kind using the Judge.
    ///
    /// Returns information about how to iterate over this type (array, string, tuple, etc.).
    pub fn judge_classify_iterable(&self, type_id: TypeId) -> crate::solver::judge::IterableKind {
        self.with_judge(|judge| judge.classify_iterable(type_id))
    }

    /// Classify a type's callable kind using the Judge.
    ///
    /// Returns information about how to call this type (function, constructor, etc.).
    pub fn judge_classify_callable(&self, type_id: TypeId) -> crate::solver::judge::CallableKind {
        self.with_judge(|judge| judge.classify_callable(type_id))
    }

    /// Get a property from a type using the Judge.
    ///
    /// Returns information about the property if found.
    pub fn judge_get_property(
        &self,
        type_id: TypeId,
        name: crate::interner::Atom,
    ) -> crate::solver::judge::PropertyResult {
        self.with_judge(|judge| judge.get_property(type_id, name))
    }

    /// Classify a type's truthiness using the Judge.
    ///
    /// Returns whether the type is always truthy, always falsy, or sometimes either.
    pub fn judge_classify_truthiness(
        &self,
        type_id: TypeId,
    ) -> crate::solver::judge::TruthinessKind {
        self.with_judge(|judge| judge.classify_truthiness(type_id))
    }

    /// Get primitive flags for a type using the Judge.
    ///
    /// Returns flags indicating if the type is number-like, string-like, etc.
    pub fn judge_classify_primitive(
        &self,
        type_id: TypeId,
    ) -> crate::solver::judge::PrimitiveFlags {
        self.with_judge(|judge| judge.classify_primitive(type_id))
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    // Tests would go here but require full CheckerState setup
    // which is complex. Integration is tested via conformance tests.
}
