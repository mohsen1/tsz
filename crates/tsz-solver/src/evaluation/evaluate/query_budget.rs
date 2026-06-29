//! Cross-instance per-query operation budget for [`TypeEvaluator::evaluate`].
//!
//! [`TypeEvaluator::evaluate`]: super::TypeEvaluator::evaluate
//!
//! `MAX_GLOBAL_EVAL_DEPTH` bounds the live call-*stack* depth, but some recursive
//! type families never grow a deep stack: conditional and `infer`-pattern
//! evaluation spin up *fresh* `TypeEvaluator` / `SubtypeChecker` instances
//! mid-relation, each with per-instance cycle/depth/iteration guards reset to
//! zero, and bounce between a handful of types whose identity keeps changing
//! (fresh `infer` placeholders / object shapes), so no per-instance guard ever
//! fires. A recursive generic wrapper applied to a literal/object argument —
//! `type Unbox<T> = T extends Box<infer U> ? Unbox<U> : T` over `Box<2>`, or the
//! standard-library `Awaited<Promise<2>>` — hangs the compile this way.
//!
//! This module provides the cross-instance work bound that mirrors tsc's global
//! `instantiationCount`: [`EvalQueryFrame`] counts *every* `evaluate` operation
//! across all instances within one top-level query (reset when the outermost
//! frame begins) and reports exhaustion so `evaluate` can bail, terminating the
//! runaway regardless of which boundary it bounces through.

use super::TypeEvaluator;
use crate::relations::subtype::TypeResolver;

impl<R: TypeResolver> TypeEvaluator<'_, R> {
    /// Enter one operation of the cross-instance per-query budget.
    ///
    /// Returns the live [`EvalQueryFrame`] guard while within budget. When the
    /// budget is exhausted it flags the evaluator's recursion-bail state
    /// (`deep_recursion_seen` / `silent_depth_bailed`, matching the structural
    /// depth-bailout arms) and returns `None`, signalling the caller to leave the
    /// type opaque so the cross-instance runaway unwinds instead of hanging.
    pub(super) fn enter_eval_query_budget(&mut self) -> Option<EvalQueryFrame> {
        let frame = EvalQueryFrame::enter(resolved_max_eval_ops());
        if frame.budget_state.is_exhausted() {
            self.mark_deep_recursion_seen();
            self.mark_silent_depth_bailed();
            return None;
        }
        Some(frame)
    }
}

// The live-frame and op counters live in the consolidated `crate::limits`
// thread-local budget state (issue #13091): the op counter is reset whenever
// the live-frame count transitions from `0`, so the budget is per top-level
// query and never carries over to poison sibling type positions. See
// [`DEFAULT_MAX_EVAL_OPS_PER_QUERY`].

/// Total `evaluate` operations permitted for a single top-level evaluation query
/// (the outermost `evaluate` call on the thread, before it returns).
///
/// Defaults to the whole-file `MAX_EVALUATION_FUEL` (`2_000_000`): a single
/// top-level query that out-works the entire file's evaluation-fuel budget is,
/// by construction, a runaway. This keeps the guard strictly weaker than the
/// existing whole-file fuel for terminating evaluations (which never approach
/// it), so it changes behaviour only for the cross-instance recursions that the
/// fuel's per-128-iteration sampling fails to observe.
///
/// Overridable via the `TSZ_MAX_EVAL_OPS` environment variable, which tests use
/// to force the bail quickly without a multi-million-op spin. See
/// [`resolved_max_eval_ops`].
pub(super) const DEFAULT_MAX_EVAL_OPS_PER_QUERY: u32 =
    crate::limits::DEFAULT_MAX_EVAL_OPS_PER_QUERY;

/// Resolve the per-query `evaluate` operation budget, honoring the
/// `TSZ_MAX_EVAL_OPS` override.
///
/// Defaults to [`DEFAULT_MAX_EVAL_OPS_PER_QUERY`]. The override exists so tests
/// can force the cross-instance runaway bail at a small budget instead of
/// spinning through two million operations; a value of `0` (or an unparseable
/// value) falls back to the default.
pub(super) fn resolved_max_eval_ops() -> u32 {
    use std::sync::OnceLock;
    static OVERRIDE: OnceLock<Option<u32>> = OnceLock::new();
    let configured = *OVERRIDE.get_or_init(|| {
        std::env::var("TSZ_MAX_EVAL_OPS")
            .ok()
            .and_then(|v| v.trim().parse::<u32>().ok())
            .filter(|&v| v > 0)
    });
    configured.unwrap_or(DEFAULT_MAX_EVAL_OPS_PER_QUERY)
}

