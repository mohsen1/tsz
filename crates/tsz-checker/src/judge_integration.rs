//! Judge Integration for the Checker
//!
//! Provides integration between the Checker and the Solver's Judge trait.

use crate::state::CheckerState;
use tsz_solver::TypeId;
use tsz_solver::judge::{DefaultJudge, Judge, JudgeConfig};

impl<'a> CheckerState<'a> {
    /// Execute a closure with a configured Judge instance.
    ///
    /// The Judge provides pure type algebra operations (`is_subtype`, evaluate, etc.)
    /// without TypeScript-specific quirks. For assignability checking with TS rules,
    /// use `is_assignable_to` which goes through the Lawyer (`CompatChecker`) layer.
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
            sound_mode: self.ctx.sound_mode(),
        };
        let judge = DefaultJudge::new(self.ctx.types, &env, config);
        f(&judge)
    }

    /// Evaluate a type using the Judge.
    ///
    /// Expands meta-types (conditionals, mapped types, etc.) to their concrete forms.
    pub fn judge_evaluate(&self, type_id: TypeId) -> TypeId {
        self.with_judge(|judge| judge.evaluate(type_id))
    }
}