/// RAII frame that maintains the thread-local per-query `evaluate` operation
/// budget across every `TypeEvaluator` instance.
///
/// On construction it increments the live frame count (resetting the op counter
/// when starting a fresh top-level query) and bumps the op counter, recording
/// whether the budget is now exhausted. On drop it decrements the live frame
/// count, so the bound is restored on every return path, including panics.
pub(super) struct EvalQueryFrame {
    /// Whether this frame entered within the operation budget or exhausted it.
    budget_state: EvalQueryBudgetState,
}

/// Solver-owned verdict for one `evaluate` operation's per-query budget entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum EvalQueryBudgetState {
    WithinBudget,
    Exhausted,
}

impl EvalQueryBudgetState {
    const fn from_exhausted(exhausted: bool) -> Self {
        if exhausted {
            Self::Exhausted
        } else {
            Self::WithinBudget
        }
    }

    const fn is_exhausted(self) -> bool {
        matches!(self, Self::Exhausted)
    }
}

impl EvalQueryFrame {
    #[inline]
    pub(super) fn enter(max_ops: u32) -> Self {
        // Single consolidated TLS access bumps the live frame count, resets
        // the op counter on a fresh top-level query, and bumps the op count.
        let entry = crate::limits::eval_query_enter();
        if entry.began_top_level_query {
            // A fresh top-level query begins: drop any cross-evaluator result
            // memo from the previous query so results never leak across queries,
            // threads, or files (#11586).
            crate::evaluation::cross_eval_guard::reset_query_memo();
        }
        Self {
            budget_state: EvalQueryBudgetState::from_exhausted(entry.ops > max_ops),
        }
    }
}

impl Drop for EvalQueryFrame {
    #[inline]
    fn drop(&mut self) {
        crate::limits::eval_query_leave();
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DEFAULT_MAX_EVAL_OPS_PER_QUERY, EvalQueryBudgetState, EvalQueryFrame, resolved_max_eval_ops,
    };
    use crate::limits::{eval_query_active, eval_query_ops};

    #[test]
    fn budget_state_names_entry_verdict() {
        assert_eq!(
            EvalQueryBudgetState::from_exhausted(false),
            EvalQueryBudgetState::WithinBudget
        );
        assert_eq!(
            EvalQueryBudgetState::from_exhausted(true),
            EvalQueryBudgetState::Exhausted
        );
    }

    /// The per-query operation counter resets when a fresh top-level query
    /// begins (live frame count returns to zero), so one type position can never
    /// carry its op count into the next.
    #[test]
    fn op_counter_resets_per_top_level_query() {
        {
            let _f1 = EvalQueryFrame::enter(1000);
            let _f2 = EvalQueryFrame::enter(1000);
            let _f3 = EvalQueryFrame::enter(1000);
            assert_eq!(eval_query_ops(), 3);
            assert_eq!(eval_query_active(), 3);
        }
        // All frames dropped -> live count back to zero.
        assert_eq!(eval_query_active(), 0);

        // Second top-level query starts fresh: op counter reset to 1, not 4.
        let _f = EvalQueryFrame::enter(1000);
        assert_eq!(eval_query_ops(), 1);
    }

    /// Once the budget is exceeded within a single query, the frame reports
    /// exhaustion so `evaluate` can bail; nested frames keep reporting it until
    /// the query unwinds.
    #[test]
    fn budget_exhaustion_is_reported_until_query_unwinds() {
        let f1 = EvalQueryFrame::enter(2);
        assert_eq!(f1.budget_state, EvalQueryBudgetState::WithinBudget);
        let f2 = EvalQueryFrame::enter(2);
        assert_eq!(f2.budget_state, EvalQueryBudgetState::WithinBudget);
        let f3 = EvalQueryFrame::enter(2);
        assert_eq!(f3.budget_state, EvalQueryBudgetState::Exhausted);
        let f4 = EvalQueryFrame::enter(2);
        assert_eq!(f4.budget_state, EvalQueryBudgetState::Exhausted);
        drop((f1, f2, f3, f4));
        assert_eq!(eval_query_active(), 0);
    }

    /// With no override set the resolved budget is the default.
    #[test]
    fn budget_defaults_when_env_unset() {
        assert_eq!(resolved_max_eval_ops(), DEFAULT_MAX_EVAL_OPS_PER_QUERY);
    }
}
